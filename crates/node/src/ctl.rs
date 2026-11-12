//! `saarthi ctl`: a command-line client of the hub's control service, for trying intervention
//! before the phone app exists and for scripts. A node is named by its name or its hub id.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use clap::Subcommand;
use saarthi_proto::control::v1::control_service_client::ControlServiceClient;
use saarthi_proto::control::v1::*;
use saarthi_proto::event::v1::{AnswerValue, Answers, Deny, Resolution, resolution};
use tonic::Request;
use tonic::transport::Channel;

#[derive(Subcommand)]
pub enum Ctl {
    /// Every node the hub knows, connected or not.
    Nodes,
    /// Live sessions, or every session with `--all`.
    Sessions {
        #[arg(long)]
        all: bool,
    },
    /// Open permission requests and questions.
    Pending,
    /// Print changes to nodes, sessions and pending requests until interrupted.
    Watch,
    /// Allow a held permission request.
    Allow { node: String, pending_id: String },
    /// Deny a held permission request; Claude sees the message.
    Deny {
        node: String,
        pending_id: String,
        #[arg(long, default_value = "")]
        message: String,
    },
    /// Answer a held question: `--answer "<question>=<answer>"` once per question, repeated
    /// for each label of a multi-select question.
    Answer {
        node: String,
        pending_id: String,
        #[arg(long = "answer", required = true)]
        answers: Vec<String>,
    },
    /// Send a prompt to a session.
    Prompt {
        node: String,
        session_id: String,
        text: String,
    },
    /// End a session's process: a wrapped or spawned one through the node, a bare one by
    /// SIGTERM.
    Kill { node: String, session_id: String },
    /// Start a headless session on a node, in a directory under one of its spawn roots;
    /// prints the new session's id. Without a prompt it starts idle, waiting for one.
    Spawn {
        node: String,
        cwd: String,
        #[arg(default_value = "")]
        prompt: String,
        /// Create the directory, and its missing parents, if it does not exist.
        #[arg(long)]
        create: bool,
        /// The model, as claude's `--model` takes it; claude's default when omitted.
        #[arg(long, default_value = "")]
        model: String,
        /// The permission mode, as claude's `--permission-mode` takes it (acceptEdits, plan,
        /// bypassPermissions, ...); the user's default when omitted.
        #[arg(long, default_value = "")]
        permission_mode: String,
        /// The node's launch profile, such as a sandbox (ADR-0038); the host when omitted.
        #[arg(long, default_value = "")]
        profile: String,
        #[command(flatten)]
        token: TokenChoice,
    },
    /// Continue a finished session headless, in place; without a prompt it resumes idle.
    Resume {
        node: String,
        session_id: String,
        #[arg(default_value = "")]
        prompt: String,
        /// The model, as claude's `--model` takes it; claude's default when omitted.
        #[arg(long, default_value = "")]
        model: String,
        #[command(flatten)]
        token: TokenChoice,
    },
    /// The named Claude tokens the hub holds (ADR-0039): expiry and the account's latest
    /// usage seen under each.
    Tokens,
    /// Set a Claude token's expiry by hand (`YYYY-MM-DD`), or `clear` it back to a year
    /// after the hub first saw the token.
    TokenExpiry { name: String, date: String },
    /// Set the Claude token a node's spawned sessions run under when none is picked.
    DefaultToken {
        node: String,
        #[arg(required_unless_present = "own_login")]
        name: Option<String>,
        /// The node's own login instead of a token.
        #[arg(long, conflicts_with = "name")]
        own_login: bool,
    },
    /// Past and live sessions, newest first: id, start, directory and first prompt.
    History {
        /// Only this node's sessions.
        #[arg(long)]
        node: Option<String>,
        #[arg(long, default_value_t = 20)]
        limit: u32,
    },
    /// Switch a node's keep-awake hold on or off.
    KeepAwake {
        node: String,
        #[arg(value_parser = ["on", "off"])]
        state: String,
    },
    /// Interrupt a session's turn, optionally typing a prompt after it.
    Interrupt {
        node: String,
        session_id: String,
        #[arg(long)]
        then: Option<String>,
    },
}

/// Which Claude credential a spawn or resume runs under (ADR-0039).
#[derive(Debug, clap::Args)]
pub struct TokenChoice {
    /// A named Claude token the hub holds (`saarthi ctl tokens`). Without it a spawn runs
    /// under the node's default token and a resume under the session's last.
    #[arg(long)]
    claude_token: Option<String>,
    /// The node's own login, not a token.
    #[arg(long, conflicts_with = "claude_token")]
    own_login: bool,
}

impl TokenChoice {
    fn pick(self) -> Option<String> {
        if self.own_login {
            Some(String::new())
        } else {
            self.claude_token
        }
    }
}

