//! The daemon's side of `saarthi run`. Each wrapper connects to a unix socket the daemon
//! serves, says which process it runs, and from then on tells the daemon when a person types
//! at its terminal; the daemon sends it interrupts and kills. Messages are JSON lines.
//!
//! A session is wrapped when its pid in Claude Code's registry is a wrapper's child. For a
//! wrapped session a keystroke releases any permission request held for it, since Claude
//! Code queues that keystroke until the hook returns (EXP-0001), and the hold itself may
//! last as long as the hook allows.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use saarthi_proto::event::v1::{EventSource, ReleaseReason};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Mutex, mpsc};

use crate::outbox::Outbox;
use crate::pending::Pending;
use crate::sessions::Sessions;

/// From a wrapper to the daemon.
#[derive(Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FromWrapper {
    /// First message: the pid of the process the wrapper runs under its PTY, and the named
    /// Claude token it started that process under, if any (ADR-0046), with the socket the
    /// process sends its API requests to, which the daemon serves for as long as this
    /// connection lasts (ADR-0051).
    Hello {
        pid: u32,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        claude_token: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        api_socket: String,
    },
    /// First and only message on a connection of its own, before the wrapper starts its
    /// program: which Claude token to start it under, `name` or the node's default without
    /// one (ADR-0046). The daemon answers with one [`TokenReply`] and closes.
    ClaudeToken { name: Option<String> },
    /// A person typed at the wrapper's terminal.
    Keystroke,
}

/// From the daemon to a wrapper.
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ToWrapper {
    Interrupt { then_prompt: Option<String> },
    /// Typed into the terminal, text then Enter, as if the person had (ADR-0027).
    Prompt { text: String },
    Kill,
}

/// The daemon's answer to [`FromWrapper::ClaudeToken`]: the name of one of the hub's tokens
/// and the socket to start the program with (ADR-0051), or neither for the node's own
/// login; `error` when the hub could not be asked or refused. The token itself never
/// leaves the hub.
#[derive(Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct TokenReply {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub socket: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

/// Where the daemon listens for wrappers unless told otherwise: a directory only this user
/// can enter, `$XDG_RUNTIME_DIR` on Linux and the per-user temporary directory on macOS.
pub fn default_socket() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    base.join(format!("saarthi-{}", unsafe { libc::getuid() }))
        .join("node.sock")
}

pub struct Wrappers {
    sessions: Arc<Sessions>,
    pending: Arc<Pending>,
    outbox: Outbox,
    by_pid: Mutex<HashMap<u32, mpsc::Sender<ToWrapper>>>,
    /// The hub's address and this node's token, once the node links to a hub: where a
    /// wrapper's Claude token comes from.
    hub: std::sync::OnceLock<(String, String)>,
    /// The same hub, relaying the API requests of wrapped programs under its tokens.
    hub_api: std::sync::OnceLock<Arc<crate::claude_api::HubApi>>,
}

impl Wrappers {
    pub fn new(sessions: Arc<Sessions>, pending: Arc<Pending>, outbox: Outbox) -> Arc<Self> {
        Arc::new(Self {
            sessions,
            pending,
            outbox,
            by_pid: Mutex::new(HashMap::new()),
            hub: std::sync::OnceLock::new(),
            hub_api: std::sync::OnceLock::new(),
        })
    }

    /// Where to ask for a wrapper's Claude token: the hub this node links to.
    pub fn set_hub(&self, hub: String, token: String) {
        let _ = self.hub_api.set(crate::claude_api::HubApi::new(hub.clone(), token.clone()));
        let _ = self.hub.set((hub, token));
    }

    async fn claude_token(&self, name: Option<String>) -> TokenReply {
        let Some((hub, token)) = self.hub.get() else {
            return match name {
                None => TokenReply::default(),
                Some(name) => TokenReply {
                    error: Some(format!("this node has no hub to ask for the {name} token")),
                    ..Default::default()
                },
            };
        };
        match crate::link::claude_token(hub, token, name).await {
            Ok(Some(t)) => match crate::claude_api::host_socket() {
                Ok(socket) => TokenReply {
                    name: t.name,
                    socket: Some(socket.to_string_lossy().into_owned()),
                    error: None,
                },
                Err(e) => TokenReply {
                    error: Some(format!("the Claude API socket's directory: {e}")),
                    ..Default::default()
                },
            },
            Ok(None) => TokenReply::default(),
            Err(e) => TokenReply {
                error: Some(e),
                ..Default::default()
            },
        }
    }

