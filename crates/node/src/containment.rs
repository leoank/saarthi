//! Launch profiles and the containment a spawned session runs under (ADR-0038, from
//! ADR-0017). A profile is named in the node's profiles file and picked at spawn time; the
//! empty name is the host, where a session runs as before with the user's own login. A
//! sandboxed profile runs claude under mxc's seatbelt backend on macOS with:
//!
//! - its own state directory: Claude's config directory (`claude/`, which receives the
//!   registry entry and transcripts), a scratch directory and a home, all read-write;
//! - the long-lived token of `claude setup-token`: the named token the hub sent with the
//!   command (ADR-0039), else the profile's `token_file` read at each start; never written
//!   anywhere (EXP-20261107-413q6m), and never inside: claude there holds a dummy and sends
//!   its API requests over a unix socket to the node's proxy, which adds the token
//!   (ADR-0050, `claude_api`);
//! - the working directory read-write, `/nix/store` and the profile's paths read-only,
//!   the rest of the home directory unreadable;
//! - outbound network, no inbound and no loopback TCP: hooks come back over the node's unix
//!   socket, and the other sessions' sockets and the wrappers' socket are denied
//!   (EXP-20261107-413q82).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// The base `PATH` entries every sandboxed session gets after the profile's own.
const SYSTEM_PATH: [&str; 4] = ["/usr/bin", "/bin", "/usr/sbin", "/sbin"];

/// Other sessions' messaging sockets: a sandboxed session must not prompt them.
const CC_SOCKS: &str = "/private/tmp/cc-socks";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Backend {
    /// No containment: the user's own privileges, as for the empty profile.
    Host,
    /// mxc's seatbelt backend, macOS only.
    Seatbelt,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    #[serde(skip)]
    pub name: String,
    pub backend: Backend,
    /// One line for the app's chooser.
    #[serde(default)]
    pub description: String,
    /// Read-write; holds `claude/` (the config directory), `tmp/` and `home/`.
    #[serde(default)]
    pub state_dir: Option<PathBuf>,
    /// A file holding the token `claude setup-token` printed, used when the hub sends none
    /// (ADR-0039: until every node's default is set at the hub).
    #[serde(default)]
    pub token_file: Option<PathBuf>,
    /// `PATH` inside, before the system directories.
    #[serde(default)]
    pub path: Vec<String>,
    #[serde(default)]
    pub readonly: Vec<PathBuf>,
    #[serde(default)]
    pub readwrite: Vec<PathBuf>,
    #[serde(default)]
    pub denied: Vec<PathBuf>,
    /// Entries of the user's `~/.claude` linked into the profile's config directory and
    /// readable inside, such as `CLAUDE.md`, `skills`, `commands`, `agents`.
    #[serde(default)]
    pub claude_files: Vec<String>,
    /// More environment, after the node's own variables, so a profile can override them.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    profiles: BTreeMap<String, Profile>,
}

/// The profiles a node offers, by name.
#[derive(Debug, Default, Clone)]
pub struct Profiles(BTreeMap<String, Profile>);

impl Profiles {
    /// Reads a profiles file; `~/` in any path is the user's home. Refuses what could not
    /// run: an unknown field, seatbelt off macOS, a sandbox without a state directory, two
    /// profiles sharing a state directory (a session's profile is found again from where its
    /// transcript lives). A sandbox without `token_file` runs only under a token the hub
    /// sends.
    pub fn load(path: &Path, home: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Self::parse(&text, home).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn parse(text: &str, home: &Path) -> Result<Self, String> {
        let file: File = toml::from_str(text).map_err(|e| e.to_string())?;
        let mut dirs = BTreeSet::new();
        let mut out = BTreeMap::new();
        for (name, mut p) in file.profiles {
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                return Err(format!("profile name {name:?}: letters, digits, - and _ only"));
            }
            p.name = name.clone();
            let tilde = |p: &PathBuf| expand(p, home);
            p.state_dir = p.state_dir.as_ref().map(tilde);
            p.token_file = p.token_file.as_ref().map(tilde);
            p.readonly = p.readonly.iter().map(tilde).collect();
            p.readwrite = p.readwrite.iter().map(tilde).collect();
            p.denied = p.denied.iter().map(tilde).collect();
            p.path = p.path.iter().map(|s| expand(Path::new(s), home).to_string_lossy().into_owned()).collect();
            p.env = p
                .env
                .into_iter()
                .map(|(k, v)| (k, expand(Path::new(&v), home).to_string_lossy().into_owned()))
                .collect();
            if p.backend == Backend::Seatbelt {
                if !cfg!(target_os = "macos") {
                    return Err(format!("profile {name}: seatbelt runs only on macOS"));
                }
                let Some(state) = &p.state_dir else {
                    return Err(format!("profile {name}: a sandbox needs state_dir"));
                };
                if !state.is_absolute() {
                    return Err(format!("profile {name}: state_dir must be absolute"));
                }
                if !dirs.insert(state.clone()) {
                    return Err(format!("profile {name}: another profile has the same state_dir"));
                }
            }
            out.insert(name, p);
        }
        Ok(Self(out))
    }

