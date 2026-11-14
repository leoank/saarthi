//! Headless sessions the node starts itself (ADR-0034): `claude -p` with stream-json input
//! and output, in a directory under one of the node's spawn roots, through the user's login
//! shell so it gets a terminal's PATH. The node owns the process's stdin and stdout: a
//! prompt is a `user` line, an interrupt the SDK's `control_request`, kill closes stdin and
//! kills the process if it lingers. Busy and idle come from the stream, since the registry
//! keeps such a session `busy` (EXP-20261106-37f79k); everything else about the session
//! (its transcript, hooks, registry entry) is tracked as for any other. The node's hooks go
//! to the process with `--settings` too, so its permission requests reach the node even when
//! the user's settings file lacks them: without a PermissionRequest hook `claude -p` refuses
//! every tool that needs approval (EXP-20261106-qt94y4).
//!
//! A spawn may name a launch profile (ADR-0038): a sandboxed one runs claude under seatbelt
//! with its own config directory and the profile's token, its hooks coming back over the
//! node's unix socket as command hooks, since loopback TCP is closed inside.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use saarthi_proto::event::v1::{LaunchProfile, SessionStatus};
use saarthi_proto::node::v1::ClaudeToken;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::{Mutex, mpsc};

use crate::containment::{self, Child, Profile, Profiles, Proc};
use crate::sessions::Sessions;

/// A usage probe's one turn (ADR-0043): as short as a model turn gets.
const PROBE_PROMPT: &str = "Reply with the single word ok.";

/// How long a usage probe may take before it is given up.
const PROBE_TIMEOUT: Duration = Duration::from_secs(90);

/// How long a killed session has to exit after its input closes before it is killed.
const KILL_GRACE: Duration = Duration::from_secs(5);

/// The values claude's `--permission-mode` takes; anything else is refused, so a client
/// cannot pass other flags through it.
const PERMISSION_MODES: [&str; 6] = ["acceptEdits", "auto", "bypassPermissions", "manual", "dontAsk", "plan"];

pub struct Spawner {
    /// The claude binary, resolved by the login shell when it is a bare name.
    claude: String,
    /// The shell that starts it: the user's `$SHELL`, run as a login shell.
    shell: PathBuf,
    /// Canonical directories a session may be spawned under.
    roots: Vec<PathBuf>,
    /// The node's hooks block, passed to every session it starts as `--settings`.
    hooks: Option<String>,
    contained: Contained,
    sessions: Arc<Sessions>,
    /// Running spawned sessions by id: the line writer for each one's stdin. Dropping the
    /// sender closes the input, which ends the session.
    procs: Mutex<HashMap<String, mpsc::Sender<String>>>,
    /// The hub, through which a session under one of its tokens reaches the Claude API
    /// (ADR-0051); set when the node links to a hub.
    hub_api: std::sync::OnceLock<Arc<crate::claude_api::HubApi>>,
}

/// What sandboxed sessions need besides their profile.
#[derive(Default)]
pub struct Contained {
    pub profiles: Profiles,
    /// The node's hooks block for a sandboxed session: command hooks posting to the unix
    /// socket.
    pub hooks: Option<Value>,
    /// The user's own Claude config directory, which the profiles' Claude files come from.
    pub user_claude: PathBuf,
    /// Paths the node keeps from every sandbox, such as the wrappers' socket.
    pub deny: Vec<PathBuf>,
}

/// A spawn root as a canonical directory, created when missing; None, logged, when it
/// cannot be made or is not a directory.
fn resolve_root(root: &Path) -> Option<PathBuf> {
    if !root.exists() {
        match std::fs::create_dir_all(root) {
            Ok(()) => tracing::info!(root = %root.display(), "created a missing spawn root"),
            Err(e) => {
                tracing::error!(root = %root.display(), error = %e, "spawn root left out: cannot create it");
                return None;
            }
        }
    }
    match root.canonicalize() {
        Ok(dir) if dir.is_dir() => Some(dir),
        Ok(_) => {
            tracing::error!(root = %root.display(), "spawn root left out: not a directory");
            None
        }
        Err(e) => {
            tracing::error!(root = %root.display(), error = %e, "spawn root left out");
            None
        }
    }
}

impl Spawner {
    /// A root that does not exist is created; one that cannot be is left out with an error in
    /// the log, so a bad root costs that root and never the node (ADR-0044).
    /// `hooks` is the settings block that points Claude Code's hooks at this node.
    pub fn new(
        claude: String,
        shell: PathBuf,
        roots: &[PathBuf],
        hooks: Option<Value>,
        contained: Contained,
        sessions: Arc<Sessions>,
    ) -> Arc<Self> {
        let roots = roots.iter().filter_map(|r| resolve_root(r)).collect();
        Arc::new(Self {
            claude,
            shell,
            roots,
            hooks: hooks.map(|h| h.to_string()),
            contained,
            sessions,
            procs: Mutex::new(HashMap::new()),
            hub_api: std::sync::OnceLock::new(),
        })
    }

