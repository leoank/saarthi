//! Prompts queued while a session works (phase PHS-20261117-f22kxe item 7). A client sends a
//! prompt with `queue` set; if the session is busy the node holds it here instead of handing
//! it to Claude Code, whose own queue nobody could see or take back, and delivers the oldest
//! when the session is next idle, one per turn. Each change goes out as the session's daemon
//! `queue` event, the whole list, so a client shows it from the conversation it streams. A
//! queued prompt can be sent at once (Claude Code takes it at its next step) or discarded.
//!
//! The queue is in memory, as the node's other session state: after a restart the last
//! `queue` event may list prompts the node no longer holds, and taking one of them sends the
//! queue as it is now, which clears the client's.

use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use serde::Serialize;
use tokio::sync::Mutex;

use crate::sessions::Sessions;

/// How often the node looks for a queued prompt whose session went idle.
const TICK: Duration = Duration::from_secs(1);
/// After a delivery, the next waits until the session was seen busy, or this long: a turn
/// that started and ended between two looks would otherwise hold the queue for good.
const SETTLE: Duration = Duration::from_secs(15);

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Queued {
    pub id: String,
    pub text: String,
    pub time_ms: i64,
}

#[derive(Default)]
struct Line {
    items: VecDeque<Queued>,
    /// When the last prompt went from here, until the session was seen busy after it.
    delivered: Option<Instant>,
}

pub struct Queue {
    sessions: Arc<Sessions>,
    lines: Mutex<HashMap<String, Line>>,
}

impl Queue {
    pub fn new(sessions: Arc<Sessions>) -> Arc<Self> {
        Arc::new(Self { sessions, lines: Mutex::new(HashMap::new()) })
    }

    /// Holds `text` for session `id`; its queue id.
    pub async fn push(&self, id: &str, text: &str) -> String {
        let q = Queued {
            id: uuid::Uuid::new_v4().simple().to_string(),
            text: text.to_string(),
            time_ms: crate::outbox::now_ms(),
        };
        let mut lines = self.lines.lock().await;
        let line = lines.entry(id.to_string()).or_default();
        line.items.push_back(q.clone());
        self.report(id, &line.items).await;
        q.id
    }

    /// Takes queued prompt `queued_id` off session `id`'s queue. When it is not there (sent
    /// already, or lost with a restart) the queue as it is goes out again.
    pub async fn take(&self, id: &str, queued_id: &str) -> Option<String> {
        let mut lines = self.lines.lock().await;
        let line = lines.entry(id.to_string()).or_default();
        let found = line.items.iter().position(|q| q.id == queued_id).and_then(|i| line.items.remove(i));
        self.report(id, &line.items).await;
        found.map(|q| q.text)
    }

    /// The prompts due now: the oldest of each live, idle session's queue, if the last one
    /// delivered from it has been taken up; a session that ended loses its queue.
    async fn due(&self) -> Vec<(String, Queued)> {
        let mut lines = self.lines.lock().await;
        let mut due = Vec::new();
        let mut gone = Vec::new();
        for (id, line) in lines.iter_mut() {
            match self.sessions.idle(id).await {
                None => {
                    if !line.items.is_empty() {
                        line.items.clear();
                        self.report(id, &line.items).await;
                    }
                    gone.push(id.clone());
                }
                Some(false) => line.delivered = None,
                Some(true) if line.items.is_empty() => {}
                Some(true) if line.delivered.is_some_and(|t| t.elapsed() < SETTLE) => {}
                Some(true) => {
                    let q = line.items.pop_front().expect("not empty");
                    line.delivered = Some(Instant::now());
                    self.report(id, &line.items).await;
                    due.push((id.clone(), q));
                }
            }
        }
        lines.retain(|id, l| !gone.contains(id) && (!l.items.is_empty() || l.delivered.is_some()));
        due
    }

    /// Puts back a prompt whose delivery failed, at the front, for the person to see.
    async fn unshift(&self, id: &str, q: Queued) {
        let mut lines = self.lines.lock().await;
        let line = lines.entry(id.to_string()).or_default();
        line.items.push_front(q);
        self.report(id, &line.items).await;
    }

    async fn report(&self, id: &str, items: &VecDeque<Queued>) {
        let payload = serde_json::json!({ "items": items }).to_string();
        self.sessions.daemon_event(id, "queue", payload).await;
    }
}

/// Delivers queued prompts as their sessions go idle, with `deliver` (the link's prompt
/// delivery), until the node stops.
pub async fn run<F, Fut>(queue: Arc<Queue>, deliver: F)
where
    F: Fn(String, String) -> Fut,
    Fut: std::future::Future<Output = Result<String, String>>,
{
    let mut tick = tokio::time::interval(TICK);
    loop {
        tick.tick().await;
        for (id, q) in queue.due().await {
            if let Err(e) = deliver(id.clone(), q.text.clone()).await {
                tracing::warn!(session = %id, error = %e, "a queued prompt could not be delivered");
                queue.unshift(&id, q).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::outbox::Outbox;
    use crate::registry::Record;

    fn record(id: &str, status: &str) -> Record {
        Record {
            pid: std::process::id(),
            session_id: id.into(),
            cwd: "/w".into(),
            started_at: 1,
            status: status.into(),
            messaging_socket_path: String::new(),
            claude_dir: None,
            profile: String::new(),
        }
    }

    #[tokio::test]
    async fn a_queued_prompt_goes_when_its_session_is_idle_and_one_per_turn() {
        let dir = tempfile::tempdir().unwrap();
        let (outbox, _rx) = Outbox::new(64);
        let sessions = Arc::new(Sessions::new(dir.path().into(), outbox, Duration::from_secs(60), Vec::new()));
        sessions.apply_registry(&[record("s", "busy")]).await;
        let queue = Queue::new(sessions.clone());
        queue.push("s", "one").await;
        let two = queue.push("s", "two").await;
        assert!(queue.due().await.is_empty(), "busy: nothing goes");

        sessions.apply_registry(&[record("s", "idle")]).await;
        let due = queue.due().await;
        assert_eq!(due.iter().map(|(_, q)| q.text.as_str()).collect::<Vec<_>>(), ["one"]);
        // Still idle a moment later: the first has not been taken up yet, so the second waits.
        assert!(queue.due().await.is_empty());
        sessions.apply_registry(&[record("s", "busy")]).await;
        assert!(queue.due().await.is_empty());
        sessions.apply_registry(&[record("s", "idle")]).await;
        assert_eq!(queue.due().await[0].1.id, two);

        // Taken off: sent now or discarded, and gone from the queue either way.
        let three = queue.push("s", "three").await;
        assert_eq!(queue.take("s", &three).await.as_deref(), Some("three"));
        assert_eq!(queue.take("s", &three).await, None);

        // A session that ends loses its queue.
        queue.push("s", "four").await;
        sessions.apply_registry(&[]).await;
        assert!(queue.due().await.is_empty());
        assert!(queue.lines.lock().await.get("s").is_none());
    }
}
