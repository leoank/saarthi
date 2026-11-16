//! The daemon-facing `Link` stream: authenticate by token, register, then store what the node
//! sends and ack every event once it is stored. Pending requests and sessions are also
//! published to watching clients; commands from clients go out on the same stream
//! (`Live::command`) and their results come back here.

use std::pin::Pin;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Result, bail};
use saarthi_proto::control::v1::{PendingClosedOnNode, PendingOnNode, Update, update};
use saarthi_proto::event::v1::{EventSource, PendingClosed, ReleaseReason, Session};
use saarthi_proto::node::v1::node_service_server::NodeService;
use saarthi_proto::node::v1::{
    Ack, ClaudeApiRequest, ClaudeToken, ServiceUp, ClaudeTokenReply, ClaudeTokenRequest, HubMessage, NodeMessage,
    Registered, SessionTranscriptPart, SessionTranscriptRequest,
    hub_message, node_message::Body,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tokio_stream::{Stream, StreamExt};
use tonic::{Request, Response, Status, Streaming};

use crate::claude_tokens::ClaudeTokens;
use crate::db::Db;
use crate::live::{Live, ToNode};
use crate::notify::Notifier;
use crate::tokens::Tokens;

/// The hub's state, shared by the node and the control service.
#[derive(Clone)]
pub struct Hub {
    pub db: Db,
    pub node_tokens: Arc<Tokens>,
    pub client_tokens: Arc<Tokens>,
    /// The named Claude tokens from `--claude-tokens` (ADR-0039); empty without the flag.
    pub claude_tokens: Arc<ClaudeTokens>,
    /// Relays nodes' Claude API requests under those tokens (ADR-0051).
    pub claude_api: Arc<crate::claude_api::ClaudeApi>,
    pub live: Arc<Live>,
    /// Push to registered phones and to the ntfy topic, if one is configured.
    pub notifier: Arc<Notifier>,
}

#[tonic::async_trait]
impl NodeService for Hub {
    type LinkStream = Pin<Box<dyn Stream<Item = Result<HubMessage, Status>> + Send>>;

    async fn link(
        &self,
        request: Request<Streaming<NodeMessage>>,
    ) -> Result<Response<Self::LinkStream>, Status> {
        let auth = request
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        let Some(name) = self.tokens_name(auth) else {
            return Err(Status::unauthenticated("unknown node token"));
        };
        let (tx, rx) = mpsc::channel(256);
        let hub = self.clone();
        // Set when the link came over one of the hub's tailnets (ADR-0048).
        let remote = request.extensions().get::<saarthi_tailnet::Remote>().cloned();
        let inbound = request.into_inner();
        tokio::spawn(async move {
            if let Err(e) = serve_node(&hub, &name, remote, inbound, &tx).await {
                tracing::warn!(node = %name, error = %format!("{e:#}"), "node link ended");
                let _ = tx
                    .send(Err(Status::internal("the hub could not record the stream")))
                    .await;
            }
        });
        Ok(Response::new(Box::pin(ReceiverStream::new(rx))))
    }

    /// For a session started at the node's own terminal (ADR-0046): the token named, or the
    /// node's default without a name; none for its own login. The name only: the node
    /// relays the session's requests through `ClaudeApi` (ADR-0051).
    async fn claude_token(
        &self,
        request: Request<ClaudeTokenRequest>,
    ) -> Result<Response<ClaudeTokenReply>, Status> {
        let auth = request
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        let Some(node) = self.tokens_name(auth) else {
            return Err(Status::unauthenticated("unknown node token"));
        };
        let name = match request.into_inner().name {
            Some(name) => name,
            None => {
                let internal = |e: anyhow::Error| Status::internal(format!("{e:#}"));
                match self.db.node_id(&node).await.map_err(internal)? {
                    Some(id) => self
                        .db
                        .default_claude_token(&id)
                        .await
                        .map_err(internal)?
                        .unwrap_or_default(),
                    None => String::new(),
                }
            }
        };
        if name.is_empty() {
            return Ok(Response::new(ClaudeTokenReply { claude_token: None }));
        }
        if self.claude_tokens.get(&name).is_none() {
            return Err(Status::not_found(format!(
                "the hub holds no Claude token named {name}"
            )));
        }
        tracing::info!(%node, claude_token = %name, "named a Claude token for a terminal session");
        Ok(Response::new(ClaudeTokenReply {
            claude_token: Some(ClaudeToken {
                name,
                token: String::new(),
            }),
        }))
    }

    type ClaudeApiStream = crate::claude_api::ResponseStream;

    async fn claude_api(
        &self,
        request: Request<Streaming<ClaudeApiRequest>>,
    ) -> Result<Response<Self::ClaudeApiStream>, Status> {
        let auth = request
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        let Some(node) = self.tokens_name(auth) else {
            return Err(Status::unauthenticated("unknown node token"));
        };
        crate::claude_api::relay(self, &node, request.into_inner())
            .await
            .map(Response::new)
    }

    type ServiceStreamStream = crate::services::ToNodeStream;

    async fn service_stream(
        &self,
        request: Request<Streaming<ServiceUp>>,
    ) -> Result<Response<Self::ServiceStreamStream>, Status> {
        let auth = request
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        let Some(node) = self.tokens_name(auth) else {
            return Err(Status::unauthenticated("unknown node token"));
        };
        let node_id = self
            .db
            .node_id(&node)
            .await
            .map_err(|e| Status::internal(format!("{e:#}")))?
            .ok_or_else(|| Status::failed_precondition("the node has not registered"))?;
        crate::services::stream(self, &node_id, request.into_inner())
            .await
            .map(Response::new)
    }

    type SessionTranscriptStream = ReceiverStream<Result<SessionTranscriptPart, Status>>;

    async fn session_transcript(
        &self,
        request: Request<SessionTranscriptRequest>,
    ) -> Result<Response<Self::SessionTranscriptStream>, Status> {
        /// Rows read at a time, and roughly the most a part carries; a line longer than
        /// that goes in a part of its own.
        const PAGE: i64 = 500;
        const PART_BYTES: usize = 2 << 20;
        let auth = request
            .metadata()
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or_default();
        let Some(node) = self.tokens_name(auth) else {
            return Err(Status::unauthenticated("unknown node token"));
        };
        let internal = |e: anyhow::Error| Status::internal(format!("{e:#}"));
        let node_id = self
            .db
            .node_id(&node)
            .await
            .map_err(internal)?
            .ok_or_else(|| Status::failed_precondition("the node has not registered"))?;
        let session_id = request.into_inner().session_id;
        let (tx, rx) = mpsc::channel(4);
        let db = self.db.clone();
        tokio::spawn(async move {
            let mut after = 0;
            let mut part = Vec::new();
            let mut size = 0;
            loop {
                let rows = match db.transcript(&node_id, &session_id, after, PAGE).await {
                    Ok(rows) => rows,
                    Err(e) => {
                        let _ = tx.send(Err(Status::internal(format!("{e:#}")))).await;
                        return;
                    }
                };
                let done = (rows.len() as i64) < PAGE;
                for (seq, line) in rows {
                    after = seq;
                    if size + line.len() > PART_BYTES && !part.is_empty() {
                        let lines = std::mem::take(&mut part);
                        if tx.send(Ok(SessionTranscriptPart { lines })).await.is_err() {
                            return;
                        }
                        size = 0;
                    }
                    size += line.len();
                    part.push(line);
                }
                if done {
                    break;
                }
            }
            if !part.is_empty() {
                let _ = tx.send(Ok(SessionTranscriptPart { lines: part })).await;
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

impl Hub {
    fn tokens_name(&self, auth: &str) -> Option<String> {
        self.node_tokens.name_for(auth).map(str::to_string)
    }
}

async fn serve_node(
    hub: &Hub,
    name: &str,
    remote: Option<saarthi_tailnet::Remote>,
    mut inbound: Streaming<NodeMessage>,
    tx: &ToNode,
) -> Result<()> {
    let db = &hub.db;
    let Some(Body::Register(register)) = inbound.message().await?.and_then(|m| m.body) else {
        bail!("the first message was not a registration");
    };
    if register.name != name {
        tracing::info!(node = %name, claimed = %register.name, "node registered under another name than its token's");
    }
    let (node_id, last_seq) = db.register_node(name, &register.version, now_ms()).await?;
    let (tailnet, peer) = match &remote {
        Some(r) => {
            let peer = r.peer().await.map(|p| p.to_string()).unwrap_or_default();
            tracing::info!(node = %name, tailnet = %r.tailnet_name(), %peer, "node linked over a tailnet");
            (r.tailnet_name().to_string(), peer)
        }
        None => (String::new(), String::new()),
    };
    db.set_node_tailnet(&node_id, &tailnet, &peer).await?;
    for s in &register.sessions {
        db.store_session(&node_id, s).await?;
    }
    let gone = db
        .sync_pending(&node_id, &register.pending, now_ms())
        .await?;
    send(
        tx,
        hub_message::Body::Registered(Registered {
            node_id: node_id.clone(),
            last_seq,
        }),
    )
    .await?;
    let link = hub
        .live
        .link(
            &node_id,
            tx.clone(),
            register.spawn_roots.clone(),
            register.profiles.clone(),
            register.services.clone(),
        )
        .await;
    tracing::info!(node = %name, %node_id, last_seq, sessions = register.sessions.len(),
        pending = register.pending.len(), "node registered");
    publish_node(hub, &node_id).await?;
    for s in register.sessions {
        publish_session(hub, &node_id, s);
    }
    for request in register.pending {
        publish(
            hub,
            update::Body::Pending(PendingOnNode {
                node_id: node_id.clone(),
                request: Some(request),
            }),
        );
    }
    for pending_id in gone {
        let closed = PendingClosed {
            pending_id,
            reason: ReleaseReason::Unspecified.into(),
        };
        publish(
            hub,
            update::Body::PendingClosed(PendingClosedOnNode {
                node_id: node_id.clone(),
                closed: Some(closed),
            }),
        );
    }

    let outcome = serve_link(hub, &node_id, name, &mut inbound, tx).await;
    db.touch_node(&node_id, now_ms()).await?;
    if hub.live.unlink(&node_id, link).await {
        publish_node(hub, &node_id).await?;
    }
    tracing::info!(node = %name, "node disconnected");
    outcome
}

async fn serve_link(
    hub: &Hub,
    node_id: &str,
    name: &str,
    inbound: &mut Streaming<NodeMessage>,
    tx: &ToNode,
) -> Result<()> {
    let db = &hub.db;
    while let Some(msg) = inbound.next().await {
        match msg?.body {
            Some(Body::Event(e)) => {
                let seq = e.seq;
                if db.store_event(node_id, &e).await? {
                    hub.notifier.event(name, node_id, &e);
                    if e.source() == EventSource::Daemon && e.kind == "rate_limit" {
                        token_usage(hub, node_id, &e).await?;
                    }
                    hub.live.publish_event(node_id, e);
                }
                send(tx, hub_message::Body::Ack(Ack { seq })).await?;
            }
            Some(Body::Session(s)) => {
                if db.store_session(node_id, &s).await? {
                    hub.notifier.ended(name, node_id, &s);
                }
                publish_session(hub, node_id, s);
            }
            Some(Body::Health(h)) => {
                db.store_health(node_id, &h).await?;
                hub.notifier.health(name, node_id, &h).await;
            }
            Some(Body::Pending(p)) => {
                db.store_pending(node_id, &p).await?;
                hub.notifier.pending(name, node_id, &p);
                publish(
                    hub,
                    update::Body::Pending(PendingOnNode {
                        node_id: node_id.to_string(),
                        request: Some(p),
                    }),
                );
            }
            Some(Body::PendingClosed(c)) => {
                db.close_pending(node_id, &c.pending_id, c.reason(), now_ms())
                    .await?;
                publish(
                    hub,
                    update::Body::PendingClosed(PendingClosedOnNode {
                        node_id: node_id.to_string(),
                        closed: Some(c),
                    }),
                );
            }
            Some(Body::CommandResult(r)) => hub.live.finish(r).await,
            Some(Body::Register(_)) => bail!("registered twice on one stream"),
            None => tracing::debug!(node = %name, "empty message"),
        }
    }
    Ok(())
}

fn publish(hub: &Hub, body: update::Body) {
    hub.live.publish(Update { body: Some(body) });
}

fn publish_session(hub: &Hub, node_id: &str, mut session: Session) {
    session.node_id = node_id.to_string();
    publish(hub, update::Body::Session(session));
}

/// A node's change of state, as `ListNodes` shows it.
async fn publish_node(hub: &Hub, node_id: &str) -> Result<()> {
    if let Some(row) = hub.db.node(node_id).await? {
        let node = hub.node_view(row).await;
        publish(hub, update::Body::Node(node));
    }
    Ok(())
}

/// A session's account windows (ADR-0042): recorded for the Claude token the session runs
/// under, and alerted on as that account's, once whichever node reports them.
async fn token_usage(hub: &Hub, node_id: &str, e: &saarthi_proto::event::v1::Event) -> Result<()> {
    let Some(session) = hub.db.session(node_id, &e.session_id).await? else {
        return Ok(());
    };
    if session.claude_token.is_empty() {
        return Ok(());
    }
    let Some(w) = crate::claude_tokens::windows(&e.payload_json, e.time_ms) else {
        return Ok(());
    };
    hub.db
        .record_claude_token_usage(&session.claude_token, &w)
        .await?;
    hub.notifier
        .token_usage(&session.claude_token, node_id, &w)
        .await;
    Ok(())
}

async fn send(
    tx: &mpsc::Sender<Result<HubMessage, Status>>,
    body: hub_message::Body,
) -> Result<()> {
    if tx.send(Ok(HubMessage { body: Some(body) })).await.is_err() {
        bail!("the node went away");
    }
    Ok(())
}

pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
