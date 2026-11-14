//! The client-facing API: the hub's view across every node, a session's events as they
//! happen, health history, and the commands a person sends from the phone. Clients
//! authenticate with a pre-shared token, as nodes do, from their own token file.

use std::pin::Pin;

use saarthi_proto::control::v1::control_service_server::ControlService;
use saarthi_proto::control::v1::*;
use saarthi_proto::event::v1::{Event, resolution};
use saarthi_proto::node::v1::{
    self as node, CommandResult, Interrupt, Kill, Prompt, Resume, SetKeepAwake, Spawn, command,
};
use tokio::sync::broadcast::error::RecvError;
use tokio_stream::Stream;
use tonic::{Request, Response, Status};

use crate::node_service::Hub;

type Stream_<T> = Pin<Box<dyn Stream<Item = Result<T, Status>> + Send>>;

impl Hub {
    fn client<T>(&self, request: &Request<T>) -> Result<String, Status> {
        let auth = request
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        self.client_tokens
            .name_for(auth)
            .map(str::to_string)
            .ok_or_else(|| Status::unauthenticated("unknown client token"))
    }
}

impl Hub {
    /// A node as clients see it: its record, and what its live link registered.
    pub(crate) async fn node_view(&self, row: crate::db::NodeRow) -> Node {
        let crate::db::NodeRow {
            id,
            name,
            version,
            last_seen_ms,
            default_claude_token,
            tailnet,
            tailnet_peer,
        } = row;
        let spawning = self.live.spawning(&id).await;
        let connected = spawning.is_some();
        let (spawn_roots, profiles) = spawning.unwrap_or_default();
        Node {
            id,
            name,
            version,
            connected,
            last_seen_ms,
            spawn_roots,
            profiles,
            default_claude_token,
            tailnet,
            tailnet_peer,
        }
    }

    /// The token to name to a node for a spawn or resume under `name`, without its value
    /// (ADR-0051); None for the node's own login (the empty name). `why` says where the
    /// name came from, for the error.
    fn claude_token(&self, name: &str, why: &str) -> Result<Option<node::ClaudeToken>, Status> {
        if name.is_empty() {
            return Ok(None);
        }
        match self.claude_tokens.get(name) {
            Some(_) => Ok(Some(node::ClaudeToken {
                name: name.to_string(),
                token: String::new(),
            })),
            None => Err(Status::failed_precondition(format!(
                "the hub holds no Claude token named {name} ({why})"
            ))),
        }
    }

    async fn token_list(&self) -> Result<Response<ListClaudeTokensResponse>, Status> {
        let names: Vec<&str> = self.claude_tokens.names().collect();
        let tokens = self.db.claude_tokens(&names).await.map_err(internal)?;
        Ok(Response::new(ListClaudeTokensResponse { tokens }))
    }
}

fn internal(e: anyhow::Error) -> Status {
    tracing::warn!(error = %format!("{e:#}"), "control request failed");
    Status::internal("the hub could not read its record")
}

fn response(r: CommandResult) -> Response<CommandResponse> {
    Response::new(CommandResponse {
        ok: r.ok,
        error: r.error,
        note: r.note,
        session_id: String::new(),
    })
}

#[tonic::async_trait]
impl ControlService for Hub {
    async fn list_nodes(
        &self,
        request: Request<ListNodesRequest>,
    ) -> Result<Response<ListNodesResponse>, Status> {
        self.client(&request)?;
        let mut nodes = Vec::new();
        for row in self.db.nodes().await.map_err(internal)? {
            nodes.push(self.node_view(row).await);
        }
        Ok(Response::new(ListNodesResponse { nodes }))
    }

    async fn list_sessions(
        &self,
        request: Request<ListSessionsRequest>,
    ) -> Result<Response<ListSessionsResponse>, Status> {
        self.client(&request)?;
        let r = request.into_inner();
        let sessions = self
            .db
            .sessions(&r.node_id, r.include_ended)
            .await
            .map_err(internal)?;
        Ok(Response::new(ListSessionsResponse { sessions }))
    }