    /// Binds the socket (replacing a stale one) in a directory only this user can enter.
    pub fn bind(path: &Path) -> Result<UnixListener> {
        use std::os::unix::fs::PermissionsExt;
        let dir = path
            .parent()
            .context("the wrapper socket needs a directory")?;
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
        let _ = std::fs::remove_file(path);
        UnixListener::bind(path).with_context(|| format!("binding {}", path.display()))
    }

    pub async fn serve(self: Arc<Self>, listener: UnixListener) {
        loop {
            let Ok((stream, _)) = listener.accept().await else {
                continue;
            };
            // The directory keeps other users out; this makes it explicit.
            let same_user = stream
                .peer_cred()
                .is_ok_and(|c| c.uid() == unsafe { libc::getuid() });
            if !same_user {
                continue;
            }
            let this = self.clone();
            tokio::spawn(async move {
                if let Err(e) = this.connection(stream).await {
                    tracing::info!(error = %format!("{e:#}"), "wrapper connection ended");
                }
            });
        }
    }

    async fn connection(&self, stream: UnixStream) -> Result<()> {
        let (read, mut write) = stream.into_split();
        let mut lines = BufReader::new(read).lines();
        let Some(first) = lines.next_line().await? else {
            return Ok(());
        };
        let (pid, claude_token, api_socket) = match serde_json::from_str(&first)? {
            FromWrapper::Hello {
                pid,
                claude_token,
                api_socket,
            } => (pid, claude_token, api_socket),
            FromWrapper::ClaudeToken { name } => {
                let reply = self.claude_token(name).await;
                if let Some(e) = &reply.error {
                    tracing::warn!(error = %e, "no Claude token for a wrapper");
                }
                let mut line = serde_json::to_string(&reply)?;
                line.push('\n');
                write.write_all(line.as_bytes()).await?;
                return Ok(());
            }
            FromWrapper::Keystroke => anyhow::bail!("a wrapper must say hello first"),
        };
        // Held for as long as the wrapper is connected, and started again when it says hello
        // to a restarted daemon.
        let _proxy = self.proxy(&claude_token, &api_socket);
        let (tx, mut rx) = mpsc::channel::<ToWrapper>(8);
        self.by_pid.lock().await.insert(pid, tx);
        self.sessions.set_wrapped(pid, true, &claude_token).await;
        tracing::info!(pid, claude_token = %claude_token, "wrapper connected");

        let outcome = async {
            loop {
                tokio::select! {
                    line = lines.next_line() => match line? {
                        Some(line) => {
                            if serde_json::from_str::<FromWrapper>(&line)? == FromWrapper::Keystroke {
                                self.keystroke(pid).await;
                            }
                        }
                        None => return anyhow::Ok(()),
                    },
                    cmd = rx.recv() => {
                        let Some(cmd) = cmd else { return Ok(()) };
                        let mut line = serde_json::to_string(&cmd)?;
                        line.push('\n');
                        write.write_all(line.as_bytes()).await?;
                    }
                }
            }
        }
        .await;
        self.by_pid.lock().await.remove(&pid);
        self.sessions.set_wrapped(pid, false, "").await;
        tracing::info!(pid, "wrapper disconnected");
        outcome
    }

    /// The proxy a wrapped program under the hub's token `name` sends its API requests to,
    /// on `socket`, a path in the node's socket directory; None, logged, when it cannot be.
    fn proxy(&self, name: &str, socket: &str) -> Option<crate::claude_api::Proxy> {
        if name.is_empty() || socket.is_empty() {
            return None;
        }
        let path = Path::new(socket);
        if path.parent() != Some(crate::claude_api::host_socket_dir().as_path()) {
            tracing::warn!(socket, "refused a wrapper's API socket outside the node's directory");
            return None;
        }
        let Some(api) = self.hub_api.get() else {
            tracing::warn!(claude_token = %name, "no hub to relay a wrapped session's API requests");
            return None;
        };
        let upstream = crate::claude_api::Upstream::Hub {
            api: api.clone(),
            name: name.to_string(),
        };
        match crate::claude_api::Proxy::start(path, upstream) {
            Ok(p) => Some(p),
            Err(e) => {
                tracing::warn!(socket, error = %e, "a wrapped session's API proxy could not start");
                None
            }
        }
    }

    async fn keystroke(&self, pid: u32) {
        if let Some(session_id) = self.sessions.session_for_pid(pid).await {
            self.pending
                .release_session(&session_id, ReleaseReason::Keystroke)
                .await;
        }
    }

