//! The node's view of its sessions, merged from two sources: hooks, which name the session
//! and its transcript, and the session registry, which adds the pid and idle or busy status
//! and finds sessions that predate the node. Each known session has a transcript tailer.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Duration;

use saarthi_proto::event::v1::{Launch, Session, SessionStatus};
use serde_json::Value;
use tokio::sync::{Mutex, watch};
use tokio::task::JoinHandle;

use crate::outbox::{Outbox, now_ms};
use crate::registry::Record;
use crate::transcript::{self, TailState};

const HARNESS: &str = "claude-code";

pub struct Sessions {
    /// Working directories whose sessions are not tracked at all (ADR-0028).
    ignore: Vec<glob::Pattern>,
    claude_dir: PathBuf,
    outbox: Outbox,
    poll: Duration,
    entries: Mutex<HashMap<String, Entry>>,
    /// Pids running under an `saarthi run` wrapper.
    wrapped: Mutex<HashSet<u32>>,
}

struct Entry {
    session: Session,
    /// Seen in the registry, so leaving it means the process is gone.
    in_registry: bool,
    /// From the registry: where a prompt can be delivered.
    socket: String,
    /// From the latest hook: `default`, `plan`, `acceptEdits` or `bypassPermissions`.
    permission_mode: String,
    /// Started by the node's spawner, which owns its input and knows idle from busy; the
    /// registry's status of such a session stays `busy` (EXP-20261106-37f79k).
    spawned: bool,
    tail: Option<Tail>,
}

/// Where and how to deliver a prompt to a running session.
#[derive(Debug, Clone, PartialEq)]
pub struct PromptTarget {
    pub pid: u32,
    pub socket: PathBuf,
    pub permission_mode: String,
}

struct Tail {
    stop: watch::Sender<bool>,
    task: JoinHandle<TailState>,
}

impl Sessions {
    pub fn new(claude_dir: PathBuf, outbox: Outbox, poll: Duration, ignore: Vec<glob::Pattern>) -> Self {
        Self {
            ignore,
            claude_dir,
            outbox,
            poll,
            entries: Mutex::new(HashMap::new()),
            wrapped: Mutex::new(HashSet::new()),
        }
    }

    /// A session in an ignored working directory: not listed, not tailed, not held.
    pub fn ignored(&self, cwd: &str) -> bool {
        self.ignore.iter().any(|p| p.matches(cwd))
    }

    /// Every session the node knows, for the registration message after a (re)connect.
    pub async fn snapshot(&self) -> Vec<Session> {
        self.entries
            .lock()
            .await
            .values()
            .map(|e| e.session.clone())
            .collect()
    }

    /// Marks a pid as running under a wrapper, or no longer, and updates its session.
    pub async fn set_wrapped(&self, pid: u32, wrapped: bool) {
        {
            let mut set = self.wrapped.lock().await;
            if wrapped {
                set.insert(pid);
            } else {
                set.remove(&pid);
            }
        }
        let mut entries = self.entries.lock().await;
        for entry in entries.values_mut().filter(|e| e.session.pid == pid) {
            let launch = if wrapped {
                Launch::Wrapper
            } else {
                Launch::Bare
            };
            if entry.session.launch() != launch {
                entry.session.launch = launch.into();
                self.outbox.session(entry.session.clone()).await;
            }
        }
    }

    pub async fn is_wrapped(&self, id: &str) -> bool {
        match self.pid(id).await {
            Some(pid) => self.wrapped.lock().await.contains(&pid),
            None => false,
        }
    }

    /// Known and not ended.
    pub async fn is_live(&self, id: &str) -> bool {
        let entries = self.entries.lock().await;
        entries.get(id).is_some_and(|e| e.session.ended_ms.is_none())
    }

    pub async fn is_spawned(&self, id: &str) -> bool {
        let entries = self.entries.lock().await;
        entries.get(id).is_some_and(|e| e.spawned)
    }

    /// Starts tracking a session the spawner has just started, before the registry or a
    /// hook reports it: spawned, busy when it was given a first prompt, idle otherwise, its
    /// transcript tailed (from when the first prompt writes it) in `claude_dir`, the node's
    /// own config directory or a sandboxed profile's, under the launch `profile` (empty for
    /// the host) and the named Claude token `claude_token` (empty for the node's own login).
    #[allow(clippy::too_many_arguments)]
    pub async fn add_spawned(
        &self,
        id: &str,
        pid: u32,
        cwd: &str,
        claude_dir: &Path,
        profile: &str,
        claude_token: &str,
        busy: bool,
    ) {
        let mut entries = self.entries.lock().await;
        let entry = entries.entry(id.to_string()).or_insert_with(|| Entry::new(id));
        entry.spawned = true;
        let s = &mut entry.session;
        s.launch = Launch::Spawned.into();
        s.pid = pid;
        s.cwd = cwd.to_string();
        s.transcript_path = transcript_path(claude_dir, cwd, id)
            .to_string_lossy()
            .into_owned();
        s.status = if busy { SessionStatus::Busy } else { SessionStatus::Idle }.into();
        s.profile = profile.to_string();
        s.claude_token = claude_token.to_string();
        s.ended_ms = None;
        self.ensure_tail(entry).await;
        self.outbox.session(entry.session.clone()).await;
    }