    /// The profile by name; the empty name is the host and has none.
    pub fn get(&self, name: &str) -> Result<Option<&Profile>, String> {
        if name.is_empty() {
            return Ok(None);
        }
        match self.0.get(name) {
            Some(p) => Ok(Some(p)),
            None => Err(format!("this node has no launch profile {name}")),
        }
    }

    pub fn all(&self) -> impl Iterator<Item = &Profile> {
        self.0.values()
    }

    pub fn any_sandboxed(&self) -> bool {
        self.all().any(|p| p.backend == Backend::Seatbelt)
    }
}

fn expand(p: &Path, home: &Path) -> PathBuf {
    match p.strip_prefix("~") {
        Ok(rest) => home.join(rest),
        Err(_) => p.to_path_buf(),
    }
}

impl Profile {
    /// Claude's config directory for sessions of this profile; `None` for one on the host,
    /// which uses the node's own.
    pub fn claude_dir(&self) -> Option<PathBuf> {
        match self.backend {
            Backend::Host => None,
            Backend::Seatbelt => self.state_dir.as_ref().map(|s| s.join("claude")),
        }
    }

    /// Makes the state directory and links the user's Claude files into its config
    /// directory, leaving anything already there alone.
    pub fn prepare(&self, user_claude: &Path) -> std::io::Result<()> {
        let Some(state) = &self.state_dir else {
            return Ok(());
        };
        for d in ["claude", "tmp", "home"] {
            std::fs::create_dir_all(state.join(d))?;
        }
        for f in &self.claude_files {
            let (from, to) = (user_claude.join(f), state.join("claude").join(f));
            if from.exists() && std::fs::symlink_metadata(&to).is_err() {
                std::os::unix::fs::symlink(&from, &to)?;
            }
        }
        Ok(())
    }

    /// The Claude token a session runs under: the one the hub sent, else the profile's
    /// token file.
    fn token(&self, claude_token: Option<&str>) -> Result<String, String> {
        let token = match (claude_token, &self.token_file) {
            (Some(t), _) => t.to_string(),
            (None, Some(f)) => std::fs::read_to_string(f)
                .map_err(|e| format!("reading the profile's token file {}: {e}", f.display()))?,
            (None, None) => {
                return Err(format!(
                    "the {} sandbox needs a Claude token: pick one, or set the node's default at the hub",
                    self.name
                ));
            }
        };
        let token = token.trim().to_string();
        if token.is_empty() || axum::http::HeaderValue::from_str(&format!("Bearer {token}")).is_err() {
            return Err(format!("the {} sandbox's Claude token is empty or not valid in a header", self.name));
        }
        Ok(token)
    }

    /// Where a session's Claude API proxy listens: in the state directory, which the
    /// sandbox may use. One socket per session, since sessions may run under different
    /// tokens.
    fn api_socket(&self) -> PathBuf {
        let id = uuid::Uuid::new_v4().simple().to_string();
        self.state_dir.as_ref().expect("checked at load").join("tmp").join(format!("claude-api-{}.sock", &id[..8]))
    }

