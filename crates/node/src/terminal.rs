//! A shell for a client (ADR-0054): on `OpenTerminal` the node starts the owner's login
//! shell under a PTY in their home directory and dials `TerminalStream` back to the hub,
//! naming the terminal. What the client types goes into the shell, a resize resizes the
//! PTY, and the shell's output goes up. The stream ending ends the shell; the shell ending
//! says why and ends the stream. Only with `--terminal`: a shell is the owner's whole
//! account on this machine.

use std::io::{Read, Write};
use std::sync::Arc;

use portable_pty::{CommandBuilder, PtySize, native_pty_system};
use saarthi_proto::node::v1::{OpenTerminal, TerminalIn, TerminalOut};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

use crate::claude_api::HubApi;

/// Output is sent up in pieces of at most this.
const PIECE: usize = 64 * 1024;

/// Serves one `OpenTerminal`. `shell` is `--terminal`: None for no terminal, which the
/// stream back says; empty for the owner's login shell; else that program (tests use one).
pub async fn open(api: Arc<HubApi>, open: OpenTerminal, shell: Option<String>) {
    let (up, up_rx) = mpsc::channel::<TerminalOut>(64);
    let _ = up.send(TerminalOut { id: open.id.clone(), ..Default::default() }).await;
    let mut down = match api.terminal_stream(ReceiverStream::new(up_rx)).await {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!(id = %open.id, error = %e, "the hub did not take a terminal stream");
            return;
        }
    };
    let Some(shell) = shell else {
        return end(up, down, "this node offers no terminal: it runs without --terminal".into()).await;
    };
    let size = PtySize { rows: open.rows.max(1) as u16, cols: open.cols.max(1) as u16, pixel_width: 0, pixel_height: 0 };
    let pty = match native_pty_system().openpty(size) {
        Ok(p) => p,
        Err(e) => return end(up, down, format!("could not open a PTY: {e}")).await,
    };
    // The owner's shell as a login shell, in their home directory; where the node runs when
    // that is not a directory (unset, or a build sandbox's `/homeless-shelter`).
    let mut cmd = if shell.is_empty() { CommandBuilder::new_default_prog() } else { CommandBuilder::new(&shell) };
    // portable-pty would otherwise start it in `$HOME` itself, and fail there.
    match std::env::var_os("HOME").filter(|h| std::path::Path::new(h).is_dir()) {
        Some(home) => cmd.cwd(home),
        None => cmd.cwd(std::env::current_dir().unwrap_or_else(|_| "/".into())),
    }
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    // Nothing of the node's own Claude sessions leaks into the shell.
    for (name, _) in std::env::vars_os() {
        let name = name.to_string_lossy();
        if name.starts_with("CLAUDE") || name == "AI_AGENT" {
            cmd.env_remove(name.as_ref());
        }
    }
    let mut child = match pty.slave.spawn_command(cmd) {
        Ok(c) => c,
        Err(e) => return end(up, down, format!("could not start the shell: {e}")).await,
    };
    drop(pty.slave);
    let mut killer = child.clone_killer();
    let (Ok(mut reader), Ok(mut writer)) = (pty.master.try_clone_reader(), pty.master.take_writer()) else {
        let _ = killer.kill();
        return end(up, down, "could not read the PTY".into()).await;
    };
    tracing::info!(id = %open.id, cols = open.cols, rows = open.rows, "a terminal started");

    // The shell's output, up as it comes; then, once it exits, why.
    let output = up.clone();
    let read = tokio::task::spawn_blocking(move || {
        let mut buf = vec![0u8; PIECE];
        loop {
            match reader.read(&mut buf) {
                Ok(0) | Err(_) => return,
                Ok(n) => {
                    if output.blocking_send(TerminalOut { output: buf[..n].to_vec(), ..Default::default() }).is_err() {
                        return;
                    }
                }
            }
        }
    });
    let exit = tokio::task::spawn_blocking(move || child.wait());

    // What the client sends, into the shell, until the stream ends or the shell does.
    let master = pty.master;
    tokio::pin!(exit);
    let status = loop {
        tokio::select! {
            m = down.message() => match m {
                Ok(Some(m)) => {
                    if m.cols > 0 && m.rows > 0 {
                        // With the pixels, which programs that draw images read (yazi).
                        let px = |v: u32| u16::try_from(v).unwrap_or(0);
                        let _ = master.resize(PtySize {
                            rows: m.rows as u16,
                            cols: m.cols as u16,
                            pixel_width: px(m.width_px),
                            pixel_height: px(m.height_px),
                        });
                    }
                    if !m.input.is_empty() && writer.write_all(&m.input).and_then(|()| writer.flush()).is_err() {
                        break None;
                    }
                }
                // The client went away: the shell goes with it.
                Ok(None) | Err(_) => {
                    let _ = killer.kill();
                    tracing::info!(id = %open.id, "a terminal closed by its client");
                    return;
                }
            },
            s = &mut exit => break s.ok().and_then(Result::ok),
        }
    };
    // Whatever the shell wrote before it exited goes up before the reason does.
    let _ = read.await;
    let why = match status {
        Some(s) if s.success() => "the shell exited".to_string(),
        Some(s) => format!("the shell exited with {}", s.exit_code()),
        None => "the shell's input closed".to_string(),
    };
    tracing::info!(id = %open.id, %why, "a terminal ended");
    end(up, down, why).await;
}

/// Says why the terminal ended. Dropping the call at once would cancel it before the reason
/// went out: it is held until the hub, having passed the reason on, closes it.
async fn end(up: mpsc::Sender<TerminalOut>, mut down: tonic::Streaming<TerminalIn>, why: String) {
    let _ = up.send(TerminalOut { ended: why, ..Default::default() }).await;
    drop(up);
    let _ = tokio::time::timeout(std::time::Duration::from_secs(5), async { while let Ok(Some(_)) = down.message().await {} }).await;
}
