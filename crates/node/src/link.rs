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
}

/// What the hub's commands act on.
pub struct Parts {
    pub sessions: Arc<Sessions>,
    pub pending: Arc<Pending>,
    pub wrappers: Arc<Wrappers>,
    pub spawner: Arc<Spawner>,
    /// Whether the keep-awake hold may be taken (ADR-0031), switched by `SetKeepAwake`.
    pub keep_awake: Arc<AtomicBool>,
}

/// Runs until the outbox closes, which happens only when the node shuts down.
pub async fn run(cfg: Config, mut rx: mpsc::Receiver<NodeMessage>, parts: Parts) {
    let mut buffer = Buffer {
        events: VecDeque::new(),
        cap: cfg.buffer_cap,
        dropped: 0,
    };
    let mut backoff = MIN_BACKOFF;
    loop {
        match stream(&cfg, &mut rx, &parts, &mut buffer, &mut backoff).await {
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

/// The transport boundary: today a plain TCP connection; the embedded tsnet dialer
/// replaces this function in phase 3.
async fn dial(hub: &str) -> Result<Channel> {
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
    for e in &buffer.events {
        tx.send(NodeMessage {
            body: Some(Body::Event(e.clone())),
        })
        .await?;
    }
    *backoff = MIN_BACKOFF;

    loop {
        tokio::select! {
            msg = rx.recv() => {
                let Some(msg) = msg else { return Ok(()) };
                tx.send(buffer.keep(msg)).await.map_err(|_| anyhow!("the link closed"))?;
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
                            })
                            .await;
                    });
                }
                Some(hub_message::Body::Command(cmd)) => {
                    let result = act(&cmd, parts).await;
                    tx.send(NodeMessage { body: Some(Body::CommandResult(result)) })
                        .await
                        .map_err(|_| anyhow!("the link closed"))?;
                }
                Some(hub_message::Body::Registered(_)) => bail!("registered twice on one stream"),
                None => bail!("the hub closed the link"),
            },
        }
    }
}

/// Carries out one command from the hub. Ok carries a note for the person, if any.
async fn act(cmd: &Command, parts: &Parts) -> CommandResult {
    let Parts {
        sessions,
        pending,
        wrappers,
        spawner,
        keep_awake,
    } = parts;
    let id = &cmd.session_id;
    let outcome = match &cmd.action {
        Some(command::Action::Resolve(r)) => pending.resolve(r).await.map(|()| String::new()),
        Some(command::Action::Prompt(p)) if sessions.is_spawned(id).await => {
            spawner.prompt(id, &p.text).await.map(|()| String::new())
        }
        Some(command::Action::Prompt(p)) => {
            if sessions.is_wrapped(&cmd.session_id).await {
                // Typed at the terminal, so Claude sees the person's own prompt (ADR-0027).
                let typed = ToWrapper::Prompt {
                    text: p.text.clone(),
                };
                wrappers
                    .command(&cmd.session_id, typed)
                    .await
                    .map(|()| String::new())
            } else {
                prompt(sessions, &cmd.session_id, &p.text).await
            }
        }
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
            .resume(id, &r.cwd, &r.prompt, &r.model, r.claude_token.as_ref())
            .await
            .map(|()| String::new()),
        Some(command::Action::SetKeepAwake(k)) => {
            keep_awake.store(k.on, Ordering::Relaxed);
            tracing::info!(on = k.on, "keep-awake switched from a client");
            Ok(String::new())
        }
        Some(command::Action::Probe(_)) => Err("a probe is run apart".to_string()),
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