    /// A fact about a session only the daemon knows, such as a spawned session's rate-limit
    /// windows from its stream.
    pub async fn daemon_event(&self, id: &str, kind: &str, payload_json: String) {
        self.outbox
            .event(
                id,
                saarthi_proto::event::v1::EventSource::Daemon,
                kind,
                payload_json,
            )
            .await;
    }

    /// Sets an open session's status, as the spawner reads it from the stream.
    pub async fn set_status(&self, id: &str, status: SessionStatus) {
        let mut entries = self.entries.lock().await;
        let Some(entry) = entries.get_mut(id) else {
            return;
        };
        if entry.session.ended_ms.is_none() && entry.session.status() != status {
            entry.session.status = status.into();
            self.outbox.session(entry.session.clone()).await;
        }
    }

    /// Ends a session whose process the node saw exit, if nothing has ended it yet.
    pub async fn finish(&self, id: &str) {
        let mut entries = self.entries.lock().await;
        let Some(entry) = entries.get_mut(id) else {
            return;
        };
        if entry.session.ended_ms.is_none() {
            self.end(entry).await;
            self.outbox.session(entry.session.clone()).await;
        }
    }

    /// The session's pid, once the registry has given it.
    pub async fn pid(&self, id: &str) -> Option<u32> {
        let entries = self.entries.lock().await;
        entries.get(id).map(|e| e.session.pid).filter(|&p| p != 0)
    }

    /// The open session a pid belongs to.
    pub async fn session_for_pid(&self, pid: u32) -> Option<String> {
        let entries = self.entries.lock().await;
        entries
            .values()
            .find(|e| e.session.pid == pid && e.session.ended_ms.is_none())
            .map(|e| e.session.id.clone())
    }

    pub fn claude_dir(&self) -> &Path {
        &self.claude_dir
    }

    /// Where to deliver a prompt to a session, or why it cannot be prompted.
    pub async fn prompt_target(&self, id: &str) -> Result<PromptTarget, String> {
        let entries = self.entries.lock().await;
        let Some(e) = entries.get(id) else {
            return Err("no such session on this node".into());
        };
        if e.session.ended_ms.is_some() {
            return Err("the session has ended".into());
        }
        if e.socket.is_empty() {
            return Err("the session has no messaging socket in Claude Code's registry".into());
        }
        Ok(PromptTarget {
            pid: e.session.pid,
            socket: PathBuf::from(&e.socket),
            permission_mode: e.permission_mode.clone(),
        })
    }

    /// Updates the session a hook input names. SessionEnd ends it; any other hook on an
    /// ended session reopens it, as `--resume` does.
    pub async fn apply_hook(&self, input: &Value) {
        let Some(id) = input["session_id"].as_str() else {
            return;
        };
        if self.ignored(input["cwd"].as_str().unwrap_or_default()) {
            return;
        }
        let mut entries = self.entries.lock().await;
        let entry = entries
            .entry(id.to_string())
            .or_insert_with(|| Entry::new(id));
        let before = entry.session.clone();
        let s = &mut entry.session;
        if let Some(cwd) = input["cwd"].as_str() {
            s.cwd = cwd.to_string();
        }
        if let Some(path) = input["transcript_path"].as_str() {
            s.transcript_path = path.to_string();
        }
        if let Some(mode) = input["permission_mode"].as_str() {
            entry.permission_mode = mode.to_string();
        }
        if input["hook_event_name"] == "SessionEnd" {
            self.end(entry).await;
        } else {
            entry.session.ended_ms = None;
            self.ensure_tail(entry).await;
        }
        if entry.session != before {
            self.outbox.session(entry.session.clone()).await;
        }
    }