    async fn list_pending(
        &self,
        request: Request<ListPendingRequest>,
    ) -> Result<Response<ListPendingResponse>, Status> {
        self.client(&request)?;
        let pending = self
            .db
            .open_pendings()
            .await
            .map_err(internal)?
            .into_iter()
            .map(|(node_id, request)| PendingOnNode {
                node_id,
                request: Some(request),
            })
            .collect();
        Ok(Response::new(ListPendingResponse { pending }))
    }

    type WatchStream = Stream_<Update>;

    async fn watch(
        &self,
        request: Request<WatchRequest>,
    ) -> Result<Response<Self::WatchStream>, Status> {
        let client = self.client(&request)?;
        let mut updates = self.live.subscribe();
        let stream = async_stream(move |tx| async move {
            loop {
                match updates.recv().await {
                    Ok(u) => {
                        if tx.send(Ok(u)).await.is_err() {
                            return;
                        }
                    }
                    // A client too slow for the stream lists again and resubscribes.
                    Err(RecvError::Lagged(n)) => {
                        tracing::info!(%client, missed = n, "watcher lagged");
                        let _ = tx
                            .send(Err(Status::data_loss("missed updates; list again")))
                            .await;
                        return;
                    }
                    Err(RecvError::Closed) => return,
                }
            }
        });
        Ok(Response::new(stream))
    }

    type StreamSessionStream = Stream_<Event>;

    async fn stream_session(
        &self,
        request: Request<StreamSessionRequest>,
    ) -> Result<Response<Self::StreamSessionStream>, Status> {
        self.client(&request)?;
        let r = request.into_inner();
        // Subscribe before reading the record, so nothing stored in between is missed; the
        // live part skips what the stored part already sent.
        let mut live = self.live.subscribe_events();
        let db = self.db.clone();
        let stream = async_stream(move |tx| async move {
            let mut last = r.after_seq;
            loop {
                let stored = match db
                    .session_events(&r.node_id, &r.session_id, last, 500)
                    .await
                {
                    Ok(stored) => stored,
                    Err(e) => {
                        let _ = tx.send(Err(internal(e))).await;
                        return;
                    }
                };
                let done = stored.len() < 500;
                for e in stored {
                    last = e.seq;
                    if tx.send(Ok(e)).await.is_err() {
                        return;
                    }
                }
                if done {
                    break;
                }
            }
            loop {
                match live.recv().await {
                    Ok((node_id, e)) => {
                        if node_id == r.node_id && e.session_id == r.session_id && e.seq > last {
                            last = e.seq;
                            if tx.send(Ok(e)).await.is_err() {
                                return;
                            }
                        }
                    }
                    Err(RecvError::Lagged(_)) => {
                        let _ = tx
                            .send(Err(Status::data_loss(
                                "fell behind; stream again from the last seq",
                            )))
                            .await;
                        return;
                    }
                    Err(RecvError::Closed) => return,
                }
            }
        });
        Ok(Response::new(stream))
    }

    async fn get_health(
        &self,
        request: Request<GetHealthRequest>,
    ) -> Result<Response<GetHealthResponse>, Status> {
        self.client(&request)?;
        let r = request.into_inner();
        let samples = self
            .db
            .health_since(&r.node_id, r.since_ms)
            .await
            .map_err(internal)?;
        Ok(Response::new(GetHealthResponse { samples }))
    }

    async fn resolve(
        &self,
        request: Request<ResolveRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        let client = self.client(&request)?;
        let r = request.into_inner();
        let resolution = r
            .resolution
            .ok_or_else(|| Status::invalid_argument("no resolution"))?;
        let Some(pending) = self
            .db
            .open_pending(&r.node_id, &resolution.pending_id)
            .await
            .map_err(internal)?
        else {
            return Err(Status::not_found("no such open request"));
        };
        let decision = match &resolution.decision {
            Some(resolution::Decision::Allow(_)) => "allow",
            Some(resolution::Decision::Deny(_)) => "deny",
            Some(resolution::Decision::Answers(_)) => "answers",
            None => "none",
        };
        let result = self
            .live
            .command(
                &r.node_id,
                &pending.session_id,
                command::Action::Resolve(resolution.clone()),
            )
            .await?;
        if result.ok {
            let json = serde_json::json!({ "by": client, "decision": decision }).to_string();
            self.db
                .record_resolution(&r.node_id, &resolution.pending_id, &json)
                .await
                .map_err(internal)?;
        }
        Ok(response(result))
    }

