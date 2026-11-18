//! Tails a session's transcript JSONL and its subagent transcripts (EXP-0002). Keeps the
//! `uuid`-bearing `user`, `assistant`, `system` and `attachment` lines, the ones a
//! conversation is chained through, so the hub's copy can rebuild a transcript a node no
//! longer has ([`rebuild`]); session-state lines are dropped. Usage is summed once per
//! `message.id`, since one API message is written as one line per content block.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::SeekFrom;
use std::path::{Path, PathBuf};
use std::time::Duration;

use saarthi_proto::event::v1::EventSource;
use serde::Serialize;
use serde_json::Value;
use sha2::Digest;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio::sync::watch;

use crate::outbox::Outbox;

const KEPT_TYPES: [&str; 4] = ["user", "assistant", "system", "attachment"];

/// Token totals for one model, as `message.usage` names them.
#[derive(Debug, Default, Clone, PartialEq, Serialize)]
pub struct ModelUsage {
    pub messages: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_input_tokens: u64,
    pub cache_creation_input_tokens: u64,
}

/// Usage for one session, main thread and subagents together, as `cost-state` counts it.
#[derive(Debug, Default)]
pub struct Usage {
    seen: HashSet<String>,
    pub by_model: BTreeMap<String, ModelUsage>,
}

impl Usage {
    /// Adds an assistant message's usage unless its id was already counted; true if added.
    fn add(&mut self, message: &Value) -> bool {
        let (Some(id), Some(usage)) = (message["id"].as_str(), message.get("usage")) else {
            return false;
        };
        if !self.seen.insert(id.to_string()) {
            return false;
        }
        let model = message["model"].as_str().unwrap_or("unknown").to_string();
        let m = self.by_model.entry(model).or_default();
        let n = |k: &str| usage[k].as_u64().unwrap_or(0);
        m.messages += 1;
        m.input_tokens += n("input_tokens");
        m.output_tokens += n("output_tokens");
        m.cache_read_input_tokens += n("cache_read_input_tokens");
        m.cache_creation_input_tokens += n("cache_creation_input_tokens");
        true
    }
}

/// What one transcript line turns into.
#[derive(Debug, PartialEq)]
pub enum Line {
    /// A line worth keeping: its `type`, the line as written and its `uuid`.
    Kept {
        kind: String,
        line: String,
        uuid: String,
    },
    /// A kept assistant line that also carried a message not counted before, by its id.
    KeptWithUsage {
        kind: String,
        line: String,
        uuid: String,
        message_id: String,
    },
    Dropped,
}