    /// Applies a registry scan: records update their sessions, and a session that was in the
    /// registry and has left it has ended.
    pub async fn apply_registry(&self, live: &[Record]) {
        let mut entries = self.entries.lock().await;
        for record in live.iter().filter(|r| !self.ignored(&r.cwd)) {
            let entry = entries
                .entry(record.session_id.clone())
                .or_insert_with(|| Entry::new(&record.session_id));
            let before = entry.session.clone();
            entry.in_registry = true;
            entry.socket = record.messaging_socket_path.clone();
            let wrapped = self.wrapped.lock().await.contains(&record.pid);
            let s = &mut entry.session;
            s.pid = record.pid;
            if wrapped {
                s.launch = Launch::Wrapper.into();
            }
            if !entry.spawned {
                s.status = status(&record.status).into();
            }
            if s.cwd.is_empty() {
                s.cwd = record.cwd.clone();
            }
            if s.transcript_path.is_empty() {
                let claude_dir = record.claude_dir.as_deref().unwrap_or(&self.claude_dir);
                s.transcript_path =
                    transcript_path(claude_dir, &record.cwd, &record.session_id)
                        .to_string_lossy()
                        .into_owned();
            }
            if record.started_at > 0 {
                s.started_ms = record.started_at;
            }
            if !record.profile.is_empty() {
                s.profile = record.profile.clone();
            }
            s.ended_ms = None;
            self.ensure_tail(entry).await;
            if entry.session != before {
                self.outbox.session(entry.session.clone()).await;
            }
        }
        for entry in entries.values_mut() {
            let gone = entry.in_registry && !live.iter().any(|r| r.session_id == entry.session.id);
            if gone && entry.session.ended_ms.is_none() {
                self.end(entry).await;
                self.outbox.session(entry.session.clone()).await;
            }
        }
    }

    async fn ensure_tail(&self, entry: &mut Entry) {
        if entry.session.transcript_path.is_empty() {
            return;
        }
        let state = match entry.tail.take() {
            Some(tail) if !*tail.stop.borrow() => {
                entry.tail = Some(tail);
                return;
            }
            Some(tail) => tail.task.await.unwrap_or_default(),
            None => TailState::default(),
        };
        let (stop, stop_rx) = watch::channel(false);
        let task = tokio::spawn(transcript::tail(
            entry.session.id.clone(),
            PathBuf::from(&entry.session.transcript_path),
            self.outbox.clone(),
            self.poll,
            stop_rx,
            state,
        ));
        entry.tail = Some(Tail { stop, task });
    }

    async fn end(&self, entry: &mut Entry) {
        entry.session.ended_ms.get_or_insert_with(now_ms);
        entry.session.status = SessionStatus::Unspecified.into();
        entry.in_registry = false;
        if let Some(tail) = &entry.tail {
            // The tailer reads once more before it stops, so the last lines are not lost.
            let _ = tail.stop.send(true);
        }
    }
}

impl Entry {
    fn new(id: &str) -> Self {
        let session = Session {
            id: id.to_string(),
            harness: HARNESS.to_string(),
            launch: Launch::Bare.into(),
            started_ms: now_ms(),
            ..Default::default()
        };
        Self {
            session,
            in_registry: false,
            socket: String::new(),
            permission_mode: String::new(),
            spawned: false,
            tail: None,
        }
    }
}

fn status(registry_status: &str) -> SessionStatus {
    match registry_status {
        "idle" => SessionStatus::Idle,
        "busy" => SessionStatus::Busy,
        _ => SessionStatus::Unspecified,
    }
}