    /// Sends an interrupt or a kill to the wrapper of a session.
    pub async fn command(&self, session_id: &str, cmd: ToWrapper) -> Result<(), String> {
        let pid = self.sessions.pid(session_id).await.unwrap_or(0);
        let tx = self.by_pid.lock().await.get(&pid).cloned();
        let Some(tx) = tx else {
            return Err("this session is not under the saarthi wrapper".into());
        };
        let kind = match &cmd {
            ToWrapper::Interrupt { .. } => Some("interrupted"),
            // A typed prompt shows in the transcript as the person's turn.
            ToWrapper::Prompt { .. } => None,
            ToWrapper::Kill => Some("killed"),
        };
        tx.send(cmd)
            .await
            .map_err(|_| "the session's wrapper went away".to_string())?;
        let Some(kind) = kind else {
            return Ok(());
        };
        // No Stop hook fires for an interrupted turn (EXP-0003), so the daemon records it.
        self.outbox
            .event(session_id, EventSource::Daemon, kind, "{}".into())
            .await;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::Record;
    use std::time::Duration;

    #[test]
    fn wire_messages_are_tagged_json_lines() {
        let hello = serde_json::to_string(&FromWrapper::Hello {
            pid: 7,
            claude_token: String::new(),
            api_socket: String::new(),
        })
        .unwrap();
        assert_eq!(hello, r#"{"type":"hello","pid":7}"#);
        let named = FromWrapper::Hello {
            pid: 7,
            claude_token: "work".into(),
            api_socket: "/run/s.sock".into(),
        };
        assert_eq!(
            serde_json::to_string(&named).unwrap(),
            r#"{"type":"hello","pid":7,"claude_token":"work","api_socket":"/run/s.sock"}"#
        );
        let ask = serde_json::to_string(&FromWrapper::ClaudeToken { name: None }).unwrap();
        assert_eq!(ask, r#"{"type":"claude_token","name":null}"#);
        let interrupt = ToWrapper::Interrupt {
            then_prompt: Some("go on".into()),
        };
        assert_eq!(
            serde_json::to_string(&interrupt).unwrap(),
            r#"{"type":"interrupt","then_prompt":"go on"}"#
        );
        let typed = ToWrapper::Prompt { text: "hello".into() };
        assert_eq!(serde_json::to_string(&typed).unwrap(), r#"{"type":"prompt","text":"hello"}"#);
    }

    #[tokio::test]
    async fn a_wrapper_gets_commands_and_its_keystrokes_release_holds() {
        let dir = tempfile::tempdir().unwrap();
        let (outbox, _rx) = Outbox::new(256);
        let sessions = Arc::new(Sessions::new(
            dir.path().to_path_buf(),
            outbox.clone(),
            Duration::from_millis(10),
            Vec::new(),
        ));
        let pending = Pending::new(outbox.clone());
        let wrappers = Wrappers::new(sessions.clone(), pending.clone(), outbox);
        let socket = dir.path().join("w/node.sock");
        tokio::spawn(wrappers.clone().serve(Wrappers::bind(&socket).unwrap()));

        let record = Record {
            pid: 4242,
            session_id: "s1".into(),
            cwd: "/w".into(),
            started_at: 1,
            status: "busy".into(),
            messaging_socket_path: String::new(),
            claude_dir: None,
            profile: String::new(),
        };
        sessions.apply_registry(&[record]).await;
        assert!(
            wrappers.command("s1", ToWrapper::Kill).await.is_err(),
            "not wrapped yet"
        );

        let stream = UnixStream::connect(&socket).await.unwrap();
        let (read, mut write) = stream.into_split();
        let mut from_daemon = BufReader::new(read).lines();
        write
            .write_all(b"{\"type\":\"hello\",\"pid\":4242}\n")
            .await
            .unwrap();
        while !sessions.is_wrapped("s1").await {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        let interrupt = ToWrapper::Interrupt { then_prompt: None };
        wrappers.command("s1", interrupt.clone()).await.unwrap();
        let line = from_daemon.next_line().await.unwrap().unwrap();
        assert_eq!(serde_json::from_str::<ToWrapper>(&line).unwrap(), interrupt);

        // A request held for the session is released by a keystroke at its terminal.
        let input = serde_json::json!({"session_id": "s1", "tool_name": "Bash", "tool_input": {}});
        let hold = {
            let pending = pending.clone();
            tokio::spawn(async move { pending.hold(&input, Duration::from_secs(30)).await })
        };
        while pending.snapshot().await.is_empty() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        write
            .write_all(b"{\"type\":\"keystroke\"}\n")
            .await
            .unwrap();
        assert_eq!(hold.await.unwrap(), serde_json::json!({}));

        drop(write);
        drop(from_daemon);
        while sessions.is_wrapped("s1").await {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
}
