//! Search across sessions (ADR-0063, phase PHS-20261117-f22kxe item 9), after atuin's: SQLite
//! FTS5 indexes that store no text (migration 0009), one over the words of transcript lines,
//! one over session titles and directories. A background task fills them from the record and
//! keeps them current, so a new line is found a second or two after it arrives. A query
//! matches every word, the last as a prefix (it may still be being typed); a session ranks
//! by its best match, a title match counting five times, and an older session lower. The
//! snippet is cut from the matching line, which the record still holds.

use std::collections::HashMap;
use std::time::Duration;

use anyhow::Result;
use saarthi_proto::event::v1::{EventSource, Session};
use serde_json::Value;
use sqlx::Row;

use crate::db::{Db, session_from};

/// Rows the indexer reads at a time; a full batch means it is behind and goes on at once.
const BATCH: i64 = 2000;
/// How often a caught-up indexer looks for new rows.
const EVERY: Duration = Duration::from_secs(2);
/// A match in a title counts this many times one in the transcript.
const TITLE_WEIGHT: f64 = 5.0;
/// A session this many days old ranks at half its match; older, lower still.
const HALF_DAYS: f64 = 30.0;
/// The snippet's length, in characters.
const SNIPPET: usize = 160;
/// How many matching lines are read to find the best sessions.
const LINES: i64 = 500;

/// One session a search found: the session, the words around its best match in the
/// transcript (empty when only its title matched), and that line's seq (0 then).
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub session: Session,
    pub snippet: String,
    pub seq: u64,
}

/// The words a transcript line adds to the index: a typed prompt, a reply's text. A tool's
/// result, text injected for the model, and every other kind of line add none.
pub fn searchable(kind: &str, payload: &str) -> Option<String> {
    if kind != "user" && kind != "assistant" {
        return None;
    }
    let v: Value = serde_json::from_str(payload).ok()?;
    if v["isMeta"] == true {
        return None;
    }
    let text = match &v["message"]["content"] {
        Value::String(s) => s.clone(),
        Value::Array(blocks) => blocks
            .iter()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        _ => return None,
    };
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_string())
}

