//! What the hub knows only while it runs: which nodes are linked now, the commands waiting
//! for a node's answer, and the stream of changes clients watch.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use saarthi_proto::control::v1::{ServiceResponsePart, Update};
use saarthi_proto::event::v1::{Event, LaunchProfile};
use saarthi_proto::node::v1::{
    Command, CommandResult, HubMessage, OpenService, OpenTerminal, ServiceDown, ServiceRequestHead, TerminalIn,
    TerminalOut, command, hub_message,
};
use tokio::sync::{Mutex, broadcast, mpsc, oneshot};
use tonic::Status;

/// How long a client waits for a node to act on a command.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(15);

/// How long a service request waits for the node to dial its stream back (ADR-0052).
const SERVICE_DIAL_TIMEOUT: Duration = Duration::from_secs(20);

pub type ToNode = mpsc::Sender<Result<HubMessage, Status>>;

/// What a client's service request is answered through.
pub type ToClient = mpsc::Sender<Result<ServiceResponsePart, Status>>;

/// The request body's pieces, for the node's stream.
pub type BodyDown = mpsc::Receiver<Result<ServiceDown, Status>>;

/// What a client's terminal is answered through: the shell's output, then why it ended.
pub type ToTerminalClient = mpsc::Sender<Result<TerminalOut, Status>>;

/// What the client types and resizes, for the node's stream.
pub type TerminalInput = mpsc::Receiver<Result<TerminalIn, Status>>;

/// A terminal waiting for the node to dial its stream back (ADR-0054).
pub struct PendingTerminal {
    pub node_id: String,
    pub input: TerminalInput,
    pub to_client: ToTerminalClient,
}

/// A service request waiting for the node to dial its stream back.
pub struct PendingService {
    pub node_id: String,
    pub body: BodyDown,
    pub to_client: ToClient,
}

pub struct Live {
    nodes: Mutex<HashMap<String, NodeLink>>,
    waiting: Mutex<HashMap<String, oneshot::Sender<CommandResult>>>,
    services: Mutex<HashMap<String, PendingService>>,
    terminals: Mutex<HashMap<String, PendingTerminal>>,
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
    /// The local services it offers through the hub (ADR-0052).
    services: Vec<String>,
    /// Whether it offers a shell (ADR-0054).
    terminal: bool,
}

impl Live {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            nodes: Mutex::new(HashMap::new()),
            waiting: Mutex::new(HashMap::new()),
            services: Mutex::new(HashMap::new()),
            terminals: Mutex::new(HashMap::new()),
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
        services: Vec<String>,
        terminal: bool,
    ) -> u64 {
        let link = self.links.fetch_add(1, Ordering::Relaxed);
        self.nodes.lock().await.insert(
            node_id.to_string(),
            NodeLink {
                link,
                tx,
                spawn_roots,
                profiles,
                services,
                terminal,
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

    /// A linked node's services; None when the node is not linked.
    pub async fn services(&self, node_id: &str) -> Option<Vec<String>> {
        self.nodes.lock().await.get(node_id).map(|n| n.services.clone())
    }

    /// Asks a node to open one of its services for a client's request (ADR-0052, ADR-0057):
    /// the request waits under a fresh id until the node dials its stream back, and is
    /// answered with an error if it does not in time.
    pub async fn open_service(
        self: &Arc<Self>,
        node_id: &str,
        head: ServiceRequestHead,
        body: BodyDown,
        to_client: ToClient,
    ) -> Result<(), Status> {
        let tx = match self.nodes.lock().await.get(node_id) {
            Some(n) if n.services.contains(&head.service) => n.tx.clone(),
            Some(_) => {
                return Err(Status::not_found(format!(
                    "the node offers no service named {}",
                    head.service
                )));
            }
            None => return Err(Status::unavailable("the node is not connected")),
        };
        let id = uuid::Uuid::new_v4().simple().to_string();
        self.services.lock().await.insert(
            id.clone(),
            PendingService {
                node_id: node_id.to_string(),
                body,
                to_client,
            },
        );
        let open = OpenService {
            id: id.clone(),
            head: Some(head),
        };
        if tx
            .send(Ok(HubMessage {
                body: Some(hub_message::Body::OpenService(open)),
            }))
            .await
            .is_err()
        {
            self.services.lock().await.remove(&id);
            return Err(Status::unavailable("the node went away"));
        }
        let live = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(SERVICE_DIAL_TIMEOUT).await;
            if let Some(p) = live.services.lock().await.remove(&id) {
                let _ = p
                    .to_client
                    .send(Err(Status::deadline_exceeded("the node did not open the service in time")))
                    .await;
            }
        });
        Ok(())
    }

    /// Whether a linked node offers a shell.
    pub async fn terminal(&self, node_id: &str) -> bool {
        self.nodes.lock().await.get(node_id).is_some_and(|n| n.terminal)
    }

    /// Asks a node to open a shell for a client (ADR-0054), as `open_service` does for a
    /// request: it waits under a fresh id until the node dials its stream back.
    pub async fn open_terminal(
        self: &Arc<Self>,
        node_id: &str,
        cols: u32,
        rows: u32,
        input: TerminalInput,
        to_client: ToTerminalClient,
    ) -> Result<(), Status> {
        let tx = match self.nodes.lock().await.get(node_id) {
            Some(n) if n.terminal => n.tx.clone(),
            Some(_) => return Err(Status::failed_precondition("the node offers no terminal: it runs without --terminal")),
            None => return Err(Status::unavailable("the node is not connected")),
        };
        let id = uuid::Uuid::new_v4().simple().to_string();
        self.terminals.lock().await.insert(
            id.clone(),
            PendingTerminal { node_id: node_id.to_string(), input, to_client },
        );
        let open = OpenTerminal { id: id.clone(), cols, rows, ..Default::default() };
        if tx
            .send(Ok(HubMessage { body: Some(hub_message::Body::OpenTerminal(open)) }))
            .await
            .is_err()
        {
            self.terminals.lock().await.remove(&id);
            return Err(Status::unavailable("the node went away"));
        }
        let live = self.clone();
        tokio::spawn(async move {
            tokio::time::sleep(SERVICE_DIAL_TIMEOUT).await;
            if let Some(p) = live.terminals.lock().await.remove(&id) {
                let _ = p
                    .to_client
                    .send(Err(Status::deadline_exceeded("the node did not open the terminal in time")))
                    .await;
            }
        });
        Ok(())
    }

    /// The terminal a node's `TerminalStream` names, if it is waiting and is that node's.
    pub async fn take_terminal(&self, id: &str, node_id: &str) -> Option<PendingTerminal> {
        let mut terminals = self.terminals.lock().await;
        if terminals.get(id).is_some_and(|p| p.node_id == node_id) {
            return terminals.remove(id);
        }
        None
    }

    /// The request a node's `ServiceStream` names, if it is waiting and is that node's.
    pub async fn take_service(&self, id: &str, node_id: &str) -> Option<PendingService> {
        let mut services = self.services.lock().await;
        if services.get(id).is_some_and(|p| p.node_id == node_id) {
            return services.remove(id);
        }
        None
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
