//! `saarthi run`: Claude Code under a PTY this process owns, mirrored to the terminal it was
//! started from, so the daemon can interrupt or kill the session and learns when a person
//! types at it (EXP-0003). The wrapper never reads meaning from the terminal output; hooks
//! and the transcript stay the only sources of truth.
//!
//! Plain threads rather than async: every stream here is a blocking file descriptor.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, PtySize, native_pty_system};

use crate::wrapper::{FromWrapper, ToWrapper, TokenReply};

/// The interrupt sequence that EXP-0003 found to work: Escape stops the turn, Claude Code
/// then refills the input box with the interrupted prompt, Ctrl-U empties it, and a
/// follow-up prompt is typed and submitted. The pauses let the TUI settle between steps.
const AFTER_ESCAPE: Duration = Duration::from_secs(2);
const AFTER_CLEAR: Duration = Duration::from_secs(1);
const BEFORE_ENTER: Duration = Duration::from_secs(1);
/// Keystroke notices are rate-limited; one per burst of typing is enough.
const KEYSTROKE_EVERY: Duration = Duration::from_millis(250);
/// How long a killed process gets between SIGTERM and SIGKILL.
const KILL_GRACE: Duration = Duration::from_secs(5);
/// How often a wrapper without a daemon tries to reach one again, so a session outlives
/// the node's restarts and stays reachable from the app.
const RECONNECT_EVERY: Duration = Duration::from_secs(2);

pub struct Options {
    pub command: String,
    pub args: Vec<String>,
    /// The daemon's wrapper socket.
    pub socket: PathBuf,
    /// Which Claude account the program runs under (ADR-0046).
    pub account: Account,
}

/// The Claude account a wrapped program runs under.
#[derive(Clone, Debug, PartialEq)]
pub enum Account {
    /// The node's default, as the hub holds it; the machine's own login when it has none
    /// or the node cannot be asked.
    NodeDefault,
    /// One of the hub's named tokens; the wrapper refuses to start without it.
    Named(String),
    /// The machine's own login.
    OwnLogin,
}

/// How long the daemon gets to fetch a token from the hub.
const TOKEN_WAIT: Duration = Duration::from_secs(15);

/// The name of the hub's token for `account` and the socket to send the program's API
/// requests to (ADR-0051), asked of the daemon, which asks the hub. Ok(None) is the
/// machine's own login. A named token that cannot be had is an error; the default falls
/// back to the own login, saying why when something failed rather than there being no
/// default.
pub fn account_token(socket: &std::path::Path, account: &Account) -> Result<Option<(String, String)>> {
    let name = match account {
        Account::OwnLogin => return Ok(None),
        Account::NodeDefault => None,
        Account::Named(name) => Some(name.clone()),
    };
    let asked = (|| -> Result<TokenReply> {
        let mut s = UnixStream::connect(socket)
            .with_context(|| format!("no node daemon at {}", socket.display()))?;
        s.set_read_timeout(Some(TOKEN_WAIT))?;
        send(&mut s, &FromWrapper::ClaudeToken { name: name.clone() })?;
        let mut line = String::new();
        BufReader::new(s).read_line(&mut line).context("the node daemon did not answer")?;
        serde_json::from_str(&line).context("the node daemon's answer")
    })();
    let failed = match asked {
        Ok(TokenReply { name, socket: Some(api), error: None }) => return Ok(Some((name, api))),
        Ok(TokenReply { error: None, .. }) if name.is_none() => return Ok(None),
        Ok(TokenReply { error: Some(e), .. }) => e,
        Ok(_) => "the hub sent no token".to_string(),
        Err(e) => format!("{e:#}"),
    };
    match name {
        Some(name) => anyhow::bail!("no Claude token {name}: {failed}"),
        None => {
            eprintln!("saarthi run: the node's default Claude account could not be had ({failed}); running under this machine's own login");
            Ok(None)
        }
    }
}

type Writer = Arc<Mutex<Box<dyn Write + Send>>>;
/// The link to the daemon while there is one, for keystroke notices.
type Link = Arc<Mutex<Option<UnixStream>>>;