    /// The hub that relays the Claude API for sessions under its tokens.
    pub fn set_hub_api(&self, api: Arc<crate::claude_api::HubApi>) {
        let _ = self.hub_api.set(api);
    }

    /// Where the API requests of a session under the hub's token `name` go.
    fn via_hub(&self, name: &str) -> Result<crate::claude_api::Upstream, String> {
        let api = self
            .hub_api
            .get()
            .ok_or_else(|| format!("this node has no hub to run under the {name} token through"))?;
        Ok(crate::claude_api::Upstream::Hub {
            api: api.clone(),
            name: name.to_string(),
        })
    }

    pub fn profiles(&self) -> &Profiles {
        &self.contained.profiles
    }

    /// Claude's config directories of the sandboxed profiles, with their profiles' names,
    /// whose registries and transcripts the node watches beside its own.
    pub fn claude_dirs(&self) -> Vec<(String, PathBuf)> {
        self.contained
            .profiles
            .all()
            .filter_map(|p| Some((p.name.clone(), p.claude_dir()?)))
            .collect()
    }

    /// The launch profiles, as the node registers them for the app to offer.
    pub fn launch_profiles(&self) -> Vec<LaunchProfile> {
        self.contained
            .profiles
            .all()
            .map(|p| LaunchProfile {
                name: p.name.clone(),
                description: p.description.clone(),
                sandboxed: p.claude_dir().is_some(),
            })
            .collect()
    }

    pub fn roots(&self) -> Vec<String> {
        self.roots
            .iter()
            .map(|r| r.to_string_lossy().into_owned())
            .collect()
    }

    /// The directory a session may be spawned in: a directory under a root, after `~` and
    /// symlinks are resolved. With `create`, the missing part of the path is made, once its
    /// nearest existing parent is known to be under a root and the rest is plain names.
    fn allowed(&self, cwd: &str, create: bool) -> Result<PathBuf, String> {
        if self.roots.is_empty() {
            return Err("spawning is off on this node: start it with --spawn-root".into());
        }
        let expanded = match cwd.strip_prefix("~/") {
            Some(rest) => std::env::var_os("HOME")
                .map(|h| PathBuf::from(h).join(rest))
                .ok_or("HOME is not set")?,
            None => PathBuf::from(cwd),
        };
        if !expanded.is_absolute() {
            return Err(format!("{cwd} is not an absolute path"));
        }
        let base = expanded
            .ancestors()
            .find(|a| a.exists())
            .ok_or_else(|| format!("{cwd}: no part of it exists"))?;
        let missing = expanded.strip_prefix(base).expect("an ancestor").to_path_buf();
        if !missing.as_os_str().is_empty() {
            if !create {
                return Err(format!("{cwd} does not exist"));
            }
            if !missing
                .components()
                .all(|c| matches!(c, std::path::Component::Normal(_)))
            {
                return Err(format!("{cwd}: a directory to create may not contain . or .."));
            }
            let base = base.canonicalize().map_err(|e| format!("{cwd}: {e}"))?;
            if !self.roots.iter().any(|r| base.starts_with(r)) {
                return Err(format!("{cwd} is not under a spawn root of this node"));
            }
            std::fs::create_dir_all(base.join(&missing)).map_err(|e| format!("creating {cwd}: {e}"))?;
            tracing::info!(dir = %base.join(&missing).display(), "created a directory to spawn in");
        }
        let dir = expanded
            .canonicalize()
            .map_err(|e| format!("{cwd}: {e}"))?;
        if !dir.is_dir() {
            return Err(format!("{cwd} is not a directory"));
        }
        if !self.roots.iter().any(|r| dir.starts_with(r)) {
            return Err(format!("{cwd} is not under a spawn root of this node"));
        }
        Ok(dir)
    }

    /// Starts a session with `id` in `cwd`, creating the directory first when `create` asks,
    /// and sends it its first prompt if there is one; without, it waits idle for a prompt.
    /// An empty `mode` leaves the permission mode to the user's settings; an empty
    /// `profile` runs it on the host.
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn(
        self: &Arc<Self>,
        id: &str,
        cwd: &str,
        prompt: &str,
        model: &str,
        create: bool,
        mode: &str,
        profile: &str,
        claude_token: Option<&ClaudeToken>,
    ) -> Result<(), String> {
        if uuid::Uuid::parse_str(id).is_err() {
            return Err("the session id is not a UUID".into());
        }
        if !mode.is_empty() && !PERMISSION_MODES.contains(&mode) {
            return Err(format!("{mode} is not a permission mode claude knows"));
        }
        let profile = self.contained.profiles.get(profile)?.cloned();
        let dir = self.allowed(cwd, create)?;
        self.start(
            id,
            &dir,
            prompt,
            model,
            mode,
            "--session-id",
            profile.as_ref(),
            claude_token,
        )
        .await
    }