/// A Unix time in milliseconds as a UTC date.
fn date(ms: i64) -> String {
    jiff::Timestamp::from_millisecond(ms)
        .map(|t| t.strftime("%Y-%m-%d").to_string())
        .unwrap_or_else(|_| ms.to_string())
}

pub(crate) struct Client {
    pub(crate) inner: ControlServiceClient<Channel>,
    bearer: tonic::metadata::MetadataValue<tonic::metadata::Ascii>,
}

impl Client {
    /// A client of the hub's control service at `hub`, authenticated with `token`.
    pub(crate) async fn connect(hub: &str, token: &str) -> Result<Self> {
        let channel = tonic::transport::Endpoint::from_shared(hub.to_string())
            .context("hub address")?
            .connect_timeout(std::time::Duration::from_secs(6))
            .connect()
            .await
            .with_context(|| format!("connecting to {hub}"))?;
        Ok(Self {
            inner: ControlServiceClient::new(channel),
            bearer: format!("Bearer {token}")
                .parse()
                .context("token is not valid in a header")?,
        })
    }

    pub(crate) fn request<T>(&self, body: T) -> Request<T> {
        let mut r = Request::new(body);
        r.metadata_mut()
            .insert("authorization", self.bearer.clone());
        r
    }

    /// A node's hub id from its name or id.
    async fn node_id(&mut self, node: &str) -> Result<String> {
        let nodes = self
            .inner
            .list_nodes(self.request(ListNodesRequest {}))
            .await?
            .into_inner()
            .nodes;
        match nodes.iter().find(|n| n.id == node || n.name == node) {
            Some(n) => Ok(n.id.clone()),
            None => bail!("no node named {node:?}"),
        }
    }
}

/// `ctl.toml` in the saarthi config directory: the hub and the token file, so neither has to
/// be given on every call. A flag or its environment variable wins over the file.
#[derive(Debug, Default, PartialEq, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub hub: Option<String>,
    pub token_file: Option<PathBuf>,
}

/// `$XDG_CONFIG_HOME/saarthi/ctl.toml`, else `~/.config/saarthi/ctl.toml`, on macOS too, beside
/// the token files the guide puts in `~/.config/saarthi`.
pub fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("saarthi/ctl.toml"))
}

impl Config {
    /// The file at `path`, or an empty config when there is none.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text).with_context(|| format!("reading {}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
        }
    }

    /// The hub and token file from the flags (or their environment variables), else from
    /// this file; `~/` in the token file is the home directory.
    pub fn resolve(self, hub: Option<String>, token_file: Option<PathBuf>, path: &Path) -> Result<(String, PathBuf)> {
        let missing = |what: &str, key: &str| {
            anyhow::anyhow!("no {what}: pass --{what}, or set `{key}` in {}", path.display())
        };
        let hub = hub.or(self.hub).ok_or_else(|| missing("hub", "hub"))?;
        let token_file = token_file
            .or(self.token_file)
            .ok_or_else(|| missing("token-file", "token_file"))?;
        let token_file = match (token_file.strip_prefix("~"), std::env::var_os("HOME")) {
            (Ok(rest), Some(home)) => PathBuf::from(home).join(rest),
            _ => token_file,
        };
        Ok((hub, token_file))
    }
}

