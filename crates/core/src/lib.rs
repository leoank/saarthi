//! The saarthi client core: everything a client does against the hub's control service, in
//! Rust, exposed to Kotlin (and later Swift) through UniFFI (ADR-0009). The app keeps the
//! hub's address and its token itself; the core holds the connection, keeps lists and
//! streams typed, and turns events into a conversation.

pub mod conversation;

use std::collections::HashMap;
use std::sync::Arc;

use saarthi_proto::control::v1::control_service_client::ControlServiceClient;
use saarthi_proto::control::v1::*;
use saarthi_proto::event::v1 as ev;
use saarthi_proto::event::v1::resolution::Decision;
use serde_json::Value;
use tokio::sync::watch;
use tonic::transport::Channel;

pub use conversation::{Conversation, Item, ModelTotals, Role};

uniffi::setup_scaffolding!();

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum CoreError {
    /// The hub could not be reached or the connection broke.
    #[error("{reason}")]
    Connection { reason: String },
    /// The hub turned the request away: a wrong token, an unknown request, a node offline.
    #[error("{reason}")]
    Hub { reason: String },
    /// The node refused the command, with its reason, shown to the person as is.
    #[error("{reason}")]
    Refused { reason: String },
}

impl From<tonic::Status> for CoreError {
    fn from(s: tonic::Status) -> Self {
        match s.code() {
            tonic::Code::Unavailable | tonic::Code::Unknown => CoreError::Connection {
                reason: s.message().to_string(),
            },
            // An older hub answers a call it lacks with no message at all.
            tonic::Code::Unimplemented => CoreError::Hub {
                reason: "the hub is older than this app and cannot do this yet; update the hub".into(),
            },
            _ => CoreError::Hub {
                reason: s.message().to_string(),
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct NodeInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub connected: bool,
    pub last_seen_ms: i64,
    /// Directories a session may be spawned under; empty when the node does not allow it
    /// or is offline.
    pub spawn_roots: Vec<String>,
    /// The launch profiles a spawn may name besides the host (ADR-0038); empty when the
    /// node has none or is offline.
    pub profiles: Vec<ProfileInfo>,
    /// The Claude token its spawned sessions run under when none is picked; empty for the
    /// node's own login (ADR-0039).
    pub default_claude_token: String,
}

/// A named Claude token the hub holds (ADR-0039): its name, dates and the account's latest
/// usage seen under it (ADR-0042); never its value.
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ClaudeTokenInfo {
    pub name: String,
    pub first_seen_ms: i64,
    /// Set by hand, else a year after first seen.
    pub expires_ms: i64,
    pub expiry_set: bool,
    /// When a session under it last reported the account's windows; 0 for never.
    pub usage_ms: i64,
    pub five_hour_pct: Option<f64>,
    pub five_hour_resets_s: Option<i64>,
    pub seven_day_pct: Option<f64>,
    pub seven_day_resets_s: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct ProfileInfo {
    pub name: String,
    /// One line from the node's profiles file, for the chooser.
    pub description: String,
    /// Runs confined rather than as the node's user.
    pub sandboxed: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct SessionInfo {
    pub node_id: String,
    pub id: String,
    pub cwd: String,
    pub pid: u32,
    /// Busy or idle, as Claude Code's registry says.
    pub busy: bool,
    /// Under `saarthi run`, so it can be interrupted.
    pub wrapped: bool,
    /// Started headless by its node (ADR-0034), so it can be interrupted.
    pub spawned: bool,
    pub started_ms: i64,
    pub ended_ms: Option<i64>,
    /// The launch profile it runs under; empty for the host.
    pub profile: String,
    /// The named Claude token it last ran under; empty for the node's own login.
    pub claude_token: String,
}

/// A session as a history list shows it (ADR-0036).
#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct HistoryItem {
    pub session: SessionInfo,
    /// The first thing the person typed, or empty when nothing was.
    pub title: String,
    pub input_tokens: u64,
    pub output_tokens: u64,
    /// An estimate from list prices, as the session screen's.
    pub cost_usd: f64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct Question {
    pub question: String,
    pub header: String,
    pub multi_select: bool,
    pub options: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct PendingInfo {
    pub node_id: String,
    pub id: String,
    pub session_id: String,
    pub tool_name: String,
    /// The one input a person decides on: the command, the path, the question.
    pub summary: String,
    pub tool_input_json: String,
    /// The questions of an AskUserQuestion; empty for a permission request.
    pub questions: Vec<Question>,
    pub created_ms: i64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct MetricValue {
    pub name: String,
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record)]
pub struct HealthPoint {
    pub time_ms: i64,
    pub metrics: Vec<MetricValue>,
}

#[derive(Debug, Clone, PartialEq, uniffi::Enum)]
pub enum Change {
    Node {
        node: NodeInfo,
    },
    Session {
        session: SessionInfo,
    },
    PendingOpened {
        pending: PendingInfo,
    },
    PendingClosed {
        node_id: String,
        pending_id: String,
        reason: String,
    },
}

/// Receives the hub's changes to nodes, sessions and pending requests.
#[uniffi::export(with_foreign)]
pub trait WatchListener: Send + Sync {
    fn on_change(&self, change: Change);
    /// The stream ended; list again and watch again.
    fn on_error(&self, message: String);
}

/// Receives a session's conversation, stored items first, then live ones.
#[uniffi::export(with_foreign)]
pub trait SessionListener: Send + Sync {
    /// Items in order; `last_seq` is where to resume after an error.
    fn on_items(&self, items: Vec<Item>, last_seq: u64);
    /// The session's token totals by model, whenever they change.
    fn on_usage(&self, totals: Vec<ModelTotals>);
    fn on_error(&self, message: String);
}

/// A running watch or session stream; dropping it or calling `cancel` stops it.
#[derive(uniffi::Object)]
pub struct Subscription {
    stop: watch::Sender<bool>,
}

#[uniffi::export]
impl Subscription {
    pub fn cancel(&self) {
        let _ = self.stop.send(true);
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        let _ = self.stop.send(true);
    }
}

#[derive(uniffi::Object)]
pub struct Hub {
    client: ControlServiceClient<Channel>,
    bearer: tonic::metadata::MetadataValue<tonic::metadata::Ascii>,
}

#[uniffi::export(async_runtime = "tokio")]
impl Hub {
    /// Connects to the hub's control service, `http://host:port`, with a client token.
    #[uniffi::constructor]
    pub async fn connect(address: String, token: String) -> Result<Arc<Self>, CoreError> {
        // Trim whitespace a paste or an on-screen keyboard may add: a trailing
        // newline or stray space in the token otherwise fails to parse as a
        // header value, and in the address breaks the endpoint URL.
        let address = address.trim().to_string();
        let bearer = format!("Bearer {}", token.trim())
            .parse()
            .map_err(|_| CoreError::Hub {
                reason: "the token is not valid in a header".into(),
            })?;
        // A phone's link dies silently (sleep, a network change, the tailnet relay
        // moving): HTTP/2 pings notice within half a minute and the streams end with an
        // error the app reconnects on, and a connect attempt gives up in seconds rather
        // than hanging until the OS does.
        let channel = tonic::transport::Endpoint::from_shared(address.clone())
            .map_err(|e| CoreError::Connection {
                reason: format!("{address}: {e}"),
            })?
            .connect_timeout(std::time::Duration::from_secs(6))
            .tcp_keepalive(Some(std::time::Duration::from_secs(30)))
            .http2_keep_alive_interval(std::time::Duration::from_secs(15))
            .keep_alive_timeout(std::time::Duration::from_secs(10))
            .keep_alive_while_idle(true)
            .connect()
            .await
            .map_err(|e| CoreError::Connection {
                reason: format!("{address}: {e}"),
            })?;
        let hub = Self {
            client: ControlServiceClient::new(channel),
            bearer,
        };
        // Fail now on a wrong token rather than at the first screen.
        hub.nodes().await?;
        Ok(Arc::new(hub))
    }

    pub async fn nodes(&self) -> Result<Vec<NodeInfo>, CoreError> {
        let r = self
            .client
            .clone()
            .list_nodes(self.req(ListNodesRequest {}))
            .await?;
        Ok(r.into_inner().nodes.into_iter().map(node).collect())
    }

    /// Live sessions, or every session with `include_ended`.
    pub async fn sessions(&self, include_ended: bool) -> Result<Vec<SessionInfo>, CoreError> {
        let req = ListSessionsRequest {
            node_id: String::new(),
            include_ended,
        };
        let r = self.client.clone().list_sessions(self.req(req)).await?;
        Ok(r.into_inner().sessions.into_iter().map(session).collect())
    }

    pub async fn pending(&self) -> Result<Vec<PendingInfo>, CoreError> {
        let r = self
            .client
            .clone()
            .list_pending(self.req(ListPendingRequest {}))
            .await?;
        Ok(r.into_inner()
            .pending
            .into_iter()
            .filter_map(|p| Some(pending(p.node_id, p.request?)))
            .collect())
    }

    /// Health samples of a node since a time.
    pub async fn health(
        &self,
        node_id: String,
        since_ms: i64,
    ) -> Result<Vec<HealthPoint>, CoreError> {
        let req = GetHealthRequest { node_id, since_ms };
        let r = self.client.clone().get_health(self.req(req)).await?;
        Ok(r.into_inner()
            .samples
            .into_iter()
            .map(|s| HealthPoint {
                time_ms: s.time_ms,
                metrics: s
                    .metrics
                    .into_iter()
                    .map(|m| MetricValue {
                        name: m.name,
                        value: m.value,
                    })
                    .collect(),
            })
            .collect())
    }

    /// Allows a held permission request. Returns the node's note, if any.
    pub async fn allow(&self, node_id: String, pending_id: String) -> Result<String, CoreError> {
        self.resolve(node_id, pending_id, Decision::Allow(ev::Allow {}))
            .await
    }

    /// Denies a held permission request; Claude sees the message.
    pub async fn deny(
        &self,
        node_id: String,
        pending_id: String,
        message: String,
    ) -> Result<String, CoreError> {
        self.resolve(node_id, pending_id, Decision::Deny(ev::Deny { message }))
            .await
    }

    /// Answers a held question: for each question text, the chosen labels or one free-text
    /// answer.
    pub async fn answer(
        &self,
        node_id: String,
        pending_id: String,
        answers: HashMap<String, Vec<String>>,
    ) -> Result<String, CoreError> {
        let answers = ev::Answers {
            answers: answers
                .into_iter()
                .map(|(q, values)| (q, ev::AnswerValue { values }))
                .collect(),
        };
        self.resolve(node_id, pending_id, Decision::Answers(answers))
            .await
    }

    pub async fn prompt(
        &self,
        node_id: String,
        session_id: String,
        text: String,
    ) -> Result<String, CoreError> {
        let req = PromptRequest {
            node_id,
            session_id,
            text,
        };
        outcome(
            self.client
                .clone()
                .prompt(self.req(req))
                .await?
                .into_inner(),
        )
    }

    pub async fn interrupt(
        &self,
        node_id: String,
        session_id: String,
        then_prompt: Option<String>,
    ) -> Result<String, CoreError> {
        let req = InterruptRequest {
            node_id,
            session_id,
            then_prompt,
        };
        outcome(
            self.client
                .clone()
                .interrupt(self.req(req))
                .await?
                .into_inner(),
        )
    }

    pub async fn kill(&self, node_id: String, session_id: String) -> Result<String, CoreError> {
        let req = KillRequest {
            node_id,
            session_id,
        };
        outcome(self.client.clone().kill(self.req(req)).await?.into_inner())
    }

    /// Starts a headless session on a node and returns its id (ADR-0034). An empty prompt
    /// starts it idle; an empty model is claude's default; `create` makes a missing
    /// directory under the node's spawn root; an empty `permission_mode` is the user's
    /// default, `bypassPermissions` runs every tool without asking; an empty `profile` runs
    /// it as the node's user, otherwise under that launch profile of the node (ADR-0038).
    /// `claude_token` names the Claude token to run under: None for the node's default,
    /// empty for the node's own login (ADR-0039).
    #[allow(clippy::too_many_arguments)]
    pub async fn spawn(
        &self,
        node_id: String,
        cwd: String,
        prompt: String,
        model: String,
        create: bool,
        permission_mode: String,
        profile: String,
        claude_token: Option<String>,
    ) -> Result<String, CoreError> {
        let req = SpawnRequest {
            node_id,
            cwd,
            prompt,
            model,
            create,
            permission_mode,
            profile,
            claude_token,
        };
        let r = self.client.clone().spawn(self.req(req)).await?.into_inner();
        let id = r.session_id.clone();
        outcome(r).map(|_| id)
    }

    /// Continues a finished session headless, in place (ADR-0036); an empty prompt resumes
    /// it idle. `claude_token` as for `spawn`, except that None is the token the session
    /// last ran under.
    pub async fn resume(
        &self,
        node_id: String,
        session_id: String,
        prompt: String,
        model: String,
        claude_token: Option<String>,
    ) -> Result<String, CoreError> {
        let req = ResumeRequest {
            node_id,
            session_id,
            prompt,
            model,
            claude_token,
        };
        outcome(self.client.clone().resume(self.req(req)).await?.into_inner())
    }

    /// Sessions newest first, live and ended, started before `before_ms` (0 for now), at
    /// most `limit`; `node_id` empty for every node.
    pub async fn history(
        &self,
        node_id: String,
        before_ms: i64,
        limit: u32,
    ) -> Result<Vec<HistoryItem>, CoreError> {
        let req = HistoryRequest {
            node_id,
            before_ms,
            limit,
        };
        let r = self.client.clone().history(self.req(req)).await?.into_inner();
        Ok(r.sessions.into_iter().map(history_item).collect())
    }

    /// Switches a node's keep-awake hold on or off (ADR-0031).
    pub async fn set_keep_awake(&self, node_id: String, on: bool) -> Result<String, CoreError> {
        let req = SetKeepAwakeRequest { node_id, on };
        outcome(
            self.client
                .clone()
                .set_keep_awake(self.req(req))
                .await?
                .into_inner(),
        )
    }

    /// The named Claude tokens the hub holds, with their dates and latest usage.
    pub async fn claude_tokens(&self) -> Result<Vec<ClaudeTokenInfo>, CoreError> {
        let r = self
            .client
            .clone()
            .list_claude_tokens(self.req(ListClaudeTokensRequest {}))
            .await?;
        Ok(r.into_inner().tokens.into_iter().map(claude_token).collect())
    }

    /// Sets a token's expiry by hand, or with 0 goes back to a year after first seen;
    /// returns the tokens as they are now.
    pub async fn set_claude_token_expiry(
        &self,
        name: String,
        expires_ms: i64,
    ) -> Result<Vec<ClaudeTokenInfo>, CoreError> {
        let req = SetClaudeTokenExpiryRequest { name, expires_ms };
        let r = self
            .client
            .clone()
            .set_claude_token_expiry(self.req(req))
            .await?;
        Ok(r.into_inner().tokens.into_iter().map(claude_token).collect())
    }

    /// Sets the token a node's spawned sessions run under when none is picked; empty for
    /// the node's own login.
    pub async fn set_default_claude_token(&self, node_id: String, name: String) -> Result<(), CoreError> {
        let req = SetDefaultClaudeTokenRequest { node_id, name };
        self.client
            .clone()
            .set_default_claude_token(self.req(req))
            .await?;
        Ok(())
    }

    /// Has the hub post notifications to a UnifiedPush endpoint (ADR-0026).
    pub async fn register_push(&self, endpoint: String) -> Result<(), CoreError> {
        let req = RegisterPushRequest {
            endpoint,
            unregister: false,
        };
        self.client.clone().register_push(self.req(req)).await?;
        Ok(())
    }

    pub async fn unregister_push(&self, endpoint: String) -> Result<(), CoreError> {
        let req = RegisterPushRequest {
            endpoint,
            unregister: true,
        };
        self.client.clone().register_push(self.req(req)).await?;
        Ok(())
    }

    /// Calls the listener with every change to nodes, sessions and pending requests until
    /// the subscription is cancelled. List first, then watch.
    pub async fn watch(
        &self,
        listener: Arc<dyn WatchListener>,
    ) -> Result<Arc<Subscription>, CoreError> {
        let mut stream = self
            .client
            .clone()
            .watch(self.req(WatchRequest {}))
            .await?
            .into_inner();
        let (stop, mut stopped) = watch::channel(false);
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = stopped.changed() => return,
                    msg = stream.message() => match msg {
                        Ok(Some(u)) => {
                            if let Some(c) = u.body.and_then(change) {
                                listener.on_change(c);
                            }
                        }
                        Ok(None) => return listener.on_error("the hub closed the stream".into()),
                        Err(s) => return listener.on_error(s.message().to_string()),
                    },
                }
            }
        });
        Ok(Arc::new(Subscription { stop }))
    }

    /// Calls the listener with a session's conversation after `after_seq`: what is stored,
    /// then what happens, until the subscription is cancelled. `totals` seeds the usage
    /// state when resuming after a cached part, so the next turn's line counts only its own
    /// tokens.
    pub async fn stream_session(
        &self,
        node_id: String,
        session_id: String,
        after_seq: u64,
        totals: Vec<ModelTotals>,
        listener: Arc<dyn SessionListener>,
    ) -> Result<Arc<Subscription>, CoreError> {
        let req = StreamSessionRequest {
            node_id,
            session_id,
            after_seq,
        };
        let mut stream = self
            .client
            .clone()
            .stream_session(self.req(req))
            .await?
            .into_inner();
        let (stop, mut stopped) = watch::channel(false);
        tokio::spawn(async move {
            let mut conversation = Conversation::seeded(totals);
            loop {
                tokio::select! {
                    _ = stopped.changed() => return,
                    msg = stream.message() => match msg {
                        Ok(Some(e)) => {
                            let items = conversation.push(&e);
                            if !items.is_empty() {
                                listener.on_items(items, e.seq);
                            }
                            if let Some(totals) = conversation.take_totals() {
                                listener.on_usage(totals);
                            }
                        }
                        Ok(None) => return listener.on_error("the hub closed the stream".into()),
                        Err(s) => return listener.on_error(s.message().to_string()),
                    },
                }
            }
        });
        Ok(Arc::new(Subscription { stop }))
    }
}

impl Hub {
    fn req<T>(&self, body: T) -> tonic::Request<T> {
        let mut r = tonic::Request::new(body);
        r.metadata_mut()
            .insert("authorization", self.bearer.clone());
        r
    }

    async fn resolve(
        &self,
        node_id: String,
        pending_id: String,
        decision: Decision,
    ) -> Result<String, CoreError> {
        let req = ResolveRequest {
            node_id,
            resolution: Some(ev::Resolution {
                pending_id,
                decision: Some(decision),
            }),
        };
        outcome(
            self.client
                .clone()
                .resolve(self.req(req))
                .await?
                .into_inner(),
        )
    }
}

fn outcome(r: CommandResponse) -> Result<String, CoreError> {
    if r.ok {
        Ok(r.note)
    } else {
        Err(CoreError::Refused { reason: r.error })
    }
}

fn node(n: Node) -> NodeInfo {
    NodeInfo {
        id: n.id,
        name: n.name,
        version: n.version,
        connected: n.connected,
        last_seen_ms: n.last_seen_ms,
        spawn_roots: n.spawn_roots,
        profiles: n
            .profiles
            .into_iter()
            .map(|p| ProfileInfo {
                name: p.name,
                description: p.description,
                sandboxed: p.sandboxed,
            })
            .collect(),
        default_claude_token: n.default_claude_token,
    }
}

fn claude_token(t: ClaudeToken) -> ClaudeTokenInfo {
    ClaudeTokenInfo {
        name: t.name,
        first_seen_ms: t.first_seen_ms,
        expires_ms: t.expires_ms,
        expiry_set: t.expiry_set,
        usage_ms: t.usage_ms,
        five_hour_pct: t.five_hour_pct,
        five_hour_resets_s: t.five_hour_resets_s,
        seven_day_pct: t.seven_day_pct,
        seven_day_resets_s: t.seven_day_resets_s,
    }
}

fn history_item(h: SessionHistory) -> HistoryItem {
    const TITLE_CHARS: usize = 300;
    let title = h
        .opening
        .iter()
        .flat_map(conversation::items)
        .find(|i| i.role == Role::User)
        .map(|i| i.text.chars().take(TITLE_CHARS).collect())
        .unwrap_or_default();
    let totals = conversation::totals(&h.usage_json);
    HistoryItem {
        session: session(h.session.unwrap_or_default()),
        title,
        input_tokens: totals.iter().map(|t| t.input_tokens).sum(),
        output_tokens: totals.iter().map(|t| t.output_tokens).sum(),
        cost_usd: totals.iter().map(|t| t.cost_usd).sum(),
    }
}

fn session(s: ev::Session) -> SessionInfo {
    SessionInfo {
        busy: s.status() == ev::SessionStatus::Busy,
        wrapped: s.launch() == ev::Launch::Wrapper,
        spawned: s.launch() == ev::Launch::Spawned,
        node_id: s.node_id,
        id: s.id,
        cwd: s.cwd,
        pid: s.pid,
        started_ms: s.started_ms,
        ended_ms: s.ended_ms,
        profile: s.profile,
        claude_token: s.claude_token,
    }
}

fn pending(node_id: String, p: ev::PendingRequest) -> PendingInfo {
    let input: Value = serde_json::from_str(&p.tool_input_json).unwrap_or_default();
    let questions = input["questions"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|q| Question {
            question: q["question"].as_str().unwrap_or_default().into(),
            header: q["header"].as_str().unwrap_or_default().into(),
            multi_select: q["multiSelect"].as_bool().unwrap_or(false),
            options: q["options"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|o| o["label"].as_str().map(str::to_string))
                .collect(),
        })
        .collect();
    PendingInfo {
        summary: conversation::key_input(&input).unwrap_or_default(),
        node_id,
        id: p.id,
        session_id: p.session_id,
        tool_name: p.tool_name,
        tool_input_json: p.tool_input_json,
        questions,
        created_ms: p.created_ms,
    }
}

fn change(body: update::Body) -> Option<Change> {
    Some(match body {
        update::Body::Node(n) => Change::Node { node: node(n) },
        update::Body::Session(s) => Change::Session {
            session: session(s),
        },
        update::Body::Pending(p) => Change::PendingOpened {
            pending: pending(p.node_id, p.request?),
        },
        update::Body::PendingClosed(c) => {
            let closed = c.closed?;
            Change::PendingClosed {
                node_id: c.node_id,
                reason: closed.reason().as_str_name().to_string(),
                pending_id: closed.pending_id,
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_an_older_hub_lacks_says_so_instead_of_nothing() {
        let e = CoreError::from(tonic::Status::unimplemented(""));
        assert!(matches!(&e, CoreError::Hub { reason } if reason.contains("older than this app")), "{e:?}");
    }

    fn user(seq: u64, content: serde_json::Value, meta: bool) -> ev::Event {
        ev::Event {
            seq,
            source: ev::EventSource::Transcript.into(),
            kind: "user".into(),
            payload_json: serde_json::json!({"isMeta": meta, "message": {"role": "user", "content": content}})
                .to_string(),
            ..Default::default()
        }
    }

    #[test]
    fn a_history_row_takes_the_first_typed_prompt_and_sums_the_usage() {
        let h = SessionHistory {
            session: Some(ev::Session {
                id: "s1".into(),
                ..Default::default()
            }),
            opening: vec![
                user(1, "<command-name>/model</command-name>".into(), false),
                user(2, "Base directory for this skill: /x".into(), true),
                user(3, "fix the flaky test\nin crates/hub".into(), false),
                user(4, "and another".into(), false),
            ],
            usage_json: serde_json::json!({
                "claude-haiku-4-5": {"input_tokens": 1000, "output_tokens": 200},
                "claude-sonnet-4-5": {"input_tokens": 10, "output_tokens": 5},
            })
            .to_string(),
        };
        let item = history_item(h);
        assert_eq!(item.title, "fix the flaky test\nin crates/hub");
        assert_eq!((item.input_tokens, item.output_tokens), (1010, 205));
        assert!(item.cost_usd > 0.0);

        let empty = history_item(SessionHistory::default());
        assert_eq!((empty.title.as_str(), empty.cost_usd), ("", 0.0));
    }
}