    /// Continues the finished session `id` headless in its directory `cwd`, in place: the
    /// same id and transcript, its context kept (EXP-20261106-62st4b). Refused while the
    /// session runs, and when this node has no transcript of it there. It runs under the
    /// profile whose config directory holds its transcript, so a sandboxed session stays one.
    pub async fn resume(
        self: &Arc<Self>,
        id: &str,
        cwd: &str,
        prompt: &str,
        model: &str,
        claude_token: Option<&ClaudeToken>,
    ) -> Result<(), String> {
        if uuid::Uuid::parse_str(id).is_err() {
            return Err("the session id is not a UUID".into());
        }
        if self.sessions.is_live(id).await || self.procs.lock().await.contains_key(id) {
            return Err("the session is still running".into());
        }
        let dir = self.allowed(cwd, false)?;
        let has = |claude_dir: &Path| crate::sessions::transcript_path(claude_dir, &dir.to_string_lossy(), id).exists();
        let profile = if has(self.sessions.claude_dir()) {
            None
        } else {
            match self.contained.profiles.all().find(|p| p.claude_dir().is_some_and(|d| has(&d))) {
                Some(p) => Some(p.clone()),
                None => return Err(format!("this node has no transcript of the session in {cwd}")),
            }
        };
        self.start(
            id,
            &dir,
            prompt,
            model,
            "",
            "--resume",
            profile.as_ref(),
            claude_token,
        )
        .await
    }

    /// Starts claude for session `id` in `dir`, as a new session (`--session-id`) or a
    /// resumed one (`--resume`), and sends the first prompt if there is one. Under one of
    /// the hub's tokens claude holds a dummy (as `CLAUDE_CODE_OAUTH_TOKEN`, which wins over
    /// the keychain login, EXP-20261111-q6sgt8) and its API requests go through the hub,
    /// which adds the token (ADR-0051); without a mode it runs in auto, the default the
    /// dummy cannot read from the account ([[EXP-20261114-d82rcz]]).
    #[allow(clippy::too_many_arguments)]
    async fn start(
        self: &Arc<Self>,
        id: &str,
        dir: &Path,
        prompt: &str,
        model: &str,
        mode: &str,
        how: &str,
        profile: Option<&Profile>,
        claude_token: Option<&ClaudeToken>,
    ) -> Result<(), String> {
        let hub = match claude_token.filter(|t| !t.name.is_empty()) {
            Some(t) => Some(self.via_hub(&t.name)?),
            None => None,
        };
        let mode = if mode.is_empty() && hub.is_some() { "auto" } else { mode };
        let contained = profile.filter(|p| p.claude_dir().is_some());
        let hooks = match contained {
            Some(_) => self.contained.hooks.as_ref().map(Value::to_string),
            None => self.hooks.clone(),
        };
        let mut args = vec![
            "-p",
            "--input-format",
            "stream-json",
            "--output-format",
            "stream-json",
            "--verbose",
            how,
            id,
        ];
        if let Some(hooks) = &hooks {
            args.extend(["--settings", hooks]);
        }
        if !mode.is_empty() {
            args.extend(["--permission-mode", mode]);
        }
        if !model.is_empty() {
            args.extend(["--model", model]);
        }
        let child = match contained {
            // Inside, claude is found on the profile's PATH; no login shell runs there.
            Some(p) => containment::spawn_seatbelt(
                p,
                hub,
                &self.claude,
                &args.iter().map(|a| a.to_string()).collect::<Vec<_>>(),
                dir,
                &self.contained.user_claude,
                &self.contained.deny,
            )?,
            None => self.host(&args, dir, hub)?,
        };
        let Child {
            pid,
            mut stdin,
            stdout,
            stderr,
            proc: mut child,
        } = child;
        let profile_name = profile.map(|p| p.name.as_str()).unwrap_or("");
        let token_name = claude_token.map(|t| t.name.as_str()).unwrap_or("");
        tracing::info!(session = %id, pid, cwd = %dir.display(), profile = profile_name,
            claude_token = token_name, "spawned a session");
        let claude_dir = contained
            .and_then(Profile::claude_dir)
            .unwrap_or_else(|| self.sessions.claude_dir().to_path_buf());

        let (tx, mut lines) = mpsc::channel::<String>(64);
        let first = !prompt.trim().is_empty();
        if first {
            tx.send(user_line(prompt)).await.expect("just made");
        }
        self.procs.lock().await.insert(id.to_string(), tx);
        self.sessions
            .add_spawned(
                id,
                pid,
                &dir.to_string_lossy(),
                &claude_dir,
                profile_name,
                token_name,
                first,
            )
            .await;

        // The writer: lines to stdin until every sender is gone, then the input closes and a
        // process that does not exit within the grace period is killed.
        let (closed_tx, closed) = tokio::sync::oneshot::channel::<()>();
        tokio::spawn(async move {
            while let Some(line) = lines.recv().await {
                if stdin.write_all(line.as_bytes()).await.is_err() || stdin.flush().await.is_err() {
                    break;
                }
            }
            drop(stdin);
            let _ = closed_tx.send(());
        });
        let sid = id.to_string();
        tokio::spawn(async move {
            let mut err = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = err.next_line().await {
                tracing::info!(session = %sid, "claude: {line}");
            }
        });
        let this = self.clone();
        let sid = id.to_string();
        tokio::spawn(async move {
            let mut out = BufReader::new(stdout).lines();
            let reader = async {
                while let Ok(Some(line)) = out.next_line().await {
                    let Ok(msg) = serde_json::from_str::<Value>(&line) else {
                        continue;
                    };
                    if msg["type"] == "result" {
                        this.sessions.set_status(&sid, SessionStatus::Idle).await;
                    } else if msg["type"] == "rate_limit_event" {
                        // The account's windows, from the API's response headers: the hub
                        // keeps them per Claude token (ADR-0042).
                        let info = msg["rate_limit_info"].to_string();
                        this.sessions.daemon_event(&sid, "rate_limit", info).await;
                    }
                }
            };
            let killer = async {
                if closed.await.is_ok() {
                    tokio::time::sleep(KILL_GRACE).await;
                    true
                } else {
                    false
                }
            };
            tokio::select! {
                _ = reader => {}
                true = killer => {
                    tracing::info!(session = %sid, "the session outlived its closed input; killing it");
                    child.kill(pid);
                }
            }
            let status = child.wait().await;
            tracing::info!(session = %sid, ?status, "spawned session exited");
            this.procs.lock().await.remove(&sid);
            this.sessions.finish(&sid).await;
        });
        Ok(())
    }

