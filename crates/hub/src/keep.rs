//! Deleting sessions and dropping old transcripts (ADR-0065). A deleted session's id is
//! remembered, in the database and in memory since every stored event is checked against
//! it, so a node re-sending its lines cannot bring it back. Retention, when set, thins
//! sessions that ended long ago to what their History row and search need.

use std::time::Duration;

use anyhow::Result;
use saarthi_proto::event::v1::EventSource;
use sqlx::Row;

use crate::db::Db;

/// What became of a request to delete a session.
#[derive(Debug, PartialEq)]
pub enum Deleted {
    Done,
    /// It is still running: it has to be ended first.
    Live,
    Missing,
}

/// The first `user` lines a pruned session keeps, for its first prompt in History.
const OPENING: i64 = 20;

/// The events of session (?1, ?2) a pruning keeps: its latest usage and meta reports (source
/// ?3, the daemon) and its first ?5 `user` lines (source ?4, the transcript).
macro_rules! kept {
    () => {
        "SELECT rowid FROM (SELECT rowid FROM events WHERE node_id = ?1 AND session_id = ?2 AND source = ?3 AND kind = 'usage' ORDER BY seq DESC LIMIT 1) \
         UNION SELECT rowid FROM (SELECT rowid FROM events WHERE node_id = ?1 AND session_id = ?2 AND source = ?3 AND kind = 'meta' ORDER BY seq DESC LIMIT 1) \
         UNION SELECT rowid FROM (SELECT rowid FROM events WHERE node_id = ?1 AND session_id = ?2 AND source = ?4 AND kind = 'user' ORDER BY seq LIMIT ?5)"
    };
}

impl Db {
    pub fn is_deleted(&self, node_id: &str, session_id: &str) -> bool {
        self.deleted
            .read()
            .is_ok_and(|d| d.contains(&(node_id.to_string(), session_id.to_string())))
    }

    /// Deletes a session that has ended: its row, events, held requests and search entries,
    /// remembering its id.
    pub async fn delete_session(&self, node_id: &str, session_id: &str, now_ms: i64) -> Result<Deleted> {
        let ended: Option<Option<i64>> = sqlx::query_scalar("SELECT ended_ms FROM sessions WHERE node_id = ?1 AND id = ?2")
            .bind(node_id)
            .bind(session_id)
            .fetch_optional(&self.pool)
            .await?;
        match ended {
            None => return Ok(Deleted::Missing),
            Some(None) => return Ok(Deleted::Live),
            Some(Some(_)) => {}
        }
        let mut tx = self.pool.begin().await?;
        for sql in [
            "DELETE FROM search_text WHERE rowid IN (SELECT rowid FROM events WHERE node_id = ?1 AND session_id = ?2)",
            "DELETE FROM search_title WHERE rowid IN (SELECT rowid FROM sessions WHERE node_id = ?1 AND id = ?2)",
            "DELETE FROM events WHERE node_id = ?1 AND session_id = ?2",
            "DELETE FROM pending WHERE node_id = ?1 AND session_id = ?2",
            "DELETE FROM sessions WHERE node_id = ?1 AND id = ?2",
            "DELETE FROM launches WHERE node_id = ?1 AND session_id = ?2",
        ] {
            sqlx::query(sql).bind(node_id).bind(session_id).execute(&mut *tx).await?;
        }
        sqlx::query("INSERT OR REPLACE INTO deleted_sessions (node_id, session_id, deleted_ms) VALUES (?1, ?2, ?3)")
            .bind(node_id)
            .bind(session_id)
            .bind(now_ms)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        if let Ok(mut d) = self.deleted.write() {
            d.insert((node_id.to_string(), session_id.to_string()));
        }
        Ok(Deleted::Done)
    }

    /// Drops the events of sessions that ended before `ended_before_ms`, but for their latest
    /// usage and meta reports and their first `user` lines; how many it dropped.
    pub async fn prune(&self, ended_before_ms: i64) -> Result<u64> {
        let sessions = sqlx::query("SELECT node_id, id FROM sessions WHERE ended_ms IS NOT NULL AND ended_ms < ?1")
            .bind(ended_before_ms)
            .fetch_all(&self.pool)
            .await?;
        let (daemon, transcript) = (EventSource::Daemon.as_str_name(), EventSource::Transcript.as_str_name());
        let mut dropped = 0;
        for r in &sessions {
            let (node_id, id): (String, String) = (r.get("node_id"), r.get("id"));
            let mut tx = self.pool.begin().await?;
            sqlx::query(concat!(
                "DELETE FROM search_text WHERE rowid IN (SELECT rowid FROM events WHERE node_id = ?1 AND session_id = ?2 AND rowid NOT IN (",
                kept!(),
                "))"
            ))
            .bind(&node_id)
            .bind(&id)
            .bind(daemon)
            .bind(transcript)
            .bind(OPENING)
            .execute(&mut *tx)
            .await?;
            let done = sqlx::query(concat!(
                "DELETE FROM events WHERE node_id = ?1 AND session_id = ?2 AND rowid NOT IN (",
                kept!(),
                ")"
            ))
            .bind(&node_id)
            .bind(&id)
            .bind(daemon)
            .bind(transcript)
            .bind(OPENING)
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            dropped += done.rows_affected();
        }
        Ok(dropped)
    }
}