    async fn prompt(
        &self,
        request: Request<PromptRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        self.client(&request)?;
        let r = request.into_inner();
        let action = command::Action::Prompt(Prompt { text: r.text });
        Ok(response(
            self.live.command(&r.node_id, &r.session_id, action).await?,
        ))
    }

    async fn interrupt(
        &self,
        request: Request<InterruptRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        self.client(&request)?;
        let r = request.into_inner();
        let action = command::Action::Interrupt(Interrupt {
            then_prompt: r.then_prompt,
        });
        Ok(response(
            self.live.command(&r.node_id, &r.session_id, action).await?,
        ))
    }

    async fn kill(
        &self,
        request: Request<KillRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        self.client(&request)?;
        let r = request.into_inner();
        let action = command::Action::Kill(Kill {});
        Ok(response(
            self.live.command(&r.node_id, &r.session_id, action).await?,
        ))
    }

    async fn spawn(
        &self,
        request: Request<SpawnRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        self.client(&request)?;
        let r = request.into_inner();
        // A token picked, else the node's default (ADR-0039).
        let (name, why) = match r.claude_token {
            Some(name) => (name, "picked"),
            None => {
                let default = self
                    .db
                    .default_claude_token(&r.node_id)
                    .await
                    .map_err(internal)?;
                (
                    default.unwrap_or_default(),
                    "the node's default; pick another or change it",
                )
            }
        };
        let claude_token = self.claude_token(&name, why)?;
        // The hub names the session, so the client can open it as soon as this returns.
        let session_id = uuid::Uuid::new_v4().to_string();
        let action = command::Action::Spawn(Spawn {
            cwd: r.cwd,
            prompt: r.prompt,
            model: r.model,
            create: r.create,
            permission_mode: r.permission_mode,
            profile: r.profile,
            claude_token,
        });
        let result = self.live.command(&r.node_id, &session_id, action).await?;
        let mut response = response(result);
        if response.get_ref().ok {
            self.db
                .set_session_claude_token(&r.node_id, &session_id, &name)
                .await
                .map_err(internal)?;
            response.get_mut().session_id = session_id;
        }
        Ok(response)
    }

    async fn resume(
        &self,
        request: Request<ResumeRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        self.client(&request)?;
        let r = request.into_inner();
        // The node may have restarted since the session ran, so the hub says where it ran.
        let Some(session) = self
            .db
            .session(&r.node_id, &r.session_id)
            .await
            .map_err(internal)?
        else {
            return Err(Status::not_found("no such session on this node"));
        };
        if session.ended_ms.is_none() {
            return Err(Status::failed_precondition("the session is still running"));
        }
        // A token picked, else the one the session last ran under (ADR-0039).
        let (name, why) = match r.claude_token {
            Some(name) => (name, "picked"),
            None => (session.claude_token, "the session's last; pick another"),
        };
        let claude_token = self.claude_token(&name, why)?;
        let action = command::Action::Resume(Resume {
            cwd: session.cwd,
            prompt: r.prompt,
            model: r.model,
            claude_token,
        });
        let result = self.live.command(&r.node_id, &r.session_id, action).await?;
        if result.ok {
            self.db
                .set_session_claude_token(&r.node_id, &r.session_id, &name)
                .await
                .map_err(internal)?;
        }
        Ok(response(result))
    }

    async fn history(
        &self,
        request: Request<HistoryRequest>,
    ) -> Result<Response<HistoryResponse>, Status> {
        /// User lines to look through for a first prompt: the first few can be slash
        /// commands, skill bodies or reminders rather than anything typed.
        const OPENING: i64 = 8;
        /// A prompt pasted longer than this is passed over for the title; it keeps a page of
        /// fifty sessions well under the client's 4 MB message limit.
        const OPENING_BYTES: i64 = 16 * 1024;
        self.client(&request)?;
        let r = request.into_inner();
        let before = if r.before_ms > 0 { r.before_ms } else { i64::MAX };
        let limit = if r.limit > 0 { r.limit.min(200) } else { 50 };
        let db = &self.db;
        let mut sessions = Vec::new();
        for session in db.history(&r.node_id, before, limit.into()).await.map_err(internal)? {
            let opening = db
                .opening(&session.node_id, &session.id, OPENING, OPENING_BYTES)
                .await
                .map_err(internal)?;
            let usage_json = db
                .last_usage(&session.node_id, &session.id)
                .await
                .map_err(internal)?
                .unwrap_or_default();
            sessions.push(SessionHistory {
                session: Some(session),
                opening,
                usage_json,
            });
        }
        Ok(Response::new(HistoryResponse { sessions }))
    }