    /// One throwaway Haiku turn under `claude_token`, for its account's rate-limit windows
    /// (ADR-0043): claude runs through the login shell in a fresh temporary directory, with
    /// an empty config directory there, so no settings, hooks or session of the node's see
    /// it; the directory is removed afterwards. Its request goes through the hub, as every
    /// session's under the hub's tokens (ADR-0051). Returns the turn's `rate_limit_info` as
    /// JSON. Only a node that runs headless sessions (spawn roots) takes probes.
    pub async fn probe(&self, claude_token: Option<&ClaudeToken>) -> Result<String, String> {
        let Some(token) = claude_token.filter(|t| !t.name.is_empty()) else {
            return Err("a usage probe needs a Claude token".into());
        };
        if self.roots.is_empty() {
            return Err("this node does not run headless sessions (no --spawn-root)".into());
        }
        let dir =
            std::env::temp_dir().join(format!("saarthi-probe-{}", uuid::Uuid::new_v4().simple()));
        let (cfg, work) = (dir.join("claude"), dir.join("work"));
        let made = std::fs::create_dir_all(&cfg).and_then(|()| std::fs::create_dir_all(&work));
        let outcome = match made {
            Ok(()) => match self.via_hub(&token.name) {
                Ok(hub) => self.probe_in(hub, &cfg, &work).await,
                Err(e) => Err(e),
            },
            Err(e) => Err(format!("making {}: {e}", dir.display())),
        };
        let _ = std::fs::remove_dir_all(&dir);
        outcome
    }

    async fn probe_in(&self, hub: crate::claude_api::Upstream, cfg: &Path, work: &Path) -> Result<String, String> {
        let socket = crate::claude_api::host_socket().map_err(|e| format!("the probe's API socket: {e}"))?;
        let proxy = crate::claude_api::Proxy::start(&socket, hub)
            .map_err(|e| format!("starting the probe's Claude API proxy: {e}"))?;
        let mut child = tokio::process::Command::new(&self.shell)
            .arg("-l")
            .arg("-c")
            .arg(r#"exec "$0" "$@""#)
            .arg(&self.claude)
            .args([
                "-p",
                PROBE_PROMPT,
                "--model",
                "haiku",
                "--output-format",
                "stream-json",
                "--verbose",
            ])
            .env("CLAUDE_CONFIG_DIR", cfg)
            .envs(crate::claude_api::env(proxy.path()))
            .current_dir(work)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("starting {}: {e}", self.shell.display()))?;
        let stdout = child.stdout.take().expect("piped");
        let read = async {
            let mut lines = BufReader::new(stdout).lines();
            let (mut info, mut failed) = (None, None);
            while let Ok(Some(line)) = lines.next_line().await {
                let Ok(msg) = serde_json::from_str::<Value>(&line) else {
                    continue;
                };
                if msg["type"] == "rate_limit_event" {
                    info = Some(msg["rate_limit_info"].to_string());
                } else if msg["type"] == "result" && msg["is_error"] == true {
                    failed = Some(
                        msg["result"]
                            .as_str()
                            .unwrap_or("the turn failed")
                            .to_string(),
                    );
                }
            }
            (info, failed)
        };
        let (info, failed) = tokio::time::timeout(PROBE_TIMEOUT, read)
            .await
            .map_err(|_| "the probe turn did not finish in time".to_string())?;
        let _ = child.wait().await;
        match (info, failed) {
            (Some(info), _) => Ok(info),
            (None, Some(why)) => Err(format!("the probe turn failed: {why}")),
            (None, None) => Err("the probe turn reported no rate limits".into()),
        }
    }