    /// The environment inside: the node's own variables first, then the profile's. Claude
    /// gets a dummy token and sends its API requests to the proxy on `api_socket`.
    fn env(&self, user: &str, api_socket: &Path) -> Vec<(String, String)> {
        let state = self.state_dir.as_ref().expect("checked at load");
        let path: Vec<&str> = self.path.iter().map(String::as_str).chain(SYSTEM_PATH).collect();
        let s = |p: &Path| p.to_string_lossy().into_owned();
        let mut env: BTreeMap<String, String> = BTreeMap::from([
            ("PATH".into(), path.join(":")),
            ("HOME".into(), s(&state.join("home"))),
            ("USER".into(), user.into()),
            ("TERM".into(), "dumb".into()),
            ("LANG".into(), std::env::var("LANG").unwrap_or_else(|_| "en_US.UTF-8".into())),
            ("CLAUDE_CONFIG_DIR".into(), s(&state.join("claude"))),
            // claude ignores TMPDIR for its own files; nix and others use TMPDIR.
            ("CLAUDE_CODE_TMPDIR".into(), s(&state.join("tmp"))),
            ("TMPDIR".into(), s(&state.join("tmp"))),
            ("XDG_CACHE_HOME".into(), s(&state.join("home/.cache"))),
            ("CLAUDE_CODE_OAUTH_TOKEN".into(), crate::claude_api::DUMMY_TOKEN.into()),
            ("ANTHROPIC_UNIX_SOCKET".into(), s(api_socket)),
            // Plain HTTP over the socket: the proxy outside does the TLS.
            ("ANTHROPIC_BASE_URL".into(), "http://api.anthropic.com".into()),
        ]);
        env.extend(self.env.clone());
        env.into_iter().collect()
    }

    /// The filesystem grants: (read-only, read-write, denied).
    fn grants(&self, cwd: &Path, user_claude: &Path, deny: &[PathBuf]) -> (Vec<String>, Vec<String>, Vec<String>) {
        let s = |p: &Path| p.to_string_lossy().into_owned();
        let readonly = std::iter::once(PathBuf::from("/nix/store"))
            .chain(self.readonly.iter().cloned())
            .chain(self.claude_files.iter().map(|f| user_claude.join(f)))
            .map(|p| s(&p))
            .collect();
        let readwrite = self
            .state_dir
            .iter()
            .cloned()
            .chain([cwd.to_path_buf()])
            .chain(self.readwrite.iter().cloned())
            .map(|p| s(&p))
            .collect();
        let denied = std::iter::once(PathBuf::from(CC_SOCKS))
            .chain(deny.iter().cloned())
            .chain(self.denied.iter().cloned())
            .map(|p| s(&p))
            .collect();
        (readonly, readwrite, denied)
    }
}

/// A started session process, contained or not: its pipes, and a handle to wait on or kill.
pub struct Child {
    pub pid: u32,
    pub stdin: Box<dyn tokio::io::AsyncWrite + Send + Unpin>,
    pub stdout: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
    pub stderr: Box<dyn tokio::io::AsyncRead + Send + Unpin>,
    pub proc: Proc,
}

pub enum Proc {
    Host(tokio::process::Child),
    #[cfg(target_os = "macos")]
    /// The sandboxed process and its Claude API proxy, which stops when this is dropped.
    Seatbelt(mxc_sdk::v1::MxcProcess, crate::claude_api::Proxy),
}

impl Proc {
    /// Waits for the process to exit, and says how it ended.
    pub async fn wait(&mut self) -> String {
        match self {
            Proc::Host(c) => format!("{:?}", c.wait().await),
            #[cfg(target_os = "macos")]
            Proc::Seatbelt(p, _) => loop {
                // mxc's wait blocks; polling keeps the handle free for a kill.
                match p.try_wait() {
                    Ok(Some(code)) => break format!("exited {code}"),
                    Ok(None) => tokio::time::sleep(std::time::Duration::from_millis(200)).await,
                    Err(e) => break format!("wait failed: {e}"),
                }
            },
        }
    }

    /// Kills the process and the process groups of its tools, which claude starts as group
    /// leaders and which a kill of claude alone leaves running (EXP-20261107-413q82).
    pub fn kill(&mut self, pid: u32) {
        let groups = tool_groups(pid);
        match self {
            Proc::Host(c) => {
                let _ = c.start_kill();
            }
            #[cfg(target_os = "macos")]
            Proc::Seatbelt(p, _) => {
                let _ = p.kill();
            }
        }
        for g in groups {
            // SAFETY: a plain syscall on a group id read from the process table.
            unsafe { libc::killpg(g, libc::SIGKILL) };
        }
    }
}