/// `<claude dir>/projects/<cwd with every non-alphanumeric replaced by "-">/<session>.jsonl`
/// (EXP-0002), for sessions found in the registry before any hook named the path.
pub fn transcript_path(claude_dir: &Path, cwd: &str, session_id: &str) -> PathBuf {
    let dir: String = cwd
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect();
    claude_dir
        .join("projects")
        .join(dir)
        .join(format!("{session_id}.jsonl"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use saarthi_proto::node::v1::node_message::Body;
    use tokio::sync::mpsc;

    fn drain_sessions(
        rx: &mut mpsc::Receiver<saarthi_proto::node::v1::NodeMessage>,
    ) -> Vec<Session> {
        let mut out = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            if let Some(Body::Session(s)) = msg.body {
                out.push(s);
            }
        }
        out
    }

    #[test]
    fn transcript_path_encodes_the_cwd() {
        let p = transcript_path(Path::new("/h/.claude"), "/Users/ank/work.dir", "abc");
        assert_eq!(
            p,
            Path::new("/h/.claude/projects/-Users-ank-work-dir/abc.jsonl")
        );
    }

    #[tokio::test]
    async fn sessions_in_an_ignored_directory_are_not_tracked() {
        let dir = tempfile::tempdir().unwrap();
        let (outbox, _rx) = Outbox::new(10);
        let ignore = vec![glob::Pattern::new("*/target/live-work-*").unwrap()];
        let sessions = Sessions::new(dir.path().to_path_buf(), outbox, Duration::from_millis(10), ignore);
        assert!(sessions.ignored("/w/saarthi/target/live-work-7"));
        assert!(!sessions.ignored("/w/saarthi"));
        let hook = serde_json::json!({"hook_event_name": "UserPromptSubmit", "session_id": "t1",
            "cwd": "/w/saarthi/target/live-work-7"});
        sessions.apply_hook(&hook).await;
        let record = Record {
            pid: std::process::id(),
            session_id: "t2".into(),
            cwd: "/w/saarthi/target/live-work-8".into(),
            started_at: 1,
            status: "idle".into(),
            messaging_socket_path: String::new(),
            claude_dir: None,
            profile: String::new(),
        };
        sessions.apply_registry(&[record]).await;
        assert!(sessions.snapshot().await.is_empty());
    }

    #[tokio::test]
    async fn hooks_and_registry_merge_into_one_session() {
        let dir = tempfile::tempdir().unwrap();
        let (outbox, mut rx) = Outbox::new(64);
        let sessions = Sessions::new(dir.path().to_path_buf(), outbox, Duration::from_millis(10), Vec::new());

        let start = serde_json::json!({"hook_event_name": "SessionStart", "session_id": "s1",
            "cwd": "/w", "transcript_path": dir.path().join("s1.jsonl"),
            "permission_mode": "bypassPermissions"});
        sessions.apply_hook(&start).await;
        assert!(
            sessions.prompt_target("s1").await.is_err(),
            "no socket known yet"
        );
        let record = Record {
            pid: 42,
            session_id: "s1".into(),
            cwd: "/w".into(),
            started_at: 7,
            status: "busy".into(),
            messaging_socket_path: "/tmp/cc-socks/42.sock".into(),
            claude_dir: None,
            profile: String::new(),
        };
        sessions.apply_registry(std::slice::from_ref(&record)).await;
        assert_eq!(
            sessions.prompt_target("s1").await.unwrap(),
            PromptTarget {
                pid: 42,
                socket: "/tmp/cc-socks/42.sock".into(),
                permission_mode: "bypassPermissions".into()
            }
        );
        // Unchanged scans say nothing.
        sessions.apply_registry(&[record]).await;
        // The process left the registry: the session ended.
        sessions.apply_registry(&[]).await;
        assert_eq!(
            sessions.prompt_target("s1").await.unwrap_err(),
            "the session has ended"
        );

        let seen = drain_sessions(&mut rx);
        assert_eq!(seen.len(), 3);
        assert_eq!(seen[0].cwd, "/w");
        assert_eq!((seen[1].pid, seen[1].status()), (42, SessionStatus::Busy));
        assert!(seen[2].ended_ms.is_some());
    }

    #[tokio::test]
    async fn a_session_in_a_profiles_registry_is_listed_under_that_profile() {
        let dir = tempfile::tempdir().unwrap();
        let (outbox, _rx) = Outbox::new(64);
        let sessions = Sessions::new(dir.path().to_path_buf(), outbox, Duration::from_millis(10), Vec::new());
        let boxed = dir.path().join("box/claude");
        let record = Record {
            pid: std::process::id(),
            session_id: "s2".into(),
            cwd: "/w".into(),
            started_at: 1,
            status: "idle".into(),
            messaging_socket_path: String::new(),
            claude_dir: Some(boxed.clone()),
            profile: "box".into(),
        };
        sessions.apply_registry(&[record]).await;
        let s = sessions.snapshot().await.pop().unwrap();
        assert_eq!(s.profile, "box");
        assert_eq!(Path::new(&s.transcript_path), transcript_path(&boxed, "/w", "s2"));
    }

    #[tokio::test]
    async fn a_hook_after_session_end_reopens_the_session() {
        let dir = tempfile::tempdir().unwrap();
        let (outbox, mut rx) = Outbox::new(64);
        let sessions = Sessions::new(dir.path().to_path_buf(), outbox, Duration::from_millis(10), Vec::new());
        let hook = |name: &str| serde_json::json!({"hook_event_name": name, "session_id": "s1", "cwd": "/w"});

        sessions.apply_hook(&hook("SessionStart")).await;
        sessions.apply_hook(&hook("SessionEnd")).await;
        sessions.apply_hook(&hook("UserPromptSubmit")).await;

        let seen = drain_sessions(&mut rx);
        assert_eq!(seen.len(), 3);
        assert!(seen[1].ended_ms.is_some());
        assert!(seen[2].ended_ms.is_none());
    }
}