    /// Starts claude on the host through the user's login shell, for a terminal's PATH;
    /// under one of the hub's tokens with a dummy and a proxy to the hub, which the
    /// returned process holds.
    fn host(&self, args: &[&str], dir: &Path, hub: Option<crate::claude_api::Upstream>) -> Result<Child, String> {
        let mut command = tokio::process::Command::new(&self.shell);
        let proxy = match hub {
            Some(hub) => {
                let socket =
                    crate::claude_api::host_socket().map_err(|e| format!("the session's API socket: {e}"))?;
                let proxy = crate::claude_api::Proxy::start(&socket, hub)
                    .map_err(|e| format!("starting the Claude API proxy: {e}"))?;
                command.envs(crate::claude_api::env(proxy.path()));
                Some(proxy)
            }
            None => {
                // Not one the node itself was started with.
                command.env_remove("CLAUDE_CODE_OAUTH_TOKEN");
                None
            }
        };
        let mut child = command
            .arg("-l")
            .arg("-c")
            .arg(r#"exec "$0" "$@""#)
            .arg(&self.claude)
            .args(args)
            .current_dir(dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("starting {}: {e}", self.shell.display()))?;
        Ok(Child {
            pid: child.id().unwrap_or_default(),
            stdin: Box::new(child.stdin.take().expect("piped")),
            stdout: Box::new(child.stdout.take().expect("piped")),
            stderr: Box::new(child.stderr.take().expect("piped")),
            proc: Proc::Host(child, proxy),
        })
    }

    async fn send(&self, id: &str, line: String) -> Result<(), String> {
        let tx = self.procs.lock().await.get(id).cloned();
        let Some(tx) = tx else {
            return Err("the spawned session is not running".into());
        };
        tx.send(line)
            .await
            .map_err(|_| "the spawned session is not running".to_string())
    }

    pub async fn prompt(&self, id: &str, text: &str) -> Result<(), String> {
        if text.trim().is_empty() {
            return Err("the prompt is empty".into());
        }
        self.send(id, user_line(text)).await?;
        self.sessions.set_status(id, SessionStatus::Busy).await;
        Ok(())
    }

    /// Stops the running turn; a follow-up prompt, if any, starts the next one.
    pub async fn interrupt(&self, id: &str, then_prompt: Option<&str>) -> Result<(), String> {
        self.send(id, interrupt_line()).await?;
        if let Some(text) = then_prompt.filter(|t| !t.trim().is_empty()) {
            self.prompt(id, text).await?;
        }
        Ok(())
    }

    /// Interrupts the running turn and closes the session's input, which ends it; the
    /// process is killed if it lingers. Without the interrupt, claude finishes the turn
    /// before it reads the end of its input.
    pub async fn kill(&self, id: &str) -> Result<(), String> {
        let Some(tx) = self.procs.lock().await.remove(id) else {
            return Err("the spawned session is not running".into());
        };
        let _ = tx.send(interrupt_line()).await;
        Ok(())
    }
}

/// The SDK's interrupt: ends the running turn and its tool (EXP-20261106-37f79k).
fn interrupt_line() -> String {
    let request = json!({
        "type": "control_request",
        "request_id": uuid::Uuid::new_v4().simple().to_string(),
        "request": {"subtype": "interrupt"},
    });
    format!("{request}\n")
}

fn user_line(text: &str) -> String {
    let line = json!({"type": "user", "message": {"role": "user", "content": text}});
    format!("{line}\n")
}

/// The login shell to start sessions through: `$SHELL`, else `/bin/sh`.
pub fn login_shell() -> PathBuf {
    std::env::var_os("SHELL")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| Path::new("/bin/sh").to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outbox::Outbox;
    use saarthi_proto::event::v1::Launch;
    use saarthi_proto::node::v1::node_message::Body;

    /// A stand-in for claude: logs its arguments and every input line, answers each `user`
    /// line with a `result`, and exits when its input closes.
    const FAKE: &str = r#"#!/bin/sh
echo "$@" > LOG
while IFS= read -r line; do
  echo "$line" >> LOG
  case "$line" in
    *'"type":"user"'*) echo '{"type":"result","subtype":"success"}' ;;
  esac
done
echo closed >> LOG
"#;

    struct Fixture {
        _dir: tempfile::TempDir,
        root: PathBuf,
        log: PathBuf,
        spawner: Arc<Spawner>,
        sessions: Arc<Sessions>,
        rx: mpsc::Receiver<saarthi_proto::node::v1::NodeMessage>,
    }