/// The process groups of `pid`'s descendants, other than the node's own.
pub fn tool_groups(pid: u32) -> Vec<libc::pid_t> {
    use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};
    let mut sys = System::new();
    sys.refresh_processes_specifics(ProcessesToUpdate::All, true, ProcessRefreshKind::nothing());
    let mut found = vec![Pid::from_u32(pid)];
    let mut i = 0;
    while i < found.len() {
        let parent = found[i];
        found.extend(
            sys.processes()
                .iter()
                .filter(|(_, p)| p.parent() == Some(parent))
                .map(|(child, _)| *child),
        );
        i += 1;
    }
    // SAFETY: getpgrp and getpgid only read.
    let own = unsafe { libc::getpgrp() };
    let mut groups: Vec<libc::pid_t> = found[1..]
        .iter()
        .map(|p| unsafe { libc::getpgid(p.as_u32() as libc::pid_t) })
        .filter(|g| *g > 0 && *g != own)
        .collect();
    groups.sort();
    groups.dedup();
    groups
}

/// Starts `program args` under a sandboxed profile in `cwd`, through mxc's seatbelt
/// backend. `deny` adds paths the node keeps to itself, such as the wrappers' socket.
#[cfg(target_os = "macos")]
pub fn spawn_seatbelt(
    profile: &Profile,
    claude_token: Option<&str>,
    program: &str,
    args: &[String],
    cwd: &Path,
    user_claude: &Path,
    deny: &[PathBuf],
) -> Result<Child, String> {
    use std::os::fd::OwnedFd;

    use mxc_sdk::v1::configs::SeatbeltConfig;
    use mxc_sdk::v1::policy::{
        FilesystemPolicy, NetworkAction, NetworkEgressPolicy, NetworkIngressPolicy, NetworkPolicy,
    };
    use mxc_sdk::v1::{self, ContainerRequest, Containment};

    profile
        .prepare(user_claude)
        .map_err(|e| format!("preparing the profile's state directory: {e}"))?;
    let user = std::env::var("USER").unwrap_or_default();
    let token = profile.token(claude_token)?;
    let proxy = crate::claude_api::Proxy::start(&profile.api_socket(), &token, crate::claude_api::UPSTREAM)
        .map_err(|e| format!("starting the Claude API proxy: {e}"))?;
    let env = profile.env(&user, proxy.path());
    let (readonly, readwrite, denied) = profile.grants(cwd, user_claude, deny);
    let mut seatbelt = SeatbeltConfig::default();
    // getpwuid, which ssh and git need, goes through opendirectoryd (EXP-20261016-rq1407).
    seatbelt.extra_mach_lookups = vec!["com.apple.system.opendirectoryd.libinfo".into()];
    // `exec`, so the process mxc reports and kills is claude, not the shell around it.
    let command = std::iter::once(program)
        .chain(args.iter().map(String::as_str))
        .map(shell_quote)
        .collect::<Vec<_>>()
        .join(" ");
    let request = ContainerRequest {
        containment: Containment::Seatbelt(seatbelt),
        filesystem: Some(FilesystemPolicy {
            readwrite_paths: readwrite,
            readonly_paths: readonly,
            denied_paths: denied,
            ..Default::default()
        }),
        network: Some(NetworkPolicy {
            egress: Some(NetworkEgressPolicy {
                default: Some(NetworkAction::Allow),
                ..Default::default()
            }),
            ingress: Some(NetworkIngressPolicy {
                default: Some(NetworkAction::Deny),
                host_loopback: Some(NetworkAction::Deny),
            }),
            ..Default::default()
        }),
        environment: Some(env),
        inherit_default_environment: Some(false),
        working_directory: Some(cwd.to_string_lossy().into_owned()),
        ..ContainerRequest::new(format!("exec {command}"))
    };
    let mut proc = v1::spawn(request, Default::default()).map_err(|e| format!("starting the sandbox: {e}"))?;
    let stdio = proc
        .take_native_stdio()
        .map_err(|e| format!("the sandbox's pipes: {e}"))?
        .ok_or("the sandbox gave no pipes")?;
    let (Some(stdin), Some(stdout), Some(stderr)) = (stdio.stdin, stdio.stdout, stdio.stderr) else {
        let _ = proc.kill();
        return Err("the sandbox gave no stdin, stdout or stderr".into());
    };
    let reader = |fd: OwnedFd| -> Result<Box<dyn tokio::io::AsyncRead + Send + Unpin>, String> {
        Ok(Box::new(
            tokio::net::unix::pipe::Receiver::from_owned_fd(fd).map_err(|e| format!("a sandbox pipe: {e}"))?,
        ))
    };
    let stdin: Box<dyn tokio::io::AsyncWrite + Send + Unpin> = Box::new(
        tokio::net::unix::pipe::Sender::from_owned_fd(stdin).map_err(|e| format!("the sandbox's stdin: {e}"))?,
    );
    Ok(Child {
        pid: proc.id(),
        stdin,
        stdout: reader(stdout)?,
        stderr: reader(stderr)?,
        proc: Proc::Seatbelt(proc, proxy),
    })
}