/// The words of a query, lowercased.
fn words(query: &str) -> Vec<String> {
    query
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// The FTS5 query for what was typed: every word, each quoted, the last a prefix unless the
/// query ends in a space. None when there is no word in it.
pub fn fts_query(query: &str) -> Option<String> {
    let ws = words(query);
    let last = ws.len().checked_sub(1)?;
    let open = !query.ends_with(char::is_whitespace);
    Some(
        ws.iter()
            .enumerate()
            .map(|(i, w)| if i == last && open { format!("\"{w}\"*") } else { format!("\"{w}\"") })
            .collect::<Vec<_>>()
            .join(" "),
    )
}

/// About `SNIPPET` characters of `text` around the first word of the query found in it,
/// whitespace flattened, with an ellipsis where it was cut.
pub fn snippet(text: &str, query: &str) -> String {
    let flat: Vec<char> = text.split_whitespace().collect::<Vec<_>>().join(" ").chars().collect();
    let lower: Vec<char> = flat.iter().flat_map(|c| c.to_lowercase()).collect();
    // Lowercasing can change a character's length; fall back to the start when it did.
    let at = if lower.len() == flat.len() {
        words(query)
            .iter()
            .filter_map(|w| {
                let w: Vec<char> = w.chars().collect();
                lower.windows(w.len()).position(|win| win == w.as_slice())
            })
            .min()
            .unwrap_or(0)
    } else {
        0
    };
    let start = at.saturating_sub(SNIPPET / 3);
    let end = (start + SNIPPET).min(flat.len());
    let start = end.saturating_sub(SNIPPET);
    let mut s: String = flat[start..end].iter().collect();
    if start > 0 {
        s = format!("…{s}");
    }
    if end < flat.len() {
        s.push('…');
    }
    s
}

/// How much less a session counts for its age: 1 today, a half at `HALF_DAYS`.
fn freshness(last_ms: i64, now_ms: i64) -> f64 {
    let days = ((now_ms - last_ms).max(0) as f64) / 86_400_000.0;
    1.0 / (1.0 + days / HALF_DAYS)
}

impl Db {
    /// Indexes the next rows of the record; how many it read, below `BATCH` once caught up.
    pub async fn index_search(&self) -> Result<usize> {
        let (events_upto, sessions_upto): (i64, i64) =
            sqlx::query_as("SELECT events_upto, sessions_upto FROM search_state WHERE id = 1")
                .fetch_one(&self.pool)
                .await?;
        let lines = sqlx::query(
            "SELECT rowid, kind, payload FROM events
             WHERE rowid > ?1 AND source = ?2 AND kind IN ('user', 'assistant')
             ORDER BY rowid LIMIT ?3",
        )
        .bind(events_upto)
        .bind(EventSource::Transcript.as_str_name())
        .bind(BATCH)
        .fetch_all(&self.pool)
        .await?;
        let sessions = sqlx::query("SELECT rowid, title, cwd FROM sessions WHERE rowid > ?1 ORDER BY rowid LIMIT ?2")
            .bind(sessions_upto)
            .bind(BATCH)
            .fetch_all(&self.pool)
            .await?;
        let mut tx = self.pool.begin().await?;
        for r in &lines {
            let (kind, payload): (String, String) = (r.get("kind"), r.get("payload"));
            if let Some(body) = searchable(&kind, &payload) {
                sqlx::query("INSERT OR REPLACE INTO search_text (rowid, body) VALUES (?1, ?2)")
                    .bind(r.get::<i64, _>("rowid"))
                    .bind(body)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        for r in &sessions {
            sqlx::query("INSERT OR REPLACE INTO search_title (rowid, title, cwd) VALUES (?1, ?2, ?3)")
                .bind(r.get::<i64, _>("rowid"))
                .bind(r.get::<String, _>("title"))
                .bind(r.get::<String, _>("cwd"))
                .execute(&mut *tx)
                .await?;
        }
        let events_upto = lines.last().map_or(events_upto, |r| r.get("rowid"));
        let sessions_upto = sessions.last().map_or(sessions_upto, |r| r.get("rowid"));
        sqlx::query("UPDATE search_state SET events_upto = ?1, sessions_upto = ?2 WHERE id = 1")
            .bind(events_upto)
            .bind(sessions_upto)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(lines.len().max(sessions.len()))
    }

    /// A session's title as it now stands, into the index; after its title changed.
    pub async fn index_title(&self, node_id: &str, session_id: &str) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO search_title (rowid, title, cwd)
             SELECT rowid, title, cwd FROM sessions WHERE node_id = ?1 AND id = ?2",
        )
        .bind(node_id)
        .bind(session_id)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The sessions matching `query`, on one node or (empty) every node, that `filter`
    /// keeps, best first.
    pub async fn search(&self, query: &str, node_id: &str, limit: usize, now_ms: i64, filter: &crate::db::Filter) -> Result<Vec<Hit>> {
        let Some(q) = fts_query(query) else {
            return Ok(Vec::new());
        };
        // Each session's best line: bm25 is negative, lower is better.
        let mut best: HashMap<(String, String), (f64, u64, String)> = HashMap::new();
        let lines = sqlx::query(
            "SELECT e.node_id, e.session_id, e.seq, e.kind, e.payload, bm25(search_text) AS rank
             FROM search_text JOIN events e ON e.rowid = search_text.rowid
             WHERE search_text MATCH ?1 AND (?2 = '' OR e.node_id = ?2)
             ORDER BY rank LIMIT ?3",
        )
        .bind(&q)
        .bind(node_id)
        .bind(LINES)
        .fetch_all(&self.pool)
        .await?;
        for r in &lines {
            let mut key = (r.get::<String, _>("node_id"), r.get::<String, _>("session_id"));
            // A line a resumed or forked session copied from another is credited to the
            // session that wrote it, when the hub has it (phase item 15).
            let payload = r.get::<String, _>("payload");
            if let Some(origin) = serde_json::from_str::<Value>(&payload).ok().and_then(|v| v["sessionId"].as_str().map(str::to_string))
                && origin != key.1
                && self.session(&key.0, &origin).await?.is_some()
            {
                key.1 = origin;
            }
            let rank: f64 = r.get("rank");
            if best.get(&key).is_none_or(|b| rank < b.0) {
                let text = searchable(&r.get::<String, _>("kind"), &payload).unwrap_or_default();
                best.insert(key, (rank, r.get::<i64, _>("seq") as u64, snippet(&text, query)));
            }
        }
        let titles = sqlx::query(
            "SELECT s.*, bm25(search_title) AS rank
             FROM search_title JOIN sessions s ON s.rowid = search_title.rowid
             WHERE search_title MATCH ?1 AND (?2 = '' OR s.node_id = ?2)
             ORDER BY rank LIMIT ?3",
        )
        .bind(&q)
        .bind(node_id)
        .bind(LINES)
        .fetch_all(&self.pool)
        .await?;
        let mut sessions: HashMap<(String, String), Session> = HashMap::new();
        for r in &titles {
            let s = session_from(r);
            let key = (s.node_id.clone(), s.id.clone());
            let rank = r.get::<f64, _>("rank") * TITLE_WEIGHT;
            let e = best.entry(key.clone()).or_insert((rank, 0, String::new()));
            if rank < e.0 {
                e.0 = rank;
            }
            sessions.insert(key, s);
        }
        let mut ranked = Vec::new();
        for (key, (rank, seq, snippet)) in best {
            let session = match sessions.remove(&key) {
                Some(s) => s,
                None => match self.session(&key.0, &key.1).await? {
                    Some(s) => s,
                    None => continue,
                },
            };
            if !filter.matches(&session) {
                continue;
            }
            let last = session.ended_ms.unwrap_or(now_ms);
            ranked.push((rank * freshness(last, now_ms), Hit { session, snippet, seq }));
        }
        ranked.sort_by(|a, b| a.0.total_cmp(&b.0));
        Ok(ranked.into_iter().take(limit).map(|(_, h)| h).collect())
    }
}

/// Keeps the search index current, until the hub stops.
pub async fn run_index(db: Db) {
    loop {
        match db.index_search().await {
            Ok(n) if n as i64 >= BATCH => continue,
            Ok(_) => {}
            Err(e) => tracing::warn!(error = %format!("{e:#}"), "indexing for search failed"),
        }
        tokio::time::sleep(EVERY).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use saarthi_proto::event::v1::Event;

    #[test]
    fn queries_take_every_word_and_the_last_as_a_prefix() {
        assert_eq!(fts_query("Fix the pars").as_deref(), Some(r#""fix" "the" "pars"*"#));
        assert_eq!(fts_query("parser ").as_deref(), Some(r#""parser""#));
        assert_eq!(fts_query(r#"a"b OR c*"#).as_deref(), Some(r#""a" "b" "or" "c"*"#));
        assert_eq!(fts_query(" -- "), None);
    }

    #[test]
    fn a_snippet_is_cut_around_the_match() {
        let text = format!("{} the parser now handles tabs {}", "lead ".repeat(40), "tail ".repeat(40));
        let s = snippet(&text, "parser");
        assert!(s.starts_with('…') && s.ends_with('…') && s.contains("the parser now handles tabs"), "{s}");
        assert_eq!(snippet("short text", "nothing"), "short text");
    }

    #[test]
    fn only_prompts_and_replies_are_indexed() {
        assert_eq!(searchable("user", r#"{"message":{"content":"fix the parser"}}"#).as_deref(), Some("fix the parser"));
        let reply = r#"{"message":{"content":[{"type":"thinking","thinking":"x"},{"type":"text","text":"Done."}]}}"#;
        assert_eq!(searchable("assistant", reply).as_deref(), Some("Done."));
        let result = r#"{"message":{"content":[{"type":"tool_result","content":"secret file"}]}}"#;
        assert_eq!(searchable("user", result), None);
        assert_eq!(searchable("user", r#"{"isMeta":true,"message":{"content":"injected"}}"#), None);
        assert_eq!(searchable("system", r#"{"message":{"content":"x"}}"#), None);
    }

    fn line(seq: u64, session: &str, kind: &str, text: &str) -> Event {
        let payload = if kind == "user" {
            serde_json::json!({"type": "user", "uuid": format!("u{seq}"), "message": {"role": "user", "content": text}})
        } else {
            serde_json::json!({"type": "assistant", "uuid": format!("a{seq}"), "message": {"content": [{"type": "text", "text": text}]}})
        };
        Event {
            seq,
            session_id: session.into(),
            time_ms: 1,
            source: EventSource::Transcript.into(),
            kind: kind.into(),
            payload_json: payload.to_string(),
            line_id: format!("l{seq}"),
        }
    }

    #[tokio::test]
    async fn sessions_are_found_by_their_words_and_titles_best_and_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("hub.db")).await.unwrap();
        let (node, _) = db.register_node("n", "0", 1).await.unwrap();
        let now = 400 * 86_400_000;
        for (id, ended_days, title) in [("old", 300, ""), ("new", 399, ""), ("titled", 399, "Parser rework")] {
            let s = Session { id: id.into(), cwd: format!("/w/{id}"), started_ms: 1, ended_ms: Some(ended_days * 86_400_000), ..Default::default() };
            db.store_session(&node, &s).await.unwrap();
            if !title.is_empty() {
                db.set_session_meta(&node, id, title, "ai", "main", "").await.unwrap();
            }
        }
        for e in [
            line(1, "old", "user", "fix the parser for tabs"),
            line(2, "new", "user", "fix the parser for tabs"),
            line(3, "new", "assistant", "Fixed: the parser now handles tabs."),
            line(4, "titled", "user", "something else entirely"),
        ] {
            db.store_event(&node, &e).await.unwrap();
        }
        while db.index_search().await.unwrap() as i64 >= BATCH {}

        let all = crate::db::Filter::default();
        let hits = db.search("parse", &node, 10, now, &all).await.unwrap();
        let ids: Vec<&str> = hits.iter().map(|h| h.session.id.as_str()).collect();
        // The title counts five times; of the same words, the newer session first.
        assert_eq!(ids, ["titled", "new", "old"]);
        assert_eq!(hits[0].seq, 0);
        assert!(hits[1].snippet.contains("parser"), "{}", hits[1].snippet);
        assert_eq!(db.search("parser tabs", "", 10, now, &all).await.unwrap().len(), 2);
        assert!(db.search("nothing like it", "", 10, now, &all).await.unwrap().is_empty());
        // Only what the filter keeps: these were all started at their machine.
        let by = |l: &str| crate::db::Filter { launched_by: l.into(), ..Default::default() };
        assert_eq!(db.search("parser tabs", "", 10, now, &by("node:")).await.unwrap().len(), 2);
        assert!(db.search("parser tabs", "", 10, now, &by("client:")).await.unwrap().is_empty());
        // A resumed copy's lines from "new" are credited to "new", its own to itself.
        let copy = Session { id: "copy".into(), cwd: "/w/copy".into(), started_ms: 1, ended_ms: Some(399 * 86_400_000), ..Default::default() };
        db.store_session(&node, &copy).await.unwrap();
        let mut copied = line(6, "copy", "user", "fix the parser for tabs");
        copied.payload_json = copied.payload_json.replacen('{', r#"{"sessionId":"new","#, 1);
        let mut own = line(7, "copy", "user", "now the spaces too");
        own.payload_json = own.payload_json.replacen('{', r#"{"sessionId":"copy","#, 1);
        for e in [copied, own] {
            db.store_event(&node, &e).await.unwrap();
        }
        while db.index_search().await.unwrap() as i64 >= BATCH {}
        let ids: Vec<String> = db.search("parser tabs", "", 10, now, &all).await.unwrap().into_iter().map(|h| h.session.id).collect();
        assert!(!ids.contains(&"copy".to_string()), "{ids:?}");
        assert_eq!(db.search("spaces", "", 10, now, &all).await.unwrap()[0].session.id, "copy");
        // A title that changes is found by its new words.
        db.set_session_meta(&node, "old", "Tab handling", "ai", "main", "").await.unwrap();
        assert_eq!(db.search("handling", "", 10, now, &all).await.unwrap()[0].session.id, "old");
    }
}