/// Runs the command under a PTY until it exits; returns its exit code.
pub fn run(opts: Options) -> Result<i32> {
    let account = account_token(&opts.socket, &opts.account)?;
    let pty = native_pty_system()
        .openpty(terminal_size())
        .context("opening a PTY")?;
    let mut cmd = CommandBuilder::new(&opts.command);
    if account.is_some() && wants_auto(&opts.command, &opts.args) {
        // The dummy cannot read the account's default mode ([[EXP-20261114-d82rcz]]).
        cmd.args(["--permission-mode", "auto"]);
    }
    cmd.args(&opts.args);
    cmd.cwd(std::env::current_dir()?);
    // A session started from inside another must not be taken for its child.
    for (name, _) in std::env::vars_os() {
        let name = name.to_string_lossy();
        if name.starts_with("CLAUDE") || name == "AI_AGENT" {
            cmd.env_remove(name.as_ref());
        }
    }
    // A token in the environment wins over the login in the keychain (EXP-20261111-q6sgt8):
    // here a dummy, and the program's API requests go to the daemon's socket and on to the
    // hub, which adds the token (ADR-0051).
    let (token_name, api_socket) = match &account {
        Some((name, api)) => {
            for (k, v) in crate::claude_api::env(std::path::Path::new(api)) {
                cmd.env(k, v);
            }
            eprintln!("saarthi run: Claude account {name}\r");
            (name.clone(), api.clone())
        }
        None => (String::new(), String::new()),
    };
    let mut child = pty
        .slave
        .spawn_command(cmd)
        .with_context(|| format!("starting {}", opts.command))?;
    drop(pty.slave);
    let pid = child.process_id().context("the child has no pid")?;
    let writer: Writer = Arc::new(Mutex::new(pty.master.take_writer()?));
    let mut reader = pty.master.try_clone_reader()?;
    let _raw = RawMode::enter();

    // The PTY's output goes to our terminal as it comes.
    std::thread::spawn(move || {
        let mut out = std::io::stdout();
        let mut buf = [0u8; 8192];
        while let Ok(n) = reader.read(&mut buf) {
            if n == 0 || out.write_all(&buf[..n]).and_then(|()| out.flush()).is_err() {
                break;
            }
        }
    });

    let link: Link = Arc::new(Mutex::new(None));
    {
        let (socket, writer, link) = (opts.socket.clone(), writer.clone(), link.clone());
        std::thread::spawn(move || follow_daemon(&socket, pid, &token_name, &api_socket, &writer, &link));
    }

    // Our terminal's input goes to the PTY; a person typing is reported to the daemon.
    {
        let writer = writer.clone();
        std::thread::spawn(move || {
            let mut input = std::io::stdin();
            let mut buf = [0u8; 4096];
            let mut last = Instant::now() - KEYSTROKE_EVERY;
            while let Ok(n) = input.read(&mut buf) {
                if n == 0 {
                    break;
                }
                let typed = &buf[..n];
                if writer.lock().unwrap().write_all(typed).is_err() {
                    break;
                }
                if is_human(typed) && last.elapsed() >= KEYSTROKE_EVERY {
                    last = Instant::now();
                    let mut daemon = link.lock().unwrap();
                    if let Some(stream) = daemon.as_mut()
                        && send(stream, &FromWrapper::Keystroke).is_err()
                    {
                        *daemon = None;
                    }
                }
            }
        });
    }

    // Follow the terminal's size; polling avoids a signal handler for SIGWINCH.
    {
        let master = pty.master;
        std::thread::spawn(move || {
            let mut size = terminal_size();
            loop {
                std::thread::sleep(Duration::from_millis(250));
                let now = terminal_size();
                if (now.rows, now.cols) != (size.rows, size.cols) {
                    size = now;
                    let _ = master.resize(size);
                }
            }
        });
    }

    let status = child.wait()?;
    Ok(status.exit_code() as i32)
}