#[cfg(not(target_os = "macos"))]
pub fn spawn_seatbelt(
    _profile: &Profile,
    _claude_token: Option<&str>,
    _program: &str,
    _args: &[String],
    _cwd: &Path,
    _user_claude: &Path,
    _deny: &[PathBuf],
) -> Result<Child, String> {
    Err("seatbelt runs only on macOS".into())
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    const FILE: &str = r#"
[profiles.sandbox]
backend = "seatbelt"
description = "confined"
state_dir = "~/.local/state/saarthi/sandbox"
token_file = "~/.config/saarthi/claude-oauth-token"
path = ["~/.nix-profile/bin", "/etc/profiles/per-user/me/bin"]
readonly = ["~/.config/git"]
claude_files = ["CLAUDE.md", "skills"]
env = { GIT_CONFIG_GLOBAL = "~/.config/git/config" }

[profiles.plain]
backend = "host"
"#;

    #[test]
    fn a_profiles_file_parses_with_home_expanded() {
        let home = Path::new("/home/me");
        let p = Profiles::parse(FILE, home);
        if !cfg!(target_os = "macos") {
            assert!(p.unwrap_err().contains("only on macOS"));
            return;
        }
        let p = p.unwrap();
        let s = p.get("sandbox").unwrap().unwrap();
        assert_eq!(s.backend, Backend::Seatbelt);
        assert_eq!(s.state_dir.as_deref(), Some(Path::new("/home/me/.local/state/saarthi/sandbox")));
        assert_eq!(s.claude_dir().unwrap(), Path::new("/home/me/.local/state/saarthi/sandbox/claude"));
        assert_eq!(s.path[0], "/home/me/.nix-profile/bin");
        assert_eq!(s.env["GIT_CONFIG_GLOBAL"], "/home/me/.config/git/config");
        assert!(p.get("").unwrap().is_none(), "the empty name is the host");
        assert!(p.get("plain").unwrap().unwrap().claude_dir().is_none());
        assert!(p.get("nope").is_err());
        assert!(p.any_sandboxed());
    }

    #[test]
    fn profiles_that_could_not_run_are_refused() {
        let home = Path::new("/h");
        let bad = |t: &str| Profiles::parse(t, home).unwrap_err();
        assert!(bad("[profiles.x]\nbackend = \"host\"\nbogus = 1\n").contains("bogus"));
        assert!(bad("[profiles.\"a b\"]\nbackend = \"host\"\n").contains("letters"));
        if cfg!(target_os = "macos") {
            assert!(bad("[profiles.x]\nbackend = \"seatbelt\"\ntoken_file = \"/t\"\n").contains("state_dir"));
            // Without a token file a sandbox loads, and runs only under a token from the hub.
            let tokenless = Profiles::parse(
                "[profiles.x]\nbackend = \"seatbelt\"\nstate_dir = \"/s\"\n",
                home,
            )
            .unwrap();
            let x = tokenless.get("x").unwrap().unwrap();
            assert!(
                x.token(None)
                    .unwrap_err()
                    .contains("needs a Claude token")
            );
            assert!(x.token(Some("t")).is_ok());
            assert!(x.token(Some("a\nb")).unwrap_err().contains("not valid"));
            let two = "[profiles.a]\nbackend = \"seatbelt\"\nstate_dir = \"/s\"\ntoken_file = \"/t\"\n\
                       [profiles.b]\nbackend = \"seatbelt\"\nstate_dir = \"/s\"\ntoken_file = \"/t\"\n";
            assert!(bad(two).contains("same state_dir"));
        }
    }

    #[test]
    fn the_sandbox_sees_its_state_the_work_and_the_store_but_not_other_sessions() {
        if !cfg!(target_os = "macos") {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let token = dir.path().join("token");
        std::fs::write(&token, "tok-123\n").unwrap();
        let text = format!(
            "[profiles.s]\nbackend = \"seatbelt\"\nstate_dir = \"{}\"\ntoken_file = \"{}\"\n\
             path = [\"/p/bin\"]\nclaude_files = [\"CLAUDE.md\"]\nenv = {{ TERM = \"xterm\" }}\n",
            dir.path().join("state").display(),
            token.display()
        );
        let p = Profiles::parse(&text, dir.path()).unwrap();
        let s = p.get("s").unwrap().unwrap();
        assert_eq!(s.token(None).unwrap(), "tok-123");
        // A token from the hub wins over the file.
        assert_eq!(s.token(Some("from-hub")).unwrap(), "from-hub");
        let socket = s.api_socket();
        assert!(socket.starts_with(dir.path().join("state/tmp")), "{}", socket.display());
        let env: BTreeMap<_, _> = s.env("me", &socket).into_iter().collect();
        // The token never goes inside: a dummy, and the proxy's socket.
        assert_eq!(env["CLAUDE_CODE_OAUTH_TOKEN"], crate::claude_api::DUMMY_TOKEN);
        assert_eq!(env["ANTHROPIC_UNIX_SOCKET"], socket.to_string_lossy());
        assert_eq!(env["ANTHROPIC_BASE_URL"], "http://api.anthropic.com");
        assert!(!env.values().any(|v| v.contains("tok-123")));
        assert_eq!(env["PATH"], "/p/bin:/usr/bin:/bin:/usr/sbin:/sbin");
        assert_eq!(env["CLAUDE_CONFIG_DIR"], dir.path().join("state/claude").to_string_lossy());
        assert_eq!(env["TMPDIR"], dir.path().join("state/tmp").to_string_lossy());
        assert_eq!(env["TERM"], "xterm", "the profile's environment wins");
        let (ro, rw, denied) = s.grants(Path::new("/work/repo"), Path::new("/u/.claude"), &[PathBuf::from("/run/node")]);
        assert_eq!(ro, ["/nix/store", "/u/.claude/CLAUDE.md"]);
        assert_eq!(rw, [dir.path().join("state").to_string_lossy().into_owned(), "/work/repo".into()]);
        assert_eq!(denied, [CC_SOCKS, "/run/node"]);

        let user_claude = dir.path().join("uc");
        std::fs::create_dir_all(&user_claude).unwrap();
        std::fs::write(user_claude.join("CLAUDE.md"), "be kind").unwrap();
        s.prepare(&user_claude).unwrap();
        s.prepare(&user_claude).unwrap();
        let linked = dir.path().join("state/claude/CLAUDE.md");
        assert_eq!(std::fs::read_to_string(linked).unwrap(), "be kind");
        assert!(dir.path().join("state/home").is_dir());
    }

    /// Whether a seatbelt sandbox can start here: not inside another one, such as the
    /// profile Nix runs builders under on macOS even without its own sandbox.
    #[cfg(target_os = "macos")]
    pub fn seatbelt_available() -> bool {
        use mxc_sdk::v1::{self, ContainerRequest, Containment};
        let request = ContainerRequest {
            containment: Containment::Seatbelt(Default::default()),
            ..ContainerRequest::new("/usr/bin/true")
        };
        match v1::run(request, Default::default()) {
            Ok(_) => true,
            Err(e) => {
                eprintln!("seatbelt is not available here, skipping: {e}");
                false
            }
        }
    }

    #[tokio::test]
    async fn a_kill_reaches_the_tool_process_groups() {
        // A stand-in for claude with a tool below it, in a process group that is not the
        // node's, as claude's tools are; a kill of the stand-in alone would leave the tool.
        let dir = tempfile::tempdir().unwrap();
        let marker = dir.path().join("tool.pid");
        let script = format!("sleep 300 & echo $! > '{}'; wait", marker.display());
        let child = tokio::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(script)
            .process_group(0)
            .spawn()
            .unwrap();
        let pid = child.id().unwrap();
        let tool = loop {
            if let Ok(s) = std::fs::read_to_string(&marker)
                && let Ok(p) = s.trim().parse::<i32>()
            {
                break p;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        };
        assert_eq!(tool_groups(pid), vec![pid as libc::pid_t]);
        let mut proc = Proc::Host(child);
        proc.kill(pid);
        proc.wait().await;
        let gone = |p: i32| unsafe { libc::kill(p, 0) } != 0;
        for _ in 0..50 {
            if gone(tool) {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
        panic!("the tool outlived the kill");
    }
}
