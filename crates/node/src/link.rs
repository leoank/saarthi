//! The hub link: one outbound `Link` stream, reconnected with backoff. Events stay in a
//! buffer until the hub acks them, so a hub restart or an outage loses nothing; on every
//! (re)connect the node registers with its current sessions and resends what the hub has not
//! stored. Sessions, pending requests and health are not buffered: the registration carries
//! the sessions and the requests still held, and health samples taken while disconnected are
//! dropped. Commands from the hub are acted on here and answered with a `CommandResult`.

use std::collections::VecDeque;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use saarthi_proto::event::v1::Event;
use saarthi_proto::node::v1::node_service_client::NodeServiceClient;
use saarthi_proto::node::v1::{
    Command, CommandResult, NodeMessage, Register, command, hub_message, node_message::Body,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::Request;
use tonic::transport::{Channel, Endpoint};

use crate::pending::Pending;
use crate::prompt;
use crate::sessions::Sessions;
use crate::spawner::Spawner;
use crate::wrapper::{ToWrapper, Wrappers};

const MIN_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

pub struct Config {
    /// The hub's node service, `http://host:port`.
    pub hub: String,
    pub token: String,
    pub name: String,
    /// Unacked events kept at most; the oldest are dropped beyond it.
    pub buffer_cap: usize,
    /// Whether clients may open a shell on this node (`--terminal`, ADR-0054), and which:
    /// None for no terminal, an empty string for the owner's login shell, else that program.
    pub terminal: Option<String>,
    /// The shell holder's socket, which keeps the shells clients open (ADR-0078).
    pub shells: std::path::PathBuf,
}

/// What the hub's commands act on.
pub struct Parts {
    pub sessions: Arc<Sessions>,
    pub pending: Arc<Pending>,
    pub wrappers: Arc<Wrappers>,
    pub spawner: Arc<Spawner>,
    /// Whether the keep-awake hold may be taken (ADR-0031), switched by `SetKeepAwake`.
    pub keep_awake: Arc<AtomicBool>,
    /// The local services clients reach through the hub (ADR-0052).
    pub services: Arc<crate::services::Services>,
    /// Prompts held while their sessions work (phase PHS-20261117-f22kxe item 7).
    pub queue: Arc<crate::queue::Queue>,
}

/// Runs until the outbox closes, which happens only when the node shuts down.
pub async fn run(cfg: Config, mut rx: mpsc::Receiver<NodeMessage>, parts: Parts) {
    // Sessions under the hub's tokens reach the Claude API through it (ADR-0051).
    let api = crate::claude_api::HubApi::new(cfg.hub.clone(), cfg.token.clone());
    parts.spawner.set_hub_api(api.clone());
    let mut buffer = Buffer {
        events: VecDeque::new(),
        cap: cfg.buffer_cap,
        dropped: 0,
    };
    let mut backoff = MIN_BACKOFF;
    loop {
        match stream(&cfg, &mut rx, &parts, &api, &mut buffer, &mut backoff).await {
            Ok(()) => return,
            Err(e) => {
                tracing::warn!(error = %format!("{e:#}"), buffered = buffer.events.len(), "hub link down")
            }
        }
        // Keep draining the outbox while waiting, so hooks never block on a full channel.
        let wait = tokio::time::sleep(backoff);
        tokio::pin!(wait);
        loop {
            tokio::select! {
                _ = &mut wait => break,
                msg = rx.recv() => match msg {
                    Some(msg) => { buffer.keep(msg); }
                    None => return,
                },
            }
        }
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

struct Buffer {
    events: VecDeque<Event>,
    cap: usize,
    dropped: u64,
}

impl Buffer {
    /// Keeps an event until it is acked; returns it as a message to send now. Other messages
    /// pass through unbuffered.
    fn keep(&mut self, msg: NodeMessage) -> NodeMessage {
        if let Some(Body::Event(e)) = &msg.body {
            if self.events.len() == self.cap {
                self.events.pop_front();
                self.dropped += 1;
                if self.dropped.is_power_of_two() {
                    tracing::warn!(
                        dropped = self.dropped,
                        "event buffer full, dropping the oldest"
                    );
                }
            }
            self.events.push_back(e.clone());
        }
        msg
    }

    fn ack(&mut self, seq: u64) {
        while self.events.front().is_some_and(|e| e.seq <= seq) {
            self.events.pop_front();
        }
    }
}

/// Whether this node reaches the hub through a tailnet identity of its own (ADR-0048),
/// set once at start; the identity itself arrives when it has joined.
static VIA_TAILNET: AtomicBool = AtomicBool::new(false);
static TAILNET: std::sync::OnceLock<Arc<saarthi_tailnet::Tailnet>> = std::sync::OnceLock::new();

/// From now on every connection to the hub goes through the node's tailnet identity; until
/// [`tailnet_up`] hands it over, connecting fails and the link retries.
pub fn dial_via_tailnet() {
    VIA_TAILNET.store(true, Ordering::SeqCst);
}

/// The node's tailnet identity has joined.
pub fn tailnet_up(tailnet: Arc<saarthi_tailnet::Tailnet>) {
    let _ = TAILNET.set(tailnet);
}

/// The transport boundary (ADR-0004): plain TCP over the host's network, or the node's own
/// tailnet identity (ADR-0048), where the hub's name may not resolve for a moment after a
/// start ([[EXP-20261112-rq8kgc]]) and the caller's retry covers it.
pub(crate) async fn dial(hub: &str) -> Result<Channel> {
    // A machine that sleeps leaves a dead link behind; HTTP/2 pings notice within half a
    // minute so the stream ends and the outbox's buffer is resent on the next connection,
    // and a connect attempt gives up in seconds (phase 1.5 item 15).
    let endpoint = Endpoint::from_shared(hub.to_string())
        .context("hub address")?
        .connect_timeout(std::time::Duration::from_secs(6))
        .tcp_keepalive(Some(std::time::Duration::from_secs(30)))
        .http2_keep_alive_interval(std::time::Duration::from_secs(15))
        .keep_alive_timeout(std::time::Duration::from_secs(10))
        .keep_alive_while_idle(true);
    if VIA_TAILNET.load(Ordering::SeqCst) {
        let Some(tailnet) = TAILNET.get() else {
            bail!("not on the tailnet yet");
        };
        return endpoint
            .connect_with_connector(tailnet.connector())
            .await
            .with_context(|| format!("connecting to {hub} over tailnet {}", tailnet.name()));
    }
    endpoint
        .connect()
        .await
        .with_context(|| format!("connecting to {hub}"))
}

/// Asks the hub for the Claude token a session started at this machine's terminal runs
/// under (ADR-0046): `name`, or the node's default without one. None is the node's own
/// login. The error is the hub's or the network's, for the person at the terminal.
pub async fn claude_token(
    hub: &str,
    token: &str,
    name: Option<String>,
) -> Result<Option<saarthi_proto::node::v1::ClaudeToken>, String> {
    let channel = dial(hub).await.map_err(|e| format!("{e:#}"))?;
    let mut client = NodeServiceClient::new(channel);
    let mut request = Request::new(saarthi_proto::node::v1::ClaudeTokenRequest { name });
    let bearer = format!("Bearer {token}")
        .parse()
        .map_err(|_| "the node token is not valid in a header".to_string())?;
    request.metadata_mut().insert("authorization", bearer);
    match client.claude_token(request).await {
        Ok(reply) => Ok(reply.into_inner().claude_token),
        Err(status) => Err(status.message().to_string()),
    }
}

/// One connection: register, resend what the hub lacks, then stream live until it breaks.
/// Ok means the outbox closed.
async fn stream(
    cfg: &Config,
    rx: &mut mpsc::Receiver<NodeMessage>,
    parts: &Parts,
    api: &Arc<crate::claude_api::HubApi>,
    buffer: &mut Buffer,
    backoff: &mut Duration,
) -> Result<()> {
    let mut client = NodeServiceClient::new(dial(&cfg.hub).await?);
    let (tx, outbound) = mpsc::channel(256);
    let register = Register {
        name: cfg.name.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        sessions: parts.sessions.snapshot().await,
        pending: parts.pending.snapshot().await,
        spawn_roots: parts.spawner.roots(),
        profiles: parts.spawner.launch_profiles(),
        services: parts.services.names(),
        terminal: cfg.terminal.is_some(),
    };
    tx.send(NodeMessage {
        body: Some(Body::Register(register)),
    })
    .await?;
    let mut request = Request::new(ReceiverStream::new(outbound));
    let bearer = format!("Bearer {}", cfg.token)
        .parse()
        .context("token is not valid in a header")?;
    request.metadata_mut().insert("authorization", bearer);
    let mut inbound = client
        .link(request)
        .await
        .context("opening the link")?
        .into_inner();

    let Some(hub_message::Body::Registered(registered)) =
        inbound.message().await?.and_then(|m| m.body)
    else {
        bail!("the hub did not answer the registration");
    };
    buffer.ack(registered.last_seq);
    tracing::info!(node_id = %registered.node_id, resending = buffer.events.len(), "registered with the hub");
    *backoff = MIN_BACKOFF;

    // What is to go to the hub, in order: the buffer's events first, then whatever the
    // outbox brings. It is sent while the hub's acks are read: a node that resent a large
    // buffer before reading any let the acks pile up until the link broke, every time.
    let mut queue: VecDeque<NodeMessage> = buffer
        .events
        .iter()
        .map(|e| NodeMessage {
            body: Some(Body::Event(e.clone())),
        })
        .collect();
    loop {
        tokio::select! {
            permit = tx.reserve(), if !queue.is_empty() => {
                let permit = permit.map_err(|_| anyhow!("the link closed"))?;
                permit.send(queue.pop_front().expect("not empty"));
            }
            msg = rx.recv() => {
                let Some(msg) = msg else { return Ok(()) };
                queue.push_back(buffer.keep(msg));
            }
            reply = inbound.message() => match reply?.and_then(|m| m.body) {
                Some(hub_message::Body::Ack(ack)) => buffer.ack(ack.seq),
                // A usage probe takes a model turn: it runs on its own task and answers
                // through the outbox, so the link keeps flowing meanwhile.
                Some(hub_message::Body::Command(cmd)) if matches!(cmd.action, Some(command::Action::Probe(_))) => {
                    let (spawner, sessions) = (parts.spawner.clone(), parts.sessions.clone());
                    tokio::spawn(async move {
                        let Some(command::Action::Probe(p)) = &cmd.action else { return };
                        let outcome = spawner.probe(p.claude_token.as_ref()).await;
                        if let Err(e) = &outcome {
                            tracing::info!(id = %cmd.id, error = %e, "usage probe failed");
                        }
                        let (ok, probe_json, error) = match outcome {
                            Ok(json) => (true, json, String::new()),
                            Err(e) => (false, String::new(), e),
                        };
                        sessions
                            .command_result(CommandResult {
                                command_id: cmd.id.clone(),
                                ok,
                                error,
                                note: String::new(),
                                probe_json,
                                gone: false,
                                queued: false,
                                session_id: String::new(),
                                shells: Vec::new(),
                            })
                            .await;
                    });
                }
                Some(hub_message::Body::Command(cmd)) => {
                    let holder = cfg.terminal.is_some().then_some(cfg.shells.as_path());
                    let result = act(&cmd, parts, holder).await;
                    queue.push_back(NodeMessage { body: Some(Body::CommandResult(result)) });
                }
                // A client's request to a local service: answered over a stream of its own,
                // so the link keeps flowing (ADR-0052).
                Some(hub_message::Body::OpenService(open)) => {
                    tokio::spawn(crate::services::open(parts.services.clone(), api.clone(), open));
                }
                // A client's shell, on a stream of its own (ADR-0054).
                Some(hub_message::Body::OpenTerminal(open)) => {
                    tokio::spawn(crate::terminal::open(api.clone(), open, cfg.terminal.clone(), cfg.shells.clone()));
                }
                Some(hub_message::Body::Registered(_)) => bail!("registered twice on one stream"),
                None => bail!("the hub closed the link"),
            },
        }
    }
}

/// Carries out one command from the hub. Ok carries a note for the person, if any. `holder`
/// is the shell holder's socket when the node offers a terminal (ADR-0078).
async fn act(cmd: &Command, parts: &Parts, holder: Option<&std::path::Path>) -> CommandResult {
    let mut gone = false;
    let mut shells = Vec::new();
    let mut queued = false;
    let mut forked = String::new();
    let Parts {
        sessions,
        pending,
        wrappers,
        spawner,
        keep_awake,
        queue,
        ..
    } = parts;
    let id = &cmd.session_id;
    let outcome = match &cmd.action {
        Some(command::Action::Resolve(r)) => pending.resolve(r).await.map(|()| String::new()),
        Some(command::Action::Prompt(p)) => match with_photos(sessions, spawner, id, p).await {
            Err(e) => Err(e),
            Ok(text) if p.queue && sessions.busy(id).await => {
                queue.push(id, &text).await;
                queued = true;
                Ok(String::new())
            }
            Ok(text) => deliver(sessions, spawner, wrappers, id, &text).await,
        },
        Some(command::Action::SetModel(m)) if !valid_model(&m.model) || !valid_effort(&m.effort) => {
            Err(format!("not a model or effort Claude Code takes: {:?} {:?}", m.model, m.effort))
        }
        Some(command::Action::SetModel(m)) if sessions.is_spawned(id).await => {
            spawner.set_model(id, &m.model, &m.effort).await.map(|()| String::new())
        }
        // Typed at the terminal, as the person would (ADR-0027).
        Some(command::Action::SetModel(m)) if sessions.is_wrapped(id).await => {
            let mut typed = Ok(String::new());
            for (command, value) in [("/model", &m.model), ("/effort", &m.effort)] {
                if !value.is_empty() && typed.is_ok() {
                    let text = ToWrapper::Prompt { text: format!("{command} {value}") };
                    typed = wrappers.command(id, text).await.map(|()| String::new());
                }
            }
            typed
        }
        Some(command::Action::SetModel(_)) => {
            Err("the session was started outside saarthi run, so its model can only be changed at its terminal".into())
        }
        Some(command::Action::Unqueue(u)) => match queue.take(id, &u.queued_id).await {
            Some(text) if u.send => deliver(sessions, spawner, wrappers, id, &text).await,
            Some(_) => Ok(String::new()),
            None => Ok("it was no longer queued".into()),
        },
        Some(command::Action::Interrupt(i)) if sessions.is_spawned(id).await => spawner
            .interrupt(id, i.then_prompt.as_deref())
            .await
            .map(|()| String::new()),
        Some(command::Action::Interrupt(i)) => {
            let interrupt = ToWrapper::Interrupt {
                then_prompt: i.then_prompt.clone(),
            };
            wrappers
                .command(&cmd.session_id, interrupt)
                .await
                .map(|()| String::new())
        }
        Some(command::Action::Kill(_)) if sessions.is_spawned(id).await => {
            spawner.kill(id).await.map(|()| String::new())
        }
        Some(command::Action::Kill(_)) if sessions.is_wrapped(id).await => wrappers
            .command(id, ToWrapper::Kill)
            .await
            .map(|()| String::new()),
        // Nothing of it runs here: the node restarted since, or it was never this node's.
        Some(command::Action::Kill(_)) if sessions.pid(id).await.is_none() => {
            gone = true;
            Ok("it was not running on this node any more; it is marked ended".into())
        }
        Some(command::Action::Kill(_)) => terminate(sessions, id).await,
        Some(command::Action::Spawn(s)) => spawner
            .spawn(
                id,
                &s.cwd,
                &s.prompt,
                &s.model,
                s.create,
                &s.permission_mode,
                &s.profile,
                s.claude_token.as_ref(),
            )
            .await
            .map(|()| String::new()),
        Some(command::Action::Resume(r)) => spawner
            .resume(id, &r.cwd, &r.prompt, &r.model, r.claude_token.as_ref(), &r.profile, &r.last_line_id)
            .await
            .map(|fork| match fork {
                Some(fork) => {
                    let note = "this machine's copy of the session and the hub's went different ways, so the hub's \
                        resumed as a new session; this machine's copy is untouched"
                        .to_string();
                    forked = fork;
                    note
                }
                None => String::new(),
            }),
        Some(command::Action::SetKeepAwake(k)) => {
            keep_awake.store(k.on, Ordering::Relaxed);
            tracing::info!(on = k.on, "keep-awake switched from a client");
            Ok(String::new())
        }
        Some(command::Action::Probe(_)) => Err("a probe is run apart".to_string()),
        Some(command::Action::ListShells(_)) | Some(command::Action::KillShell(_)) if holder.is_none() => {
            Err("this node offers no terminal: it runs without --terminal".to_string())
        }
        // No holder answering holds no shells.
        Some(command::Action::ListShells(_)) => match crate::shells::connect(holder.unwrap()).await {
            Ok(mut c) => match c.list(saarthi_proto::shells::v1::ListRequest {}).await {
                Ok(r) => {
                    shells = r.into_inner().shells;
                    Ok(String::new())
                }
                Err(e) => Err(format!("the shell holder: {}", e.message())),
            },
            Err(_) => Ok(String::new()),
        },
        Some(command::Action::KillShell(k)) => match crate::shells::connect(holder.unwrap()).await {
            Ok(mut c) => match c.kill(saarthi_proto::shells::v1::KillRequest { id: k.shell_id.clone() }).await {
                Ok(r) if r.get_ref().killed => Ok(String::new()),
                Ok(_) => Err("no such shell: it ended or was killed".to_string()),
                Err(e) => Err(format!("the shell holder: {}", e.message())),
            },
            Err(_) => Err("no such shell: the shell holder is not running".to_string()),
        },
        None => Err("the command has no action".to_string()),
    };
    if let Err(e) = &outcome {
        tracing::info!(id = %cmd.id, session = %cmd.session_id, error = %e, "command refused");
    }
    let (ok, note, error) = match outcome {
        Ok(note) => (true, note, String::new()),
        Err(error) => (false, String::new(), error),
    };
    CommandResult {
        command_id: cmd.id.clone(),
        ok,
        error,
        note,
        probe_json: String::new(),
        gone,
        queued,
        session_id: forked,
        shells,
    }
}

/// Kills a bare session: SIGTERM to the pid the registry gave.
async fn terminate(sessions: &Sessions, id: &str) -> Result<String, String> {
    let Some(pid) = sessions.pid(id).await else {
        return Err("no running process of this session is known".into());
    };
    if sessions.session_for_pid(pid).await.as_deref() != Some(id) {
        return Err("the session has ended".into());
    }
    let pid = libc::pid_t::try_from(pid).map_err(|_| "bad pid".to_string())?;
    if unsafe { libc::kill(pid, libc::SIGTERM) } != 0 {
        return Err(format!("SIGTERM to {pid}: {}", std::io::Error::last_os_error()));
    }
    Ok(format!("sent SIGTERM to pid {pid}; the session ends when Claude Code exits"))
}

/// A model as `/model` and `set_model` take it: an alias or an id; nothing to type into a
/// terminal beyond that.
/// The prompt's text, with its photos saved where the session can read them and their paths
/// added (ADR-0071).
async fn with_photos(sessions: &Sessions, spawner: &Spawner, id: &str, p: &saarthi_proto::node::v1::Prompt) -> Result<String, String> {
    if p.attachments.is_empty() {
        return Ok(p.text.clone());
    }
    if id.is_empty() || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err("not a session id".into());
    }
    let home = std::env::var_os("HOME").map(std::path::PathBuf::from).ok_or("the node has no HOME to save photos under")?;
    let profile = sessions.profile(id).await;
    let state = spawner.profiles().get(&profile).ok().flatten().and_then(|p| p.state_dir.clone());
    let dir = crate::uploads::dir(&home, state.as_deref(), id);
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0);
    let (text, photos) = (p.text.clone(), p.attachments.clone());
    tokio::task::spawn_blocking(move || crate::uploads::save(&dir, &text, &photos, now))
        .await
        .map_err(|e| e.to_string())?
}

fn valid_model(m: &str) -> bool {
    m.len() <= 64 && m.chars().all(|c| c.is_ascii_alphanumeric() || "-_.[]".contains(c))
}

fn valid_effort(e: &str) -> bool {
    matches!(e, "" | "low" | "medium" | "high" | "xhigh" | "max")
}

/// Starts delivering prompts queued while their sessions work, as each is next idle.
pub fn run_queue(parts: &Parts) -> tokio::task::JoinHandle<()> {
    let (sessions, spawner, wrappers) = (parts.sessions.clone(), parts.spawner.clone(), parts.wrappers.clone());
    tokio::spawn(crate::queue::run(parts.queue.clone(), move |id: String, text: String| {
        let (sessions, spawner, wrappers) = (sessions.clone(), spawner.clone(), wrappers.clone());
        async move { deliver(&sessions, &spawner, &wrappers, &id, &text).await }
    }))
}

/// Hands a prompt to a session now, the way its kind takes one; Ok carries a note.
pub async fn deliver(
    sessions: &Sessions,
    spawner: &Spawner,
    wrappers: &Wrappers,
    id: &str,
    text: &str,
) -> Result<String, String> {
    if sessions.is_spawned(id).await {
        spawner.prompt(id, text).await.map(|()| String::new())
    } else if sessions.is_wrapped(id).await {
        // Typed at the terminal, so Claude sees the person's own prompt (ADR-0027).
        let typed = ToWrapper::Prompt { text: text.to_string() };
        wrappers.command(id, typed).await.map(|()| String::new())
    } else {
        prompt(sessions, id, text).await
    }
}

async fn prompt(sessions: &Sessions, session_id: &str, text: &str) -> Result<String, String> {
    let target = sessions.prompt_target(session_id).await?;
    match prompt::deliver(sessions.claude_dir(), session_id, &target, text).await? {
        prompt::Delivery::Queued => Ok("sent over the messaging socket: the session was not started \
            by saarthi run, so Claude sees it as a message from another session"
            .to_string()),
        prompt::Delivery::HeldForApproval => Ok("the session bypasses permissions, so Claude \
            Code holds the prompt until it is approved at the terminal"
            .to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(seq: u64) -> NodeMessage {
        NodeMessage {
            body: Some(Body::Event(Event {
                seq,
                ..Default::default()
            })),
        }
    }

    #[test]
    fn buffer_drops_acked_and_oldest() {
        let mut b = Buffer {
            events: VecDeque::new(),
            cap: 3,
            dropped: 0,
        };
        for seq in 1..=4 {
            b.keep(event(seq));
        }
        assert_eq!(
            b.events.iter().map(|e| e.seq).collect::<Vec<_>>(),
            [2, 3, 4]
        );
        assert_eq!(b.dropped, 1);
        b.ack(3);
        assert_eq!(b.events.iter().map(|e| e.seq).collect::<Vec<_>>(), [4]);
    }
}
