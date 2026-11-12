//! `saarthi tui`: a dashboard of the hub in the terminal (ADR-0047). It shows every node with
//! its live sessions, the permission requests and questions waiting for a person, and the
//! Claude accounts' usage, kept current from the control service's `Watch` stream; a key
//! allows, denies or answers a request, prompts, interrupts or ends a session, or starts a
//! new one. Conversations stay in Claude Code and the app.

mod state;
mod view;

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use ratatui::crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use saarthi_proto::control::v1::*;
use saarthi_proto::event::v1::{AnswerValue, Answers, Deny, Resolution, resolution};
use tokio::sync::mpsc;

use crate::ctl::Client;
use state::{Action, Key, State};

/// How long one request to the hub may take before the dashboard gives up on it.
const CALL: Duration = Duration::from_secs(30);

/// What the watch task reports.
enum Watched {
    /// The stream is open: reload everything, then follow updates.
    Up,
    Update(Box<update::Body>),
    /// The stream ended; it is retried.
    Down(String),
}

pub async fn run(hub: &str, token: &str) -> Result<()> {
    let mut c = Client::connect(hub, token).await?;
    let mut state = State::new(hub);

    let (key_tx, mut keys) = mpsc::channel::<Option<Key>>(64);
    std::thread::spawn(move || {
        loop {
            let key = match event::read() {
                Ok(Event::Key(k)) if k.kind != KeyEventKind::Release => key(k.code, k.modifiers),
                Ok(Event::Resize(..)) => None,
                Ok(_) => continue,
                Err(_) => return,
            };
            if key_tx.blocking_send(key).is_err() {
                return;
            }
        }
    });

    let (watch_tx, mut watched) = mpsc::channel::<Watched>(256);
    tokio::spawn(watch(Client::connect(hub, token).await?, watch_tx));

    let mut terminal = ratatui::init();
    let outcome = async {
        let mut tick = tokio::time::interval(Duration::from_secs(1));
        let mut accounts = tokio::time::interval(Duration::from_secs(60));
        loop {
            terminal.draw(|f| view::draw(f, &state, saarthi_node::outbox::now_ms()))?;
            tokio::select! {
                k = keys.recv() => match k {
                    Some(Some(k)) => match state.key(k) {
                        Some(Action::Quit) => return anyhow::Ok(()),
                        Some(Action::Reload) => reload(&mut c, &mut state).await,
                        Some(a) => perform(&mut c, &mut state, a).await,
                        None => {}
                    },
                    Some(None) => {}
                    None => return Ok(()),
                },
                w = watched.recv() => match w {
                    Some(Watched::Up) => reload(&mut c, &mut state).await,
                    Some(Watched::Update(body)) => state.apply(*body),
                    Some(Watched::Down(why)) => {
                        state.linked = false;
                        state.say(format!("lost the hub: {why}"), true);
                    }
                    None => return Ok(()),
                },
                _ = accounts.tick() => {
                    if let Ok(t) = tokens(&mut c).await {
                        state.tokens = t;
                    }
                }
                _ = tick.tick() => {}
            }
        }
    }
    .await;
    ratatui::restore();
    outcome
}

fn key(code: KeyCode, mods: KeyModifiers) -> Option<Key> {
    Some(match code {
        KeyCode::Char(c) if mods.contains(KeyModifiers::CONTROL) => Key::Ctrl(c),
        KeyCode::Char(c) => Key::Char(c),
        KeyCode::Enter => Key::Enter,
        KeyCode::Esc => Key::Esc,
        KeyCode::Tab => Key::Tab,
        KeyCode::BackTab => Key::BackTab,
        KeyCode::Backspace => Key::Backspace,
        KeyCode::Up => Key::Up,
        KeyCode::Down => Key::Down,
        KeyCode::Left => Key::Left,
        KeyCode::Right => Key::Right,
        _ => return None,
    })
}