pub async fn run(hub: &str, token: &str, cmd: Ctl) -> Result<()> {
    let mut c = Client::connect(hub, token).await?;
    match cmd {
        Ctl::Nodes => {
            let r = c.inner.list_nodes(c.request(ListNodesRequest {})).await?;
            for n in r.into_inner().nodes {
                let state = if n.connected { "connected" } else { "offline" };
                let roots = if n.spawn_roots.is_empty() {
                    String::new()
                } else {
                    format!("  spawn: {}", n.spawn_roots.join(", "))
                };
                let profiles = if n.profiles.is_empty() {
                    String::new()
                } else {
                    let names: Vec<_> = n.profiles.iter().map(|p| p.name.as_str()).collect();
                    format!("  profiles: {}", names.join(", "))
                };
                let token = if n.default_claude_token.is_empty() {
                    String::new()
                } else {
                    format!("  default token: {}", n.default_claude_token)
                };
                let tailnet = if n.tailnet.is_empty() {
                    String::new()
                } else {
                    format!("  tailnet {}: {}", n.tailnet, n.tailnet_peer)
                };
                println!(
                    "{}  {}  {}  {state}{roots}{profiles}{token}{tailnet}",
                    n.id, n.name, n.version
                );
            }
        }
        Ctl::Sessions { all } => {
            let req = ListSessionsRequest {
                node_id: String::new(),
                include_ended: all,
            };
            let r = c.inner.list_sessions(c.request(req)).await?;
            for s in r.into_inner().sessions {
                let ended = if s.ended_ms.is_some() { "  ended" } else { "" };
                println!(
                    "{}  {}  pid {}  {}  {}{ended}",
                    s.node_id,
                    s.id,
                    s.pid,
                    s.status().as_str_name(),
                    s.cwd
                );
            }
        }
        Ctl::Pending => {
            let r = c
                .inner
                .list_pending(c.request(ListPendingRequest {}))
                .await?;
            for p in r.into_inner().pending {
                let q = p.request.unwrap_or_default();
                println!(
                    "{}  {}  {}  {}  {}  {}",
                    p.node_id,
                    q.id,
                    q.session_id,
                    q.kind().as_str_name(),
                    q.tool_name,
                    q.tool_input_json
                );
            }
        }
        Ctl::Watch => {
            let mut s = c
                .inner
                .watch(c.request(WatchRequest {}))
                .await?
                .into_inner();
            while let Some(u) = s.message().await? {
                println!("{:?}", u.body);
            }
        }
        Ctl::Allow { node, pending_id } => {
            let d = resolution::Decision::Allow(Default::default());
            resolve(&mut c, &node, pending_id, d).await?;
        }
        Ctl::Deny {
            node,
            pending_id,
            message,
        } => {
            let d = resolution::Decision::Deny(Deny { message });
            resolve(&mut c, &node, pending_id, d).await?;
        }
        Ctl::Answer {
            node,
            pending_id,
            answers,
        } => {
            let mut map: HashMap<String, AnswerValue> = HashMap::new();
            for a in answers {
                let Some((q, v)) = a.split_once('=') else {
                    bail!("an answer is \"<question>=<answer>\", got {a:?}");
                };
                map.entry(q.to_string())
                    .or_default()
                    .values
                    .push(v.to_string());
            }
            let d = resolution::Decision::Answers(Answers { answers: map });
            resolve(&mut c, &node, pending_id, d).await?;
        }
        Ctl::Prompt {
            node,
            session_id,
            text,
        } => {
            let node_id = c.node_id(&node).await?;
            let req = PromptRequest {
                node_id,
                session_id,
                text,
            };
            let r = c.inner.prompt(c.request(req)).await?.into_inner();
            report(r)?;
        }
        Ctl::Kill { node, session_id } => {
            let node_id = c.node_id(&node).await?;
            let req = KillRequest {
                node_id,
                session_id,
            };
            report(c.inner.kill(c.request(req)).await?.into_inner())?;
        }
        Ctl::Spawn {
            node,
            cwd,
            prompt,
            create,
            model,
            permission_mode,
            profile,
            token,
        } => {
            let node_id = c.node_id(&node).await?;
            let req = SpawnRequest {
                node_id,
                cwd,
                prompt,
                model,
                create,
                permission_mode,
                profile,
                claude_token: token.pick(),
            };
            let r = c.inner.spawn(c.request(req)).await?.into_inner();
            if !r.ok {
                bail!("refused: {}", r.error);
            }
            println!("{}", r.session_id);
        }
        Ctl::Resume {
            node,
            session_id,
            prompt,
            model,
            token,
        } => {
            let node_id = c.node_id(&node).await?;
            let req = ResumeRequest {
                node_id,
                session_id,
                prompt,
                model,
                claude_token: token.pick(),
            };
            report(c.inner.resume(c.request(req)).await?.into_inner())?;
        }
        Ctl::Tokens => {
            let r = c
                .inner
                .list_claude_tokens(c.request(ListClaudeTokensRequest {}))
                .await?;
            for t in r.into_inner().tokens {
                println!("{}", token_line(&t, saarthi_node::outbox::now_ms()));
            }
        }
        Ctl::TokenExpiry { name, date } => {
            let expires_ms = if date == "clear" {
                0
            } else {
                let d: jiff::civil::Date = date
                    .parse()
                    .with_context(|| format!("{date:?} is not a date (YYYY-MM-DD) or clear"))?;
                d.to_zoned(jiff::tz::TimeZone::UTC)?
                    .timestamp()
                    .as_millisecond()
            };
            let req = SetClaudeTokenExpiryRequest { name, expires_ms };
            let r = c.inner.set_claude_token_expiry(c.request(req)).await?;
            for t in r.into_inner().tokens {
                println!("{}", token_line(&t, saarthi_node::outbox::now_ms()));
            }
        }
        Ctl::DefaultToken {
            node,
            name,
            own_login: _,
        } => {
            let node_id = c.node_id(&node).await?;
            let req = SetDefaultClaudeTokenRequest {
                node_id,
                name: name.unwrap_or_default(),
            };
            c.inner.set_default_claude_token(c.request(req)).await?;
        }
        Ctl::History { node, limit } => {
            let node_id = match node {
                Some(n) => c.node_id(&n).await?,
                None => String::new(),
            };
            let req = HistoryRequest {
                node_id,
                before_ms: 0,
                limit,
            };
            for h in c.inner.history(c.request(req)).await?.into_inner().sessions {
                let s = h.session.unwrap_or_default();
                let first = h
                    .opening
                    .iter()
                    .find_map(|e| first_prompt(&e.payload_json))
                    .unwrap_or_default();
                let state = if s.ended_ms.is_some() { "ended" } else { "live" };
                println!("{}  {}  {}  {state}  {}  {first}", s.node_id, s.id, s.started_ms, s.cwd);
            }
        }
        Ctl::KeepAwake { node, state } => {
            let node_id = c.node_id(&node).await?;
            let req = SetKeepAwakeRequest {
                node_id,
                on: state == "on",
            };
            report(c.inner.set_keep_awake(c.request(req)).await?.into_inner())?;
        }
        Ctl::Interrupt {
            node,
            session_id,
            then,
        } => {
            let node_id = c.node_id(&node).await?;
            let req = InterruptRequest {
                node_id,
                session_id,
                then_prompt: then,
            };
            let r = c.inner.interrupt(c.request(req)).await?.into_inner();
            report(r)?;
        }
    }
    Ok(())
}