/// Drops old transcripts once an hour, keeping `days` of them, until the hub stops.
pub async fn run_retention(db: Db, days: u64) {
    let mut hourly = tokio::time::interval(Duration::from_secs(3600));
    loop {
        hourly.tick().await;
        let cutoff = crate::node_service::now_ms() - (days as i64) * 86_400_000;
        match db.prune(cutoff).await {
            Ok(0) => {}
            Ok(n) => tracing::info!(events = n, days, "dropped old transcripts"),
            Err(e) => tracing::warn!(error = %format!("{e:#}"), "dropping old transcripts failed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use saarthi_proto::event::v1::{Event, Session};

    fn event(seq: u64, session: &str, source: EventSource, kind: &str) -> Event {
        Event {
            seq,
            session_id: session.into(),
            time_ms: 1,
            source: source.into(),
            kind: kind.into(),
            payload_json: format!(r#"{{"type":"{kind}","message":{{"content":"line {seq}"}}}}"#),
            line_id: format!("l{seq}"),
        }
    }

    #[tokio::test]
    async fn a_deleted_session_stays_deleted_and_old_ones_are_thinned() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("hub.db")).await.unwrap();
        let (node, _) = db.register_node("n", "0", 1).await.unwrap();
        let session = |id: &str, ended: Option<i64>| Session { id: id.into(), cwd: "/w".into(), started_ms: 1, ended_ms: ended, ..Default::default() };
        db.store_session(&node, &session("gone", Some(10))).await.unwrap();
        db.store_session(&node, &session("live", None)).await.unwrap();
        db.store_event(&node, &event(1, "gone", EventSource::Transcript, "user")).await.unwrap();

        assert_eq!(db.delete_session(&node, "live", 20).await.unwrap(), Deleted::Live);
        assert_eq!(db.delete_session(&node, "nothing", 20).await.unwrap(), Deleted::Missing);
        assert_eq!(db.delete_session(&node, "gone", 20).await.unwrap(), Deleted::Done);
        assert!(db.session(&node, "gone").await.unwrap().is_none());
        assert!(db.session_events(&node, "gone", 0, 10).await.unwrap().is_empty());
        // What the node sends for it afterwards is dropped, also after the hub restarts.
        let db = Db::open(&dir.path().join("hub.db")).await.unwrap();
        assert!(db.is_deleted(&node, "gone"));
        assert!(!db.store_event(&node, &event(2, "gone", EventSource::Transcript, "user")).await.unwrap());
        db.store_session(&node, &session("gone", None)).await.unwrap();
        assert!(db.session(&node, "gone").await.unwrap().is_none());

        // Retention: an old session keeps its first prompts and its latest reports.
        db.store_session(&node, &session("old", Some(100))).await.unwrap();
        let mut seq = 10;
        for (source, kind) in [
            (EventSource::Transcript, "user"),
            (EventSource::Daemon, "usage"),
            (EventSource::Transcript, "assistant"),
            (EventSource::Hook, "Stop"),
            (EventSource::Daemon, "usage"),
            (EventSource::Daemon, "meta"),
        ] {
            seq += 1;
            db.store_event(&node, &event(seq, "old", source, kind)).await.unwrap();
        }
        assert_eq!(db.prune(50).await.unwrap(), 0, "ended after the cutoff: untouched");
        assert_eq!(db.prune(200).await.unwrap(), 3);
        let left: Vec<(u64, String)> = db.session_events(&node, "old", 0, 100).await.unwrap().into_iter().map(|e| (e.seq, e.kind)).collect();
        assert_eq!(left, [(11, "user".to_string()), (15, "usage".to_string()), (16, "meta".to_string())]);
        assert_eq!(db.prune(200).await.unwrap(), 0, "nothing more to drop");
        assert!(db.session(&node, "old").await.unwrap().is_some());
    }
}