/// Follows `Watch`, reconnecting every few seconds after it ends.
async fn watch(mut c: Client, tx: mpsc::Sender<Watched>) {
    loop {
        let why = match c.inner.watch(c.request(WatchRequest {})).await {
            Ok(stream) => {
                if tx.send(Watched::Up).await.is_err() {
                    return;
                }
                let mut stream = stream.into_inner();
                loop {
                    match stream.message().await {
                        Ok(Some(u)) => {
                            if let Some(body) = u.body
                                && tx.send(Watched::Update(Box::new(body))).await.is_err()
                            {
                                return;
                            }
                        }
                        Ok(None) => break "the hub closed the stream".to_string(),
                        Err(e) => break e.message().to_string(),
                    }
                }
            }
            Err(e) => e.message().to_string(),
        };
        if tx.send(Watched::Down(why)).await.is_err() {
            return;
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

async fn tokens(c: &mut Client) -> Result<Vec<ClaudeToken>> {
    Ok(c.inner
        .list_claude_tokens(c.request(ListClaudeTokensRequest {}))
        .await?
        .into_inner()
        .tokens)
}

/// Everything at once: after connecting, after a reconnect, and on `r`.
async fn reload(c: &mut Client, state: &mut State) {
    let all = async {
        let nodes = c.inner.list_nodes(c.request(ListNodesRequest {})).await?.into_inner().nodes;
        let sessions = c
            .inner
            .list_sessions(c.request(ListSessionsRequest {
                node_id: String::new(),
                include_ended: false,
            }))
            .await?
            .into_inner()
            .sessions;
        let pending = c
            .inner
            .list_pending(c.request(ListPendingRequest {}))
            .await?
            .into_inner()
            .pending
            .into_iter()
            .filter_map(|p| Some((p.node_id, p.request?)))
            .collect();
        anyhow::Ok((nodes, sessions, pending))
    };
    match tokio::time::timeout(CALL, all).await {
        Ok(Ok((nodes, sessions, pending))) => {
            // A hub without Claude tokens answers with none; an older one may not answer.
            let accounts = tokens(c).await.unwrap_or_default();
            state.load(nodes, sessions, pending, accounts);
        }
        Ok(Err(e)) => state.say(format!("{e:#}"), true),
        Err(_) => state.say("the hub did not answer", true),
    }
}

async fn perform(c: &mut Client, state: &mut State, action: Action) {
    let outcome = tokio::time::timeout(CALL, call(c, action)).await;
    match outcome {
        Ok(Ok(done)) => state.say(done, false),
        Ok(Err(e)) => state.say(e, true),
        Err(_) => state.say("the hub did not answer in time", true),
    }
}

/// Carries out one action; Ok is what to tell the person, Err why it failed.
async fn call(c: &mut Client, action: Action) -> Result<String, String> {
    let report = |r: Result<tonic::Response<CommandResponse>, tonic::Status>, done: &str| match r {
        Ok(r) => {
            let r = r.into_inner();
            if r.ok {
                Ok(if r.note.is_empty() { done.to_string() } else { format!("{done}: {}", r.note) })
            } else {
                Err(r.error)
            }
        }
        Err(s) => Err(s.message().to_string()),
    };
    let resolve = |node: String, pending: String, decision: resolution::Decision| ResolveRequest {
        node_id: node,
        resolution: Some(Resolution {
            pending_id: pending,
            decision: Some(decision),
        }),
    };
    match action {
        Action::Quit | Action::Reload => Ok(String::new()),
        Action::Allow { node, pending } => {
            let req = resolve(node, pending, resolution::Decision::Allow(Default::default()));
            report(c.inner.resolve(c.request(req)).await, "allowed")
        }
        Action::Deny { node, pending, message } => {
            let req = resolve(node, pending, resolution::Decision::Deny(Deny { message }));
            report(c.inner.resolve(c.request(req)).await, "denied")
        }
        Action::Answer { node, pending, answers } => {
            let map: HashMap<String, AnswerValue> = answers
                .into_iter()
                .map(|(q, values)| (q, AnswerValue { values }))
                .collect();
            let req = resolve(node, pending, resolution::Decision::Answers(Answers { answers: map }));
            report(c.inner.resolve(c.request(req)).await, "answered")
        }
        Action::Prompt { node, session, text } => {
            let req = PromptRequest {
                node_id: node,
                session_id: session,
                text,
            };
            report(c.inner.prompt(c.request(req)).await, "sent")
        }
        Action::Interrupt { node, session } => {
            let req = InterruptRequest {
                node_id: node,
                session_id: session,
                then_prompt: None,
            };
            report(c.inner.interrupt(c.request(req)).await, "interrupted")
        }
        Action::Kill { node, session } => {
            let req = KillRequest {
                node_id: node,
                session_id: session,
            };
            report(c.inner.kill(c.request(req)).await, "ended")
        }
        Action::Spawn {
            node,
            cwd,
            prompt,
            model,
            mode,
            token,
        } => {
            let req = SpawnRequest {
                node_id: node,
                cwd,
                prompt,
                model,
                create: true,
                permission_mode: mode,
                profile: String::new(),
                claude_token: token,
            };
            match c.inner.spawn(c.request(req)).await {
                Ok(r) => {
                    let r = r.into_inner();
                    if r.ok {
                        Ok(format!("started {}", &r.session_id[..r.session_id.len().min(8)]))
                    } else {
                        Err(r.error)
                    }
                }
                Err(s) => Err(s.message().to_string()),
            }
        }
    }
}