/// Says hello to the daemon and follows its commands for as long as the process runs:
/// when the daemon goes away (a restart, an upgrade) the wrapper says hello again to the
/// next one, which otherwise would take the session for one started by hand. Without a
/// daemon the session runs anyway, unreachable until one appears.
fn follow_daemon(
    socket: &std::path::Path,
    pid: u32,
    claude_token: &str,
    api_socket: &str,
    writer: &Writer,
    link: &Link,
) {
    let mut warned = false;
    loop {
        let attempt = UnixStream::connect(socket).and_then(|mut s| {
            send(
                &mut s,
                &FromWrapper::Hello {
                    pid,
                    claude_token: claude_token.to_string(),
                    api_socket: api_socket.to_string(),
                },
            )?;
            Ok((s.try_clone()?, s))
        });
        match attempt {
            Ok((commands, notices)) => {
                *link.lock().unwrap() = Some(notices);
                follow_commands(commands, writer, pid);
                *link.lock().unwrap() = None;
            }
            Err(e) if !warned => {
                eprintln!(
                    "saarthi run: no node daemon at {} ({e}); this session cannot be reached remotely until one starts\r",
                    socket.display()
                );
                warned = true;
            }
            Err(_) => {}
        }
        std::thread::sleep(RECONNECT_EVERY);
    }
}

/// Whether `saarthi run` adds `--permission-mode auto` before the program's arguments: when
/// the program is Claude Code and the arguments choose no permission mode of their own.
pub fn wants_auto(command: &str, args: &[String]) -> bool {
    let program = std::path::Path::new(command).file_name().and_then(|n| n.to_str());
    program == Some("claude")
        && !args.iter().any(|a| {
            a == "--permission-mode"
                || a.starts_with("--permission-mode=")
                || a == "--dangerously-skip-permissions"
                || a == "--allow-dangerously-skip-permissions"
        })
}

fn send(stream: &mut UnixStream, msg: &FromWrapper) -> std::io::Result<()> {
    let mut line = serde_json::to_string(msg)?;
    line.push('\n');
    stream.write_all(line.as_bytes())
}

/// Carries out the daemon's commands until it closes the link.
fn follow_commands(stream: UnixStream, writer: &Writer, pid: u32) {
    for line in BufReader::new(stream).lines() {
        let Ok(line) = line else { return };
        match serde_json::from_str::<ToWrapper>(&line) {
            Ok(ToWrapper::Interrupt { then_prompt }) => {
                let type_ = |bytes: &[u8]| {
                    let mut w = writer.lock().unwrap();
                    let _ = w.write_all(bytes).and_then(|()| w.flush());
                };
                type_(b"\x1b");
                std::thread::sleep(AFTER_ESCAPE);
                type_(b"\x15");
                if let Some(text) = then_prompt.filter(|t| !t.is_empty()) {
                    std::thread::sleep(AFTER_CLEAR);
                    type_(text.as_bytes());
                    std::thread::sleep(BEFORE_ENTER);
                    type_(b"\r");
                }
            }
            Ok(ToWrapper::Prompt { text }) => {
                let mut w = writer.lock().unwrap();
                let _ = w.write_all(text.as_bytes()).and_then(|()| w.flush());
                std::thread::sleep(BEFORE_ENTER);
                let _ = w.write_all(b"\r").and_then(|()| w.flush());
            }
            Ok(ToWrapper::Kill) => {
                let pid = pid as libc::pid_t;
                unsafe { libc::kill(pid, libc::SIGTERM) };
                std::thread::spawn(move || {
                    std::thread::sleep(KILL_GRACE);
                    if unsafe { libc::kill(pid, 0) } == 0 {
                        unsafe { libc::kill(pid, libc::SIGKILL) };
                    }
                });
            }
            Err(_) => {}
        }
    }
}