pub fn classify(raw: &str, usage: &mut Usage) -> Line {
    let Ok(value) = serde_json::from_str::<Value>(raw) else {
        return Line::Dropped;
    };
    let kind = value["type"].as_str().unwrap_or_default();
    let Some(uuid) = value["uuid"].as_str() else {
        return Line::Dropped;
    };
    if !KEPT_TYPES.contains(&kind) {
        return Line::Dropped;
    }
    let (kind, line, uuid) = (kind.to_string(), raw.to_string(), uuid.to_string());
    if kind == "assistant" && usage.add(&value["message"]) {
        let message_id = value["message"]["id"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        Line::KeptWithUsage {
            kind,
            line,
            uuid,
            message_id,
        }
    } else {
        Line::Kept { kind, line, uuid }
    }
}

/// A session's main transcript rebuilt from the lines the hub kept, in the order the node
/// read them, for `claude --resume`: subagents' lines (`isSidechain`) and repeats left out.
/// Claude Code follows each line's `parentUuid` back from the last; the hub kept no
/// attachment lines before they were sent, and a line whose parent is missing would end
/// the conversation there, so it is chained to the line before it instead.
pub fn rebuild(lines: &[String]) -> String {
    let mut seen = HashSet::new();
    let mut out = String::new();
    let mut last: Option<String> = None;
    for line in lines {
        let Ok(mut v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if v["isSidechain"].as_bool() == Some(true) {
            continue;
        }
        let Some(uuid) = v["uuid"].as_str().map(str::to_string) else {
            continue;
        };
        if !seen.insert(uuid.clone()) {
            continue;
        }
        let parent = v["parentUuid"].as_str().map(str::to_string);
        let repaired = match &parent {
            Some(p) if !seen.contains(p) => Some(last.clone().map_or(Value::Null, Value::String)),
            _ => None,
        };
        match repaired {
            Some(p) => {
                v["parentUuid"] = p;
                out.push_str(&v.to_string());
            }
            None => out.push_str(line),
        }
        out.push('\n');
        last = Some(uuid);
    }
    out
}

/// Where a session's subagent transcripts live: `<dir>/<session id>/subagents/`.
fn subagent_dir(transcript: &Path) -> PathBuf {
    transcript.with_extension("").join("subagents")
}

/// One file being followed: how far it has been read and any partial last line.
struct Follow {
    offset: u64,
    partial: Vec<u8>,
}

/// How far a session's transcripts have been read, kept across a stop so that a resumed
/// session continues where it left off instead of replaying its history.
#[derive(Default)]
pub struct TailState {
    usage: Usage,
    files: HashMap<PathBuf, Follow>,
    meta: Meta,
}

/// What a session is about and where, from its main transcript: its titles by where they came
/// from and its git branch, sent as a daemon `meta` event when the best of them changes (phase
/// PHS-20261117-f22kxe item 3). A name given to the session (`agent-name`, `custom-title`)
/// outranks Claude Code's generated `ai-title`, which outranks a `summary`, as atuin ranks a
/// thread's title; a title cleared falls back to the next.
///
/// A resumed or forked session's transcript starts with the lines of the session it came
/// from, under that session's id: the last such id before its own lines is its parent (phase
/// PHS-20261117-f22kxe item 15).
#[derive(Default)]
struct Meta {
    /// The session's own id, to tell its lines from those it copied.
    own: String,
    titles: [String; 4],
    branch: String,
    parent: String,
    /// Its own lines have begun: no copied line comes after them.
    begun: bool,
    /// The uuid of its first message line, and whether a fork's origin was looked for by it.
    first_uuid: String,
    looked: bool,
    sent: Option<(String, &'static str, String, String)>,
}

const TITLE_SOURCES: [(&str, &str, &str); 4] = [
    ("agent-name", "agentName", "name"),
    ("custom-title", "customTitle", "name"),
    ("ai-title", "aiTitle", "ai-title"),
    ("summary", "summary", "summary"),
];

impl Meta {
    /// Takes what a raw line says, without parsing the large lines it does not concern.
    fn observe(&mut self, raw: &str) {
        for (i, (kind, field, _)) in TITLE_SOURCES.iter().enumerate() {
            if raw.contains(&format!("\"type\":\"{kind}\"")) {
                if let Ok(v) = serde_json::from_str::<Value>(raw) {
                    self.titles[i] = v[*field].as_str().unwrap_or_default().trim().to_string();
                }
                return;
            }
        }
        if self.first_uuid.is_empty() && (raw.contains("\"type\":\"user\"") || raw.contains("\"type\":\"assistant\""))
            && let Some(at) = raw.find("\"uuid\":\"")
            && let Some(end) = raw[at + 8..].find('"')
        {
            self.first_uuid = raw[at + 8..at + 8 + end].to_string();
        }
        // Every message line names its session, before the session's own lines a copied one.
        if !self.begun && !self.own.is_empty() {
            if let Some(at) = raw.find("\"sessionId\":\"") {
                let rest = &raw[at + 13..];
                if let Some(end) = rest.find('"') {
                    if rest[..end] == self.own {
                        self.begun = true;
                    } else {
                        self.parent = rest[..end].to_string();
                    }
                }
            }
        }
        // Every message line names the branch; read it without parsing the line.
        if let Some(at) = raw.find("\"gitBranch\":\"") {
            let rest = &raw[at + 13..];
            if let Some(end) = rest.find('"') {
                self.branch = rest[..end].to_string();
            }
        }
    }

    /// The best title, its source, and the branch, when they differ from what was sent.
    fn changed(&mut self) -> Option<String> {
        let (title, source) = self
            .titles
            .iter()
            .zip(TITLE_SOURCES)
            .find(|(t, _)| !t.is_empty())
            .map(|(t, (_, _, source))| (t.clone(), source))
            .unwrap_or_default();
        let now = (title, source, self.branch.clone(), self.parent.clone());
        if self.sent.as_ref() == Some(&now) || (now.0.is_empty() && now.2.is_empty() && now.3.is_empty()) {
            return None;
        }
        let payload =
            serde_json::json!({"title": now.0, "title_source": now.1, "git_branch": now.2, "parent_id": now.3}).to_string();
        self.sent = Some(now);
        Some(payload)
    }
}

/// The session a fork came from. `--fork-session` copies the parent's lines under the new
/// id, so they do not name it, but they keep their uuids: the fork's first message is the
/// parent's. Of the transcripts beside it made before it, the earliest that starts with
/// the same message is its origin; only each file's first lines are read.
pub async fn fork_origin(transcript: &Path, first_uuid: &str) -> Option<String> {
    use tokio::io::AsyncBufReadExt;
    let born = |m: &std::fs::Metadata| m.created().or_else(|_| m.modified()).ok();
    let own = born(&tokio::fs::metadata(transcript).await.ok()?)?;
    let mut dir = tokio::fs::read_dir(transcript.parent()?).await.ok()?;
    let mut best: Option<(std::time::SystemTime, String)> = None;
    while let Ok(Some(entry)) = dir.next_entry().await {
        let path = entry.path();
        if path == transcript || path.extension().is_none_or(|e| e != "jsonl") {
            continue;
        }
        let Some(made) = entry.metadata().await.ok().as_ref().and_then(born) else { continue };
        if made >= own || best.as_ref().is_some_and(|(b, _)| made >= *b) {
            continue;
        }
        let Ok(file) = tokio::fs::File::open(&path).await else { continue };
        let mut lines = tokio::io::BufReader::new(file).lines();
        // Its first message line, within the bookkeeping lines a transcript starts with.
        for _ in 0..50 {
            let Ok(Some(line)) = lines.next_line().await else { break };
            if !(line.contains("\"type\":\"user\"") || line.contains("\"type\":\"assistant\"")) {
                continue;
            }
            if line.contains(&format!("\"uuid\":\"{first_uuid}\"")) {
                let id = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default().to_string();
                best = Some((made, id));
            }
            break;
        }
    }
    best.map(|(_, id)| id)
}

/// Follows a session's transcripts until `stop` turns true, then reads once more and returns
/// its state. A fresh state starts at the beginning of each file, so a session that predates
/// the node is backfilled.
pub async fn tail(
    session_id: String,
    transcript: PathBuf,
    outbox: Outbox,
    poll: Duration,
    mut stop: watch::Receiver<bool>,
    state: TailState,
) -> TailState {
    let TailState {
        mut usage,
        mut files,
        mut meta,
    } = state;
    meta.own.clone_from(&session_id);
    loop {
        let stopping = *stop.borrow();
        let mut paths = vec![transcript.clone()];
        if let Ok(mut dir) = tokio::fs::read_dir(subagent_dir(&transcript)).await {
            while let Ok(Some(entry)) = dir.next_entry().await {
                let path = entry.path();
                if path.extension().is_some_and(|e| e == "jsonl") {
                    paths.push(path);
                }
            }
        }
        for path in paths {
            let follow = files.entry(path.clone()).or_insert(Follow {
                offset: 0,
                partial: Vec::new(),
            });
            let main = path == transcript;
            for raw in read_new_lines(&path, follow).await {
                if main {
                    meta.observe(&raw);
                }
                match classify(&raw, &mut usage) {
                    Line::Dropped => {}
                    Line::Kept { kind, line, uuid } => {
                        outbox
                            .line(&session_id, EventSource::Transcript, &kind, line, uuid)
                            .await;
                    }
                    Line::KeptWithUsage {
                        kind,
                        line,
                        uuid,
                        message_id,
                    } => {
                        outbox
                            .line(&session_id, EventSource::Transcript, &kind, line, uuid)
                            .await;
                        let totals = serde_json::to_string(&usage.by_model).unwrap_or_default();
                        let id = format!("usage:{message_id}");
                        outbox
                            .line(&session_id, EventSource::Daemon, "usage", totals, id)
                            .await;
                    }
                }
            }
        }
        // A fork's origin, looked for once its first message is in, if its lines named none.
        if !meta.looked && !meta.first_uuid.is_empty() {
            meta.looked = true;
            if meta.parent.is_empty()
                && let Some(origin) = fork_origin(&transcript, &meta.first_uuid).await
            {
                meta.parent = origin;
            }
        }
        if let Some(payload) = meta.changed() {
            // Once per distinct value: a node re-reading the transcript sends nothing new.
            let id = format!("meta:{}", hex::encode(&sha2::Sha256::digest(payload.as_bytes())[..8]));
            outbox.line(&session_id, EventSource::Daemon, "meta", payload, id).await;
        }
        if stopping {
            return TailState { usage, files, meta };
        }
        tokio::select! {
            _ = tokio::time::sleep(poll) => {}
            _ = stop.changed() => {}
        }
    }
}

/// Complete lines appended since the last read. A file that shrank is read from the start.
async fn read_new_lines(path: &Path, follow: &mut Follow) -> Vec<String> {
    let Ok(mut file) = tokio::fs::File::open(path).await else {
        return Vec::new();
    };
    let len = file.metadata().await.map(|m| m.len()).unwrap_or(0);
    if len < follow.offset {
        *follow = Follow {
            offset: 0,
            partial: Vec::new(),
        };
    }
    if len == follow.offset || file.seek(SeekFrom::Start(follow.offset)).await.is_err() {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).await.is_err() {
        return Vec::new();
    }
    follow.offset += bytes.len() as u64;
    follow.partial.extend_from_slice(&bytes);
    // Only complete lines are decoded, so a character split across two reads stays whole.
    let Some(end) = follow.partial.iter().rposition(|&b| b == b'\n') else {
        return Vec::new();
    };
    let complete: Vec<u8> = follow.partial.drain(..=end).collect();
    String::from_utf8_lossy(&complete)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assistant(msg_id: &str, block: &str) -> String {
        serde_json::json!({
            "type": "assistant", "uuid": format!("u-{msg_id}-{block}"), "sessionId": "s1",
            "message": {
                "id": msg_id, "model": "claude-opus-5-5", "role": "assistant",
                "content": [{"type": block}],
                "usage": {"input_tokens": 4, "output_tokens": 280,
                          "cache_read_input_tokens": 35321, "cache_creation_input_tokens": 20779}
            }
        })
        .to_string()
    }

    #[test]
    fn keeps_message_and_attachment_lines_and_drops_the_rest() {
        let mut usage = Usage::default();
        let user = r#"{"type":"user","uuid":"a","message":{"role":"user","content":"hi"}}"#;
        let system = r#"{"type":"system","subtype":"turn_duration","uuid":"b","durationMs":5}"#;
        let attachment = r#"{"type":"attachment","uuid":"c","attachment":{"type":"environment"}}"#;
        let state = r#"{"type":"last-prompt","sessionId":"s1","lastPrompt":"hi"}"#;
        assert!(
            matches!(classify(user, &mut usage), Line::Kept { kind, uuid, .. } if kind == "user" && uuid == "a")
        );
        assert!(
            matches!(classify(system, &mut usage), Line::Kept { kind, .. } if kind == "system")
        );
        assert!(
            matches!(classify(attachment, &mut usage), Line::Kept { kind, .. } if kind == "attachment")
        );
        assert_eq!(classify(state, &mut usage), Line::Dropped);
        assert_eq!(classify("not json", &mut usage), Line::Dropped);
    }

    #[test]
    fn a_transcript_is_rebuilt_whole_from_the_hubs_lines() {
        let line = |v: serde_json::Value| v.to_string();
        let lines = vec![
            line(serde_json::json!({"type": "user", "uuid": "a", "parentUuid": null})),
            // An attachment the hub never had sits between "a" and "b".
            line(serde_json::json!({"type": "assistant", "uuid": "b", "parentUuid": "gone"})),
            line(serde_json::json!({"type": "user", "uuid": "s", "parentUuid": "b", "isSidechain": true})),
            line(serde_json::json!({"type": "user", "uuid": "c", "parentUuid": "b"})),
            line(serde_json::json!({"type": "user", "uuid": "c", "parentUuid": "b"})),
            "not json".to_string(),
        ];
        let out: Vec<serde_json::Value> =
            rebuild(&lines).lines().map(|l| serde_json::from_str(l).unwrap()).collect();
        let chain: Vec<(&str, Option<&str>)> =
            out.iter().map(|v| (v["uuid"].as_str().unwrap(), v["parentUuid"].as_str())).collect();
        assert_eq!(chain, [("a", None), ("b", Some("a")), ("c", Some("b"))]);
        // A line whose parent is there is written as it was read.
        assert_eq!(rebuild(&lines[..1]), format!("{}\n", lines[0]));
    }

    #[test]
    fn a_session_is_titled_by_its_best_source_and_named_over_generated() {
        let mut m = Meta::default();
        assert_eq!(m.changed(), None, "nothing known yet");
        m.observe(r#"{"type":"user","uuid":"a","gitBranch":"main","message":{"content":"hi"}}"#);
        m.observe(r#"{"type":"ai-title","aiTitle":"Fix the parser","sessionId":"s"}"#);
        let v: Value = serde_json::from_str(&m.changed().unwrap()).unwrap();
        assert_eq!((v["title"].as_str(), v["title_source"].as_str(), v["git_branch"].as_str()), (Some("Fix the parser"), Some("ai-title"), Some("main")));
        m.observe(r#"{"type":"ai-title","aiTitle":"Fix the parser","sessionId":"s"}"#);
        assert_eq!(m.changed(), None, "the same again sends nothing");
        m.observe(r#"{"type":"agent-name","agentName":"Parser rework","sessionId":"s"}"#);
        let v: Value = serde_json::from_str(&m.changed().unwrap()).unwrap();
        assert_eq!((v["title"].as_str(), v["title_source"].as_str()), (Some("Parser rework"), Some("name")));
        // A name cleared falls back to the generated title (after the parent, below).
        m.observe(r#"{"type":"agent-name","agentName":"","sessionId":"s"}"#);
        let v: Value = serde_json::from_str(&m.changed().unwrap()).unwrap();
        assert_eq!(v["title"], "Fix the parser");
    }

    #[tokio::test]
    async fn a_fork_is_traced_to_the_earliest_transcript_that_starts_like_it() {
        let dir = tempfile::tempdir().unwrap();
        let write = |name: &str, first: &str| {
            let body = format!(
                "{{\"type\":\"mode\",\"sessionId\":\"{name}\"}}\n{{\"type\":\"user\",\"uuid\":\"{first}\",\"sessionId\":\"{name}\"}}\n"
            );
            std::fs::write(dir.path().join(format!("{name}.jsonl")), body).unwrap();
            std::thread::sleep(std::time::Duration::from_millis(20));
        };
        write("original", "m1");
        write("unrelated", "m9");
        write("fork", "m1");
        write("later", "m1");
        let fork = dir.path().join("fork.jsonl");
        assert_eq!(fork_origin(&fork, "m1").await.as_deref(), Some("original"));
        // The original has none: what starts like it came after it.
        assert_eq!(fork_origin(&dir.path().join("original.jsonl"), "m1").await, None);
        assert_eq!(fork_origin(&dir.path().join("unrelated.jsonl"), "m9").await, None);
    }

    #[test]
    fn a_resumed_session_names_the_session_it_continues() {
        let mut m = Meta { own: "child".into(), ..Default::default() };
        // A chain: the grandparent's lines, then the parent's, then its own.
        m.observe(r#"{"type":"user","uuid":"a","sessionId":"grandparent","message":{"content":"one"}}"#);
        m.observe(r#"{"type":"user","uuid":"b","sessionId":"parent","message":{"content":"two"}}"#);
        m.observe(r#"{"type":"user","uuid":"c","sessionId":"child","message":{"content":"three"}}"#);
        // A line of another session after its own (a subagent's, say) changes nothing.
        m.observe(r#"{"type":"user","uuid":"d","sessionId":"other","message":{"content":"four"}}"#);
        let v: Value = serde_json::from_str(&m.changed().unwrap()).unwrap();
        assert_eq!(v["parent_id"], "parent");
        let mut fresh = Meta { own: "s".into(), ..Default::default() };
        fresh.observe(r#"{"type":"user","uuid":"a","sessionId":"s","gitBranch":"main","message":{"content":"hi"}}"#);
        let v: Value = serde_json::from_str(&fresh.changed().unwrap()).unwrap();
        assert_eq!(v["parent_id"], "");
    }

    #[test]
    fn usage_counts_each_message_once() {
        // Two content blocks of one message, then a second message: the EXP-0002 work5 case.
        let mut usage = Usage::default();
        assert!(matches!(
            classify(&assistant("msg_1", "thinking"), &mut usage),
            Line::KeptWithUsage { uuid, message_id, .. } if uuid == "u-msg_1-thinking" && message_id == "msg_1"
        ));
        assert!(matches!(
            classify(&assistant("msg_1", "text"), &mut usage),
            Line::Kept { uuid, .. } if uuid == "u-msg_1-text"
        ));
        assert!(matches!(
            classify(&assistant("msg_2", "text"), &mut usage),
            Line::KeptWithUsage { .. }
        ));
        let m = &usage.by_model["claude-opus-5-5"];
        assert_eq!(m.messages, 2);
        assert_eq!(m.output_tokens, 560);
        assert_eq!(m.cache_read_input_tokens, 70642);
    }

    #[tokio::test]
    async fn tails_main_and_subagent_files_across_partial_writes() {
        use std::io::Write;
        let dir = tempfile::tempdir().unwrap();
        let transcript = dir.path().join("s1.jsonl");
        let sub = dir.path().join("s1/subagents");
        std::fs::create_dir_all(&sub).unwrap();
        let mut main = std::fs::File::create(&transcript).unwrap();
        let line = assistant("msg_1", "text");
        // Half a line first: nothing is emitted until the newline lands.
        write!(main, "{}", &line[..20]).unwrap();

        let (outbox, mut rx) = Outbox::new(64);
        let (stop_tx, stop_rx) = watch::channel(false);
        let task = tokio::spawn(tail(
            "s1".into(),
            transcript.clone(),
            outbox,
            Duration::from_millis(20),
            stop_rx,
            TailState::default(),
        ));
        tokio::time::sleep(Duration::from_millis(60)).await;
        writeln!(main, "{}", &line[20..]).unwrap();
        std::fs::write(sub.join("agent-x.jsonl"), assistant("msg_2", "text") + "\n").unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        stop_tx.send(true).unwrap();
        task.await.unwrap();

        let mut kinds = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            if let Some(saarthi_proto::node::v1::node_message::Body::Event(e)) = msg.body {
                kinds.push(e.kind);
            }
        }
        assert_eq!(kinds, ["assistant", "usage", "assistant", "usage"]);
    }
}