async fn resolve(
    c: &mut Client,
    node: &str,
    pending_id: String,
    decision: resolution::Decision,
) -> Result<()> {
    let node_id = c.node_id(node).await?;
    let req = ResolveRequest {
        node_id,
        resolution: Some(Resolution {
            pending_id,
            decision: Some(decision),
        }),
    };
    report(c.inner.resolve(c.request(req)).await?.into_inner())
}

/// `work  expires 2027-11-11 (set)  5h 4%  7d 36%  (2 min ago)`
fn token_line(t: &ClaudeToken, now_ms: i64) -> String {
    let set = if t.expiry_set { " (set)" } else { "" };
    let mut line = format!("{}  expires {}{set}", t.name, date(t.expires_ms));
    if t.usage_ms > 0 {
        let pct = |p: Option<f64>| p.map(|p| format!("{p:.0}%")).unwrap_or_else(|| "-".into());
        let age = (now_ms - t.usage_ms).max(0) / 60_000;
        line += &format!(
            "  5h {}  7d {}  ({age} min ago)",
            pct(t.five_hour_pct),
            pct(t.seven_day_pct)
        );
    }
    line
}

fn report(r: CommandResponse) -> Result<()> {
    if !r.ok {
        bail!("the node refused: {}", r.error);
    }
    if r.note.is_empty() {
        println!("ok");
    } else {
        println!("ok: {}", r.note);
    }
    Ok(())
}

/// The typed text of a transcript `user` line, first line only, for a one-line listing;
/// the app's core reads these lines in full.
fn first_prompt(payload: &str) -> Option<String> {
    let v: serde_json::Value = serde_json::from_str(payload).ok()?;
    if v["isMeta"].as_bool().unwrap_or(false) {
        return None;
    }
    let text = v["message"]["content"].as_str()?.trim();
    (!text.is_empty() && !text.starts_with('<')).then(|| text.lines().next().unwrap_or(text).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_fills_what_the_flags_leave_out() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ctl.toml");
        std::fs::write(&path, "hub = \"http://hub:7700\"\ntoken_file = \"~/.config/saarthi/client-token\"\n").unwrap();
        let home = PathBuf::from(std::env::var_os("HOME").unwrap());

        let (hub, token) = Config::load(&path).unwrap().resolve(None, None, &path).unwrap();
        assert_eq!(hub, "http://hub:7700");
        assert_eq!(token, home.join(".config/saarthi/client-token"));

        let (hub, token) = Config::load(&path)
            .unwrap()
            .resolve(Some("http://other:7700".into()), Some("/tmp/t".into()), &path)
            .unwrap();
        assert_eq!((hub.as_str(), token), ("http://other:7700", PathBuf::from("/tmp/t")));
    }

    #[test]
    fn no_config_and_no_flags_names_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("ctl.toml");
        assert_eq!(Config::load(&path).unwrap(), Config::default());
        let err = Config::default().resolve(None, Some("/t".into()), &path).unwrap_err().to_string();
        assert!(err.contains("--hub") && err.contains("ctl.toml"), "{err}");
        std::fs::write(&path, "hubb = \"x\"\n").unwrap();
        assert!(Config::load(&path).is_err());
    }
}