    async fn set_keep_awake(
        &self,
        request: Request<SetKeepAwakeRequest>,
    ) -> Result<Response<CommandResponse>, Status> {
        self.client(&request)?;
        let r = request.into_inner();
        let action = command::Action::SetKeepAwake(SetKeepAwake { on: r.on });
        Ok(response(self.live.command(&r.node_id, "", action).await?))
    }

    async fn list_claude_tokens(
        &self,
        request: Request<ListClaudeTokensRequest>,
    ) -> Result<Response<ListClaudeTokensResponse>, Status> {
        self.client(&request)?;
        self.token_list().await
    }

    async fn set_claude_token_expiry(
        &self,
        request: Request<SetClaudeTokenExpiryRequest>,
    ) -> Result<Response<ListClaudeTokensResponse>, Status> {
        let client = self.client(&request)?;
        let r = request.into_inner();
        if self.claude_tokens.get(&r.name).is_none() {
            return Err(Status::not_found(format!(
                "the hub holds no Claude token named {}",
                r.name
            )));
        }
        let expires = (r.expires_ms > 0).then_some(r.expires_ms);
        self.db
            .set_claude_token_expiry(&r.name, expires)
            .await
            .map_err(internal)?;
        tracing::info!(%client, token = %r.name, expires_ms = r.expires_ms, "Claude token expiry set");
        self.token_list().await
    }

    async fn set_default_claude_token(
        &self,
        request: Request<SetDefaultClaudeTokenRequest>,
    ) -> Result<Response<SetDefaultClaudeTokenResponse>, Status> {
        let client = self.client(&request)?;
        let r = request.into_inner();
        if !r.name.is_empty() && self.claude_tokens.get(&r.name).is_none() {
            return Err(Status::not_found(format!(
                "the hub holds no Claude token named {}",
                r.name
            )));
        }
        if !self
            .db
            .set_default_claude_token(&r.node_id, &r.name)
            .await
            .map_err(internal)?
        {
            return Err(Status::not_found("no such node"));
        }
        tracing::info!(%client, node = %r.node_id, token = %r.name, "default Claude token set");
        if let Some(row) = self.db.node(&r.node_id).await.map_err(internal)? {
            let node = self.node_view(row).await;
            self.live.publish(Update {
                body: Some(update::Body::Node(node)),
            });
        }
        Ok(Response::new(SetDefaultClaudeTokenResponse {}))
    }

    async fn register_push(
        &self,
        request: Request<RegisterPushRequest>,
    ) -> Result<Response<RegisterPushResponse>, Status> {
        let client = self.client(&request)?;
        let r = request.into_inner();
        let url = reqwest::Url::parse(&r.endpoint)
            .map_err(|_| Status::invalid_argument("the endpoint is not a URL"))?;
        if !matches!(url.scheme(), "http" | "https") {
            return Err(Status::invalid_argument(
                "the endpoint must be http or https",
            ));
        }
        if r.unregister {
            self.db
                .remove_push_endpoint(&r.endpoint)
                .await
                .map_err(internal)?;
        } else {
            self.db
                .add_push_endpoint(&client, &r.endpoint, crate::node_service::now_ms())
                .await
                .map_err(internal)?;
        }
        tracing::info!(%client, unregister = r.unregister, "push endpoint");
        Ok(Response::new(RegisterPushResponse {}))
    }
}

/// A response stream fed by a task, which ends when the client goes away.
fn async_stream<T, F, Fut>(f: F) -> Stream_<T>
where
    T: Send + 'static,
    F: FnOnce(tokio::sync::mpsc::Sender<Result<T, Status>>) -> Fut,
    Fut: std::future::Future<Output = ()> + Send + 'static,
{
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    tokio::spawn(f(tx));
    Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
}