/// Whether terminal input holds anything a person typed. Terminals answer a program's
/// queries (cursor position, device attributes, version, mode reports) and announce focus
/// changes on the same input stream; those must not count as a keystroke, or every held
/// request would be released as soon as Claude Code redraws.
pub fn is_human(input: &[u8]) -> bool {
    let mut i = 0;
    while i < input.len() {
        if input[i] != 0x1b {
            return true;
        }
        let Some(&kind) = input.get(i + 1) else {
            return true; // the Escape key
        };
        match kind {
            b'[' => {
                let start = i + 2;
                let mut j = start;
                while j < input.len() && (0x20..=0x3f).contains(&input[j]) {
                    j += 1;
                }
                let Some(&last) = input.get(j) else {
                    return true;
                };
                let params = &input[start..j];
                let report = matches!(params.first(), Some(b'?' | b'>' | b'='))
                    || matches!(last, b'R' | b'n' | b't' | b'c')
                    || (matches!(last, b'I' | b'O') && params.is_empty());
                if !report {
                    return true; // arrows, function keys, pastes, mouse
                }
                i = j + 1;
            }
            // OSC, DCS, APC and PM strings run to BEL or ESC \ and are always replies.
            b']' | b'P' | b'_' | b'^' => {
                let mut j = i + 2;
                while j < input.len()
                    && input[j] != 0x07
                    && !(input[j] == 0x1b && input.get(j + 1) == Some(&b'\\'))
                {
                    j += 1;
                }
                i = if input.get(j) == Some(&0x07) {
                    j + 1
                } else {
                    j + 2
                };
            }
            _ => return true, // Alt with a key
        }
    }
    false
}

fn terminal_size() -> PtySize {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    let ok = unsafe { libc::ioctl(libc::STDOUT_FILENO, libc::TIOCGWINSZ, &mut ws) } == 0;
    if ok && ws.ws_row > 0 && ws.ws_col > 0 {
        PtySize {
            rows: ws.ws_row,
            cols: ws.ws_col,
            pixel_width: ws.ws_xpixel,
            pixel_height: ws.ws_ypixel,
        }
    } else {
        PtySize::default()
    }
}

/// The terminal in raw mode while the wrapper runs, restored on drop. Nothing to do when
/// stdin is not a terminal.
struct RawMode(Option<libc::termios>);

impl RawMode {
    fn enter() -> Self {
        let mut saved: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(libc::STDIN_FILENO, &mut saved) } != 0 {
            return Self(None);
        }
        let mut raw = saved;
        unsafe {
            libc::cfmakeraw(&mut raw);
            libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &raw);
        }
        Self(Some(saved))
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        if let Some(saved) = &self.0 {
            unsafe { libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, saved) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{is_human, wants_auto};

    #[test]
    fn claude_starts_in_auto_unless_its_arguments_choose_a_mode() {
        let args = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(wants_auto("claude", &[]));
        assert!(wants_auto("/nix/store/x/bin/claude", &args(&["--resume", "abc"])));
        assert!(!wants_auto("claude", &args(&["--permission-mode", "plan"])));
        assert!(!wants_auto("claude", &args(&["--permission-mode=plan"])));
        assert!(!wants_auto("claude", &args(&["--dangerously-skip-permissions"])));
        assert!(!wants_auto("sh", &[]));
    }

    #[test]
    fn typing_counts_and_terminal_replies_do_not() {
        for typed in [
            &b"y"[..],
            b"\r",
            b"\x1b",                 // Escape
            b"\x1b[A",               // arrow up
            b"\x1b[15~",             // F5
            b"\x1b[200~hi\x1b[201~", // a paste
            b"\x1b[<0;10;5M",        // a mouse click
            b"\x1bb",                // Alt-b
            b"\x1b[I\x1b[A",         // focus, then an arrow
            b"\x1b[97;5u",           // a key in the kitty keyboard protocol
        ] {
            assert!(is_human(typed), "{typed:?}");
        }
        for reply in [
            &b"\x1b[12;40R"[..],               // cursor position
            b"\x1b[?62;22c",                   // device attributes
            b"\x1b[>1;10;0c",                  // secondary device attributes
            b"\x1b[0n",                        // status
            b"\x1b[I",                         // focus in
            b"\x1b[O",                         // focus out
            b"\x1b[?1u",                       // kitty keyboard flags
            b"\x1b[?2026;2$y",                 // mode report
            b"\x1bP>|iTerm2 3.6\x1b\\",        // terminal version
            b"\x1b]11;rgb:1e1e/1e1e/1e1e\x07", // background colour
            b"\x1b[?62;22c\x1b[12;40R\x1b[I",  // several at once
        ] {
            assert!(!is_human(reply), "{reply:?}");
        }
    }
}
