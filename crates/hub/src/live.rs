//! What the hub knows only while it runs: which nodes are linked now, the commands waiting
//! for a node's answer, and the stream of changes clients watch.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use saarthi_proto::control::v1::Update;
use saarthi_proto::event::v1::{Event, LaunchProfile};
use saarthi_proto::node::v1::{Command, CommandResult, HubMessage, command, hub_message};
use tokio::sync::{Mutex, broadcast, mpsc, oneshot};
use tonic::Status;

/// How long a client waits for a node to act on a command.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

pub type ToNode = mpsc::Sender<Result<HubMessage, Status>>;

pub struct Live {
    nodes: Mutex<HashMap<String, NodeLink>>,
    waiting: Mutex<HashMap<String, oneshot::Sender<CommandResult>>>,
    links: AtomicU64,
    updates: broadcast::Sender<Update>,
    /// Newly stored events with their node id, for session streams.
    events: broadcast::Sender<(String, Event)>,
}

struct NodeLink {
    link: u64,
    tx: ToNode,
    /// Where the node allows spawning, and the launch profiles it offers, from its
    /// registration.
    spawn_roots: Vec<String>,
    profiles: Vec<LaunchProfile>,
}

impl Live {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            nodes: Mutex::new(HashMap::new()),
            waiting: Mutex::new(HashMap::new()),
            links: AtomicU64::new(0),
            updates: broadcast::channel(1024).0,
            events: broadcast::channel(4096).0,
        })
    }

    /// Marks a node linked through `tx`, replacing an older link of the same node. The
    /// returned number identifies this link for `unlink`.
    pub async fn link(
        &self,
        node_id: &str,
        tx: ToNode,
        spawn_roots: Vec<String>,
        profiles: Vec<LaunchProfile>,
    ) -> u64 {
        let link = self.links.fetch_add(1, Ordering::Relaxed);
        self.nodes.lock().await.insert(
            node_id.to_string(),
            NodeLink {
                link,
                tx,
                spawn_roots,
                profiles,
            },
        );
        link
    }

    /// Marks a node unlinked unless a newer link has taken its place.
    pub async fn unlink(&self, node_id: &str, link: u64) -> bool {
        let mut nodes = self.nodes.lock().await;
        if nodes.get(node_id).is_some_and(|n| n.link == link) {
            nodes.remove(node_id);
            return true;
        }
        false
    }

    /// A linked node's spawn roots and launch profiles; None when the node is not linked.
    pub async fn spawning(&self, node_id: &str) -> Option<(Vec<String>, Vec<LaunchProfile>)> {
        let nodes = self.nodes.lock().await;
        nodes
            .get(node_id)
            .map(|n| (n.spawn_roots.clone(), n.profiles.clone()))
    }

    /// Sends a command to a node and waits for its result.
    pub async fn command(
        &self,
        node_id: &str,
        session_id: &str,
        action: command::Action,
    ) -> Result<CommandResult, Status> {
        self.command_within(node_id, session_id, action, COMMAND_TIMEOUT)
            .await
    }

    /// The same, waiting up to `timeout`: a usage probe takes a model turn.
    pub async fn command_within(
        &self,
        node_id: &str,
        session_id: &str,
        action: command::Action,
        timeout: Duration,
    ) -> Result<CommandResult, Status> {
        let tx = match self.nodes.lock().await.get(node_id) {
            Some(n) => n.tx.clone(),
            None => return Err(Status::unavailable("the node is not connected")),
        };
        let id = uuid::Uuid::new_v4().simple().to_string();
        let (done, result) = oneshot::channel();
        self.waiting.lock().await.insert(id.clone(), done);
        let cmd = Command {
            id: id.clone(),
            session_id: session_id.to_string(),
            action: Some(action),
        };
        let sent = tx
            .send(Ok(HubMessage {
                body: Some(hub_message::Body::Command(cmd)),
            }))
            .await;
        let outcome = match sent {
            Err(_) => Err(Status::unavailable("the node went away")),
            Ok(()) => match tokio::time::timeout(timeout, result).await {
                Ok(Ok(result)) => Ok(result),
                Ok(Err(_)) => Err(Status::unavailable("the node went away")),
                Err(_) => Err(Status::deadline_exceeded("the node did not answer in time")),
            },
        };
        self.waiting.lock().await.remove(&id);
        outcome
    }

    /// Hands a node's answer to the client waiting for it.
    pub async fn finish(&self, result: CommandResult) {
        if let Some(done) = self.waiting.lock().await.remove(&result.command_id) {
            let _ = done.send(result);
        }
    }

    pub fn publish(&self, update: Update) {
        // No subscribers is fine.
        let _ = self.updates.send(update);
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Update> {
        self.updates.subscribe()
    }

    pub fn publish_event(&self, node_id: &str, event: Event) {
        let _ = self.events.send((node_id.to_string(), event));
    }

    pub fn subscribe_events(&self) -> broadcast::Receiver<(String, Event)> {
        self.events.subscribe()
    }
}