    fn fixture() -> Fixture {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("work");
        std::fs::create_dir_all(root.join("repo")).unwrap();
        let log = dir.path().join("log");
        let fake = dir.path().join("claude");
        let script = FAKE.replace("LOG", &format!("'{}'", log.display()));
        std::fs::write(&fake, script).unwrap();
        std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
        let (outbox, rx) = Outbox::new(256);
        let sessions = Arc::new(Sessions::new(
            dir.path().join(".claude"),
            outbox,
            Duration::from_millis(20),
            Vec::new(),
        ));
        let spawner = Spawner::new(
            fake.to_string_lossy().into_owned(),
            PathBuf::from("/bin/sh"),
            std::slice::from_ref(&root),
            None,
            Contained::default(),
            sessions.clone(),
        );
        Fixture {
            root: root.canonicalize().unwrap(),
            _dir: dir,
            log,
            spawner,
            sessions,
            rx,
        }
    }

    async fn log_has(log: &Path, text: &str) -> String {
        for _ in 0..200 {
            let s = std::fs::read_to_string(log).unwrap_or_default();
            if s.contains(text) {
                return s;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        panic!("{text:?} never reached {}", log.display());
    }

    async fn status_of(sessions: &Sessions, id: &str) -> (SessionStatus, bool) {
        let s = sessions.snapshot().await;
        let s = s.iter().find(|s| s.id == id).unwrap();
        (s.status(), s.ended_ms.is_some())
    }

    #[tokio::test]
    async fn a_spawned_session_takes_prompts_interrupts_and_a_kill_over_its_input() {
        let mut f = fixture();
        let id = uuid::Uuid::new_v4().to_string();
        let cwd = f.root.join("repo");
        f.spawner
            .spawn(
                &id,
                &cwd.to_string_lossy(),
                "hello",
                "haiku",
                false,
                "",
                "",
                None,
            )
            .await
            .unwrap();
        let log = log_has(&f.log, "hello").await;
        let args = log.lines().next().unwrap();
        assert!(args.contains(&format!("--session-id {id}")), "{args}");
        assert!(args.contains("--input-format stream-json --output-format stream-json"));
        assert!(args.ends_with("--model haiku"), "{args}");

        let first = loop {
            match f.rx.recv().await.unwrap().body {
                Some(Body::Session(s)) => break s,
                _ => continue,
            }
        };
        assert_eq!(first.launch(), Launch::Spawned);
        assert_eq!(first.cwd, cwd.to_string_lossy());

        // The fake answers each turn with a result, which marks the session idle.
        for _ in 0..100 {
            if status_of(&f.sessions, &id).await.0 == SessionStatus::Idle {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert_eq!(status_of(&f.sessions, &id).await.0, SessionStatus::Idle);
        assert!(f.sessions.is_spawned(&id).await);

        f.spawner.prompt(&id, "second").await.unwrap();
        f.spawner.interrupt(&id, Some("third")).await.unwrap();
        let log = log_has(&f.log, "third").await;
        let interrupt = log.lines().find(|l| l.contains("control_request")).unwrap();
        let v: Value = serde_json::from_str(interrupt).unwrap();
        assert_eq!(v["request"]["subtype"], "interrupt");
        assert!(log.find("second").unwrap() < log.find("control_request").unwrap());

        f.spawner.kill(&id).await.unwrap();
        let log = log_has(&f.log, "closed").await;
        let last_two: Vec<_> = log.lines().rev().take(2).collect();
        assert!(last_two[1].contains("interrupt"), "a kill interrupts first: {last_two:?}");
        for _ in 0..100 {
            if status_of(&f.sessions, &id).await.1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(status_of(&f.sessions, &id).await.1, "ended once the process exited");
        assert!(f.spawner.prompt(&id, "late").await.is_err());
    }

    #[tokio::test]
    async fn the_node_hooks_and_a_permission_mode_reach_claude_and_other_modes_are_refused() {
        let f = fixture();
        let hooked = Spawner::new(
            f.spawner.claude.clone(),
            PathBuf::from("/bin/sh"),
            std::slice::from_ref(&f.root),
            Some(json!({"hooks": {"PermissionRequest": "node"}})),
            Contained::default(),
            f.sessions.clone(),
        );
        let cwd = f.root.join("repo").to_string_lossy().into_owned();
        let id = uuid::Uuid::new_v4().to_string();
        let err = hooked
            .spawn(&id, &cwd, "", "", false, "--model", "", None)
            .await
            .unwrap_err();
        assert!(err.contains("permission mode"), "{err}");
        hooked
            .spawn(&id, &cwd, "hello", "", false, "bypassPermissions", "", None)
            .await
            .unwrap();
        let log = log_has(&f.log, "hello").await;
        let args = log.lines().next().unwrap();
        assert!(
            args.contains(r#"--settings {"hooks":{"PermissionRequest":"node"}} --permission-mode bypassPermissions"#),
            "{args}"
        );
        hooked.kill(&id).await.unwrap();
    }

    #[tokio::test]
    async fn only_directories_under_a_root_are_allowed() {
        let f = fixture();
        let id = uuid::Uuid::new_v4().to_string();
        let outside = f.root.parent().unwrap().to_string_lossy().into_owned();
        let escape = format!("{}/repo/../..", f.root.display());
        for cwd in [
            outside.as_str(),
            escape.as_str(),
            "relative/path",
            "/no/such/dir",
        ] {
            assert!(
                f.spawner
                    .spawn(&id, cwd, "hi", "", true, "", "", None)
                    .await
                    .is_err(),
                "{cwd}"
            );
        }
        assert!(
            f.spawner
                .spawn(
                    "not-a-uuid",
                    &f.root.to_string_lossy(),
                    "hi",
                    "",
                    false,
                    "",
                    "",
                    None
                )
                .await
                .is_err()
        );
        let off = Spawner::new("claude".into(), "/bin/sh".into(), &[], None, Contained::default(), f.sessions.clone());
        let err = off
            .spawn(
                &id,
                &f.root.to_string_lossy(),
                "hi",
                "",
                false,
                "",
                "",
                None,
            )
            .await
            .unwrap_err();
        assert!(err.contains("--spawn-root"), "{err}");
    }

    #[tokio::test]
    async fn a_missing_directory_is_made_only_when_asked_and_only_under_a_root() {
        let f = fixture();
        let new = f.root.join("new/deeper");
        let path = new.to_string_lossy().into_owned();
        let id = uuid::Uuid::new_v4().to_string();
        let err = f
            .spawner
            .spawn(&id, &path, "", "", false, "", "", None)
            .await
            .unwrap_err();
        assert!(err.contains("does not exist"), "{err}");
        assert!(!new.exists());

        let outside = f.root.parent().unwrap().join("elsewhere/new");
        let err = f
            .spawner
            .spawn(&id, &outside.to_string_lossy(), "", "", true, "", "", None)
            .await
            .unwrap_err();
        assert!(err.contains("spawn root"), "{err}");
        assert!(!outside.parent().unwrap().exists());
        let sneaky = format!("{}/new/../../elsewhere", f.root.display());
        assert!(
            f.spawner
                .spawn(&id, &sneaky, "", "", true, "", "", None)
                .await
                .is_err()
        );

        // Made, then started idle with no prompt: nothing written to its input.
        f.spawner
            .spawn(&id, &path, "", "", true, "", "", None)
            .await
            .unwrap();
        assert!(new.is_dir());
        let log = log_has(&f.log, "--session-id").await;
        assert_eq!(log.lines().count(), 1, "no first prompt: {log}");
        assert_eq!(status_of(&f.sessions, &id).await.0, SessionStatus::Idle);
        f.spawner.kill(&id).await.unwrap();
    }

    /// A stand-in for claude inside a sandbox: logs its arguments, what it can see, and every
    /// input line, as FAKE does.
    #[cfg(target_os = "macos")]
    const FAKE_INSIDE: &str = r#"#!/bin/sh
{ echo "$@"; echo "token=$CLAUDE_CODE_OAUTH_TOKEN config=$CLAUDE_CONFIG_DIR home=$HOME";
  cat SECRET 2>&1 || true
  echo "socket=$ANTHROPIC_UNIX_SOCKET base=$ANTHROPIC_BASE_URL"
  /usr/bin/curl -s -o /dev/null -w 'proxy=%{http_code}\n' --unix-socket "$ANTHROPIC_UNIX_SOCKET" http://x/api/elsewhere; } > LOG
while IFS= read -r line; do
  echo "$line" >> LOG
  case "$line" in
    *'"type":"user"'*) echo '{"type":"result","subtype":"success"}' ;;
  esac
done
echo closed >> LOG
"#;

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn a_sandboxed_profile_runs_confined_with_its_token_hooks_and_config_dir_and_resumes_in_it() {
        use std::os::unix::fs::PermissionsExt;
        if !crate::containment::tests::seatbelt_available() {
            return;
        }
        let f = fixture();
        let base = f._dir.path().canonicalize().unwrap();
        let (bin, logs) = (base.join("bin"), base.join("logs"));
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::create_dir_all(&logs).unwrap();
        let (log, secret) = (logs.join("log"), base.join("secret"));
        std::fs::write(&secret, "the owner's secret").unwrap();
        let script = FAKE_INSIDE
            .replace("LOG", &format!("'{}'", log.display()))
            .replace("SECRET", &format!("'{}'", secret.display()));
        std::fs::write(bin.join("claude"), script).unwrap();
        std::fs::set_permissions(bin.join("claude"), std::fs::Permissions::from_mode(0o755)).unwrap();
        std::fs::write(base.join("token"), "tok-abc\n").unwrap();
        let profiles = Profiles::parse(
            &format!(
                "[profiles.box]\nbackend = \"seatbelt\"\nstate_dir = \"{0}/state\"\ntoken_file = \"{0}/token\"\n\
                 path = [\"{0}/bin\"]\nreadonly = [\"{0}/bin\"]\nreadwrite = [\"{0}/logs\"]\n",
                base.display()
            ),
            &base,
        )
        .unwrap();
        let spawner = Spawner::new(
            "claude".into(),
            PathBuf::from("/bin/sh"),
            std::slice::from_ref(&f.root),
            Some(json!({"hooks": "tcp"})),
            Contained {
                profiles,
                hooks: Some(json!({"hooks": "socket"})),
                user_claude: base.join(".claude"),
                deny: Vec::new(),
            },
            f.sessions.clone(),
        );
        assert_eq!(spawner.claude_dirs(), [("box".to_string(), base.join("state/claude"))]);
        assert_eq!(
            spawner.launch_profiles(),
            [LaunchProfile {
                name: "box".into(),
                description: String::new(),
                sandboxed: true
            }]
        );
        let cwd = f.root.join("repo").to_string_lossy().into_owned();
        let id = uuid::Uuid::new_v4().to_string();
        let err = spawner
            .spawn(&id, &cwd, "", "", false, "", "nope", None)
            .await
            .unwrap_err();
        assert!(err.contains("no launch profile nope"), "{err}");

        spawner
            .spawn(&id, &cwd, "hello", "", false, "", "box", None)
            .await
            .unwrap();
        let text = log_has(&log, "hello").await;
        let mut lines = text.lines();
        let args = lines.next().unwrap();
        assert!(args.contains(r#"--settings {"hooks":"socket"}"#), "the socket hooks, not the TCP ones: {args}");
        let env = lines.next().unwrap();
        // Inside, a dummy; the token stays with the node's proxy (ADR-0050).
        assert!(env.contains(&format!("token={}", crate::claude_api::DUMMY_TOKEN)), "{env}");
        assert!(!text.contains("tok-abc"), "the token reached the sandbox: {text}");
        assert!(env.contains(&format!("config={}/state/claude", base.display())), "{env}");
        assert!(env.contains(&format!("home={}/state/home", base.display())), "{env}");
        let read = lines.next().unwrap();
        assert!(read.contains("Operation not permitted"), "the sandbox read the secret: {read}");
        let api = lines.next().unwrap();
        let socket = api.split_whitespace().next().unwrap().trim_start_matches("socket=").to_string();
        assert!(socket.starts_with(&*base.join("state/tmp").to_string_lossy()), "{api}");
        assert!(api.ends_with("base=http://api.anthropic.com"), "{api}");
        // The sandbox reaches the proxy, which forwards only the API's /v1/.
        assert_eq!(lines.next().unwrap(), "proxy=403", "{text}");
        let s = f.sessions.snapshot().await;
        let s = s.iter().find(|s| s.id == id).unwrap();
        assert!(s.transcript_path.starts_with(&*base.join("state/claude/projects").to_string_lossy()));
        spawner.kill(&id).await.unwrap();
        log_has(&log, "closed").await;
        for _ in 0..100 {
            if status_of(&f.sessions, &id).await.1 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        for _ in 0..100 {
            if !Path::new(&socket).exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(!Path::new(&socket).exists(), "the proxy outlived its session");

        // Its transcript is in the profile's config directory, so the resume runs there too.
        let transcript = crate::sessions::transcript_path(&base.join("state/claude"), &cwd, &id);
        std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
        std::fs::write(&transcript, "").unwrap();
        std::fs::remove_file(&log).unwrap();
        spawner.resume(&id, &cwd, "go on", "", None).await.unwrap();
        let text = log_has(&log, "go on").await;
        assert!(text.contains(&format!("--resume {id}")), "{text}");
        assert!(text.contains("proxy=403"), "resumed inside the sandbox, with its proxy: {text}");
        spawner.kill(&id).await.unwrap();
    }

    #[tokio::test]
    async fn a_finished_session_resumes_in_place_and_only_when_finished() {
        let f = fixture();
        let id = uuid::Uuid::new_v4().to_string();
        let cwd = f.root.join("repo");
        let cwd_s = cwd.to_string_lossy().into_owned();
        let err = f
            .spawner
            .resume(&id, &cwd_s, "", "", None)
            .await
            .unwrap_err();
        assert!(err.contains("no transcript"), "{err}");

        let transcript = crate::sessions::transcript_path(f.sessions.claude_dir(), &cwd_s, &id);
        std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
        std::fs::write(&transcript, "").unwrap();
        f.spawner
            .resume(&id, &cwd_s, "go on", "", None)
            .await
            .unwrap();
        let log = log_has(&f.log, "go on").await;
        let args = log.lines().next().unwrap();
        assert!(args.contains(&format!("--resume {id}")), "{args}");
        assert!(!args.contains("--session-id"), "{args}");
        assert!(f.sessions.is_spawned(&id).await);

        let err = f
            .spawner
            .resume(&id, &cwd_s, "", "", None)
            .await
            .unwrap_err();
        assert!(err.contains("still running"), "{err}");
        f.spawner.kill(&id).await.unwrap();
    }
}
