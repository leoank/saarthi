//! The hub's record: SQLite in WAL mode through sqlx, schema in `migrations/`. Queries are
//! checked at run time, so building needs no database.

use std::path::Path;

use anyhow::{Context, Result};
use saarthi_proto::control::v1::ClaudeToken;
use saarthi_proto::event::v1::{
    Event, EventSource, HealthSample, Launch, Metric, PendingKind, PendingRequest, ReleaseReason,
    Session, SessionStatus,
};
use sqlx::Row;
use sqlx::sqlite::{
    SqliteConnectOptions, SqliteJournalMode, SqlitePool, SqlitePoolOptions, SqliteRow,
};

/// A node as the hub records it.
#[derive(Clone, Debug, sqlx::FromRow)]
pub struct NodeRow {
    pub id: String,
    pub name: String,
    pub version: String,
    pub last_seen_ms: i64,
    /// The Claude token its spawned sessions run under when none is picked; empty for its
    /// own login.
    pub default_claude_token: String,
    /// The tailnet its link last came over, empty for plain TCP (ADR-0048).
    pub tailnet: String,
    /// Who that tailnet said the caller was: `host (login)`.
    pub tailnet_peer: String,
}

#[derive(Clone)]
pub struct Db {
    pub(crate) pool: SqlitePool,
    /// The sessions deleted from a client, by node and id (ADR-0065).
    pub(crate) deleted: std::sync::Arc<std::sync::RwLock<std::collections::HashSet<(String, String)>>>,
}

impl Db {
    pub async fn open(path: &Path) -> Result<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);
        let pool = SqlitePoolOptions::new()
            .connect_with(options)
            .await
            .with_context(|| format!("opening {}", path.display()))?;
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .context("migrating")?;
        let deleted: Vec<(String, String)> = sqlx::query_as("SELECT node_id, session_id FROM deleted_sessions")
            .fetch_all(&pool)
            .await?;
        let deleted = std::sync::Arc::new(std::sync::RwLock::new(deleted.into_iter().collect()));
        Ok(Self { pool, deleted })
    }

    /// The node's id, created on first registration, and the highest seq stored for it.
    pub async fn register_node(
        &self,
        name: &str,
        version: &str,
        now_ms: i64,
    ) -> Result<(String, u64)> {
        let id: String = sqlx::query_scalar(
            "INSERT INTO nodes (id, name, version, first_seen_ms, last_seen_ms) VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT (name) DO UPDATE SET version = excluded.version, last_seen_ms = excluded.last_seen_ms
             RETURNING id",
        )
        .bind(uuid::Uuid::new_v4().simple().to_string())
        .bind(name)
        .bind(version)
        .bind(now_ms)
        .fetch_one(&self.pool)
        .await?;
        let last: Option<i64> =
            sqlx::query_scalar("SELECT MAX(seq) FROM events WHERE node_id = ?1")
                .bind(&id)
                .fetch_one(&self.pool)
                .await?;
        Ok((id, last.unwrap_or(0) as u64))
    }

    pub async fn touch_node(&self, node_id: &str, now_ms: i64) -> Result<()> {
        sqlx::query("UPDATE nodes SET last_seen_ms = ?2 WHERE id = ?1")
            .bind(node_id)
            .bind(now_ms)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Stores an event unless the node sent it before: the same seq, or the same line of
    /// the session under another seq. True when it was new.
    pub async fn store_event(&self, node_id: &str, e: &Event) -> Result<bool> {
        if self.is_deleted(node_id, &e.session_id) {
            return Ok(false);
        }
        let done = sqlx::query(
            "INSERT INTO events (node_id, seq, session_id, time_ms, source, kind, payload, line_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) ON CONFLICT DO NOTHING",
        )
        .bind(node_id)
        .bind(e.seq as i64)
        .bind(&e.session_id)
        .bind(e.time_ms)
        .bind(e.source().as_str_name())
        .bind(&e.kind)
        .bind(&e.payload_json)
        .bind((!e.line_id.is_empty()).then_some(&e.line_id))
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() == 1)
    }

    /// A session's stored events after a seq, oldest first, at most `limit`.
    pub async fn session_events(
        &self,
        node_id: &str,
        session_id: &str,
        after_seq: u64,
        limit: i64,
    ) -> Result<Vec<Event>> {
        let rows = sqlx::query(
            "SELECT seq, session_id, time_ms, source, kind, payload, line_id FROM events
             WHERE node_id = ?1 AND session_id = ?2 AND seq > ?3 ORDER BY seq LIMIT ?4",
        )
        .bind(node_id)
        .bind(session_id)
        .bind(after_seq as i64)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(event_from).collect())
    }

    /// How much the hub holds of a session: events, their bytes, transcript lines, and the
    /// times of the first and last.
    pub async fn session_about(&self, node_id: &str, session_id: &str) -> Result<(u64, u64, u64, i64, i64)> {
        let r = sqlx::query(
            "SELECT count(*) AS n, coalesce(sum(length(payload)), 0) AS bytes,
                    coalesce(sum(source = ?3), 0) AS lines,
                    coalesce(min(time_ms), 0) AS first, coalesce(max(time_ms), 0) AS last
             FROM events WHERE node_id = ?1 AND session_id = ?2",
        )
        .bind(node_id)
        .bind(session_id)
        .bind(EventSource::Transcript.as_str_name())
        .fetch_one(&self.pool)
        .await?;
        Ok((
            r.get::<i64, _>("n") as u64,
            r.get::<i64, _>("bytes") as u64,
            r.get::<i64, _>("lines") as u64,
            r.get("first"),
            r.get("last"),
        ))
    }

    /// The uuid of the last transcript line the hub holds of a session, empty when none.
    pub async fn last_line_id(&self, node_id: &str, session_id: &str) -> Result<String> {
        let id: Option<String> = sqlx::query_scalar(
            "SELECT line_id FROM events WHERE node_id = ?1 AND session_id = ?2 AND source = ?3 AND line_id IS NOT NULL
             ORDER BY seq DESC LIMIT 1",
        )
        .bind(node_id)
        .bind(session_id)
        .bind(EventSource::Transcript.as_str_name())
        .fetch_optional(&self.pool)
        .await?;
        Ok(id.unwrap_or_default())
    }

    /// Where a page of a session's record starts that ends before `before_seq` (0: at its
    /// end) and holds `lines` transcript lines: the seq of the first of them, or 0 when
    /// there are no more than that, so the page is the whole rest.
    pub async fn page_start(&self, node_id: &str, session_id: &str, before_seq: u64, lines: u32) -> Result<u64> {
        let before = if before_seq == 0 { i64::MAX } else { before_seq as i64 };
        let start: Option<i64> = sqlx::query_scalar(
            "SELECT seq FROM events
             WHERE node_id = ?1 AND session_id = ?2 AND source = ?3 AND seq < ?4
             ORDER BY seq DESC LIMIT 1 OFFSET ?5",
        )
        .bind(node_id)
        .bind(session_id)
        .bind(EventSource::Transcript.as_str_name())
        .bind(before)
        .bind(i64::from(lines.max(1)) - 1)
        .fetch_optional(&self.pool)
        .await?;
        Ok(start.unwrap_or(0) as u64)
    }

    /// The node's latest reports on a session from before `seq` that a page starting there
    /// still needs: its usage totals, its context and its queue, oldest first.
    pub async fn page_seeds(&self, node_id: &str, session_id: &str, seq: u64) -> Result<Vec<Event>> {
        let mut seeds = Vec::new();
        for kind in ["usage", "context", "queue"] {
            let row = sqlx::query(
                "SELECT seq, session_id, time_ms, source, kind, payload, line_id FROM events
                 WHERE node_id = ?1 AND session_id = ?2 AND source = ?3 AND kind = ?4 AND seq < ?5
                 ORDER BY seq DESC LIMIT 1",
            )
            .bind(node_id)
            .bind(session_id)
            .bind(EventSource::Daemon.as_str_name())
            .bind(kind)
            .bind(seq as i64)
            .fetch_optional(&self.pool)
            .await?;
            seeds.extend(row.as_ref().map(event_from));
        }
        seeds.sort_by_key(|e| e.seq);
        Ok(seeds)
    }

    /// A node's health samples since a time, oldest first.
    pub async fn health_since(&self, node_id: &str, since_ms: i64) -> Result<Vec<HealthSample>> {
        let rows: Vec<(i64, String, f64)> = sqlx::query_as(
            "SELECT time_ms, metric, value FROM health WHERE node_id = ?1 AND time_ms >= ?2
             ORDER BY time_ms, metric",
        )
        .bind(node_id)
        .bind(since_ms)
        .fetch_all(&self.pool)
        .await?;
        let mut samples: Vec<HealthSample> = Vec::new();
        for (time_ms, name, value) in rows {
            if samples.last().is_none_or(|s| s.time_ms != time_ms) {
                samples.push(HealthSample {
                    time_ms,
                    metrics: Vec::new(),
                });
            }
            let last = samples.last_mut().expect("pushed above");
            last.metrics.push(Metric { name, value });
        }
        Ok(samples)
    }

    /// Stores a session; true when this update ends a session the hub had as live, which
    /// is when a person is told it ended (ADR-0029).
    pub async fn store_session(&self, node_id: &str, s: &Session) -> Result<bool> {
        if self.is_deleted(node_id, &s.id) {
            return Ok(false);
        }
        let was_live: Option<(Option<i64>,)> =
            sqlx::query_as("SELECT ended_ms FROM sessions WHERE node_id = ?1 AND id = ?2")
                .bind(node_id)
                .bind(&s.id)
                .fetch_optional(&self.pool)
                .await?;
        let just_ended = s.ended_ms.is_some() && matches!(was_live, Some((None,)));
        sqlx::query(
            "INSERT INTO sessions (node_id, id, harness, cwd, transcript_path, launch, pid, account_label, status,
                                   started_ms, ended_ms, profile, claude_token, launched_by, labels)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
                     COALESCE((SELECT launched_by FROM launches WHERE node_id = ?1 AND session_id = ?2),
                              'node:' || COALESCE((SELECT name FROM nodes WHERE id = ?1), '')),
                     COALESCE((SELECT labels FROM launches WHERE node_id = ?1 AND session_id = ?2), '{}'))
             ON CONFLICT (node_id, id) DO UPDATE SET
               harness = excluded.harness, cwd = excluded.cwd, transcript_path = excluded.transcript_path,
               launch = excluded.launch, pid = excluded.pid, account_label = excluded.account_label,
               status = excluded.status, started_ms = excluded.started_ms, ended_ms = excluded.ended_ms,
               profile = excluded.profile,
               claude_token = CASE WHEN excluded.claude_token <> '' THEN excluded.claude_token
                                   ELSE claude_token END",
            // claude_token is set by `set_session_claude_token` when the hub starts or resumes
            // the session, or reported by the node for one started at its terminal under a
            // token (ADR-0046); kept when a restarted node reports the session without one.
            // launched_by is the hub's alone (ADR-0082): set once, from `record_launch` or as
            // the node's, whatever the node reports; so are the labels, set from the launch.
        )
        .bind(node_id)
        .bind(&s.id)
        .bind(&s.harness)
        .bind(&s.cwd)
        .bind(&s.transcript_path)
        .bind(s.launch().as_str_name())
        .bind(s.pid as i64)
        .bind(&s.account_label)
        .bind(s.status().as_str_name())
        .bind(s.started_ms)
        .bind(s.ended_ms)
        .bind(&s.profile)
        .bind(&s.claude_token)
        .execute(&self.pool)
        .await?;
        Ok(just_ended)
    }

    pub async fn store_health(&self, node_id: &str, h: &HealthSample) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for m in &h.metrics {
            sqlx::query(
                "INSERT INTO health (node_id, time_ms, metric, value) VALUES (?1, ?2, ?3, ?4)",
            )
            .bind(node_id)
            .bind(h.time_ms)
            .bind(&m.name)
            .bind(m.value)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn store_pending(&self, node_id: &str, p: &PendingRequest) -> Result<()> {
        if self.is_deleted(node_id, &p.session_id) {
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO pending (node_id, id, session_id, kind, tool_name, tool_input, created_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7) ON CONFLICT DO NOTHING",
        )
        .bind(node_id)
        .bind(&p.id)
        .bind(&p.session_id)
        .bind(p.kind().as_str_name())
        .bind(&p.tool_name)
        .bind(&p.tool_input_json)
        .bind(p.created_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn close_pending(
        &self,
        node_id: &str,
        id: &str,
        reason: ReleaseReason,
        now_ms: i64,
    ) -> Result<()> {
        sqlx::query(
            "UPDATE pending SET closed_ms = ?3, reason = ?4
             WHERE node_id = ?1 AND id = ?2 AND closed_ms IS NULL",
        )
        .bind(node_id)
        .bind(id)
        .bind(now_ms)
        .bind(reason.as_str_name())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// After a (re)connect the node's list of held requests is the truth: stores the ones the
    /// hub missed and closes the ones the node no longer holds, returning those ids. How they
    /// ended is unknown.
    pub async fn sync_pending(
        &self,
        node_id: &str,
        held: &[PendingRequest],
        now_ms: i64,
    ) -> Result<Vec<String>> {
        for p in held {
            self.store_pending(node_id, p).await?;
        }
        let open: Vec<String> =
            sqlx::query_scalar("SELECT id FROM pending WHERE node_id = ?1 AND closed_ms IS NULL")
                .bind(node_id)
                .fetch_all(&self.pool)
                .await?;
        let gone: Vec<String> = open
            .into_iter()
            .filter(|id| !held.iter().any(|p| &p.id == id))
            .collect();
        for id in &gone {
            self.close_pending(node_id, id, ReleaseReason::Unspecified, now_ms)
                .await?;
        }
        Ok(gone)
    }

    pub async fn record_resolution(&self, node_id: &str, id: &str, json: &str) -> Result<()> {
        sqlx::query("UPDATE pending SET resolution = ?3 WHERE node_id = ?1 AND id = ?2")
            .bind(node_id)
            .bind(id)
            .bind(json)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// When the turn ending at event `seq` began: the session's last prompt submitted before it.
    pub async fn turn_started(&self, node_id: &str, session_id: &str, seq: u64) -> Result<Option<i64>> {
        Ok(sqlx::query_scalar(
            "SELECT time_ms FROM events
             WHERE node_id = ?1 AND session_id = ?2 AND source = ?3 AND kind = 'UserPromptSubmit' AND seq < ?4
             ORDER BY seq DESC LIMIT 1",
        )
        .bind(node_id)
        .bind(session_id)
        .bind(EventSource::Hook.as_str_name())
        .bind(seq as i64)
        .fetch_optional(&self.pool)
        .await?)
    }

    /// The open request `id` on a node, if there is one.
    pub async fn open_pending(&self, node_id: &str, id: &str) -> Result<Option<PendingRequest>> {
        let row = sqlx::query(
            "SELECT id, session_id, kind, tool_name, tool_input, created_ms FROM pending
             WHERE node_id = ?1 AND id = ?2 AND closed_ms IS NULL",
        )
        .bind(node_id)
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| pending_from_row(&r)))
    }

    /// Every open request, oldest first, with its node id.
    pub async fn open_pendings(&self) -> Result<Vec<(String, PendingRequest)>> {
        let rows = sqlx::query(
            "SELECT node_id, id, session_id, kind, tool_name, tool_input, created_ms FROM pending
             WHERE closed_ms IS NULL ORDER BY created_ms",
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| (r.get("node_id"), pending_from_row(r)))
            .collect())
    }

    /// Every node's record.
    pub async fn nodes(&self) -> Result<Vec<NodeRow>> {
        Ok(sqlx::query_as(
            "SELECT id, name, version, last_seen_ms, default_claude_token, tailnet, tailnet_peer
             FROM nodes ORDER BY name",
        )
            .fetch_all(&self.pool)
            .await?)
    }

    /// One node's record.
    pub async fn node(&self, node_id: &str) -> Result<Option<NodeRow>> {
        Ok(sqlx::query_as(
            "SELECT id, name, version, last_seen_ms, default_claude_token, tailnet, tailnet_peer
             FROM nodes WHERE id = ?1",
        )
            .bind(node_id)
            .fetch_optional(&self.pool)
            .await?)
    }

    /// Records the tailnet a node's link came over and who the tailnet says the caller is
    /// (ADR-0048); both empty for a link over plain TCP. Recorded, not checked.
    pub async fn set_node_tailnet(&self, node_id: &str, tailnet: &str, peer: &str) -> Result<()> {
        sqlx::query("UPDATE nodes SET tailnet = ?2, tailnet_peer = ?3 WHERE id = ?1")
            .bind(node_id)
            .bind(tailnet)
            .bind(peer)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// The Claude token a node's spawned sessions run under when none is picked; empty for
    /// its own login.
    /// The id of the node registered under `name`, if it ever registered.
    pub async fn node_id(&self, name: &str) -> Result<Option<String>> {
        Ok(sqlx::query_scalar("SELECT id FROM nodes WHERE name = ?1")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?)
    }

    pub async fn default_claude_token(&self, node_id: &str) -> Result<Option<String>> {
        Ok(
            sqlx::query_scalar("SELECT default_claude_token FROM nodes WHERE id = ?1")
                .bind(node_id)
                .fetch_optional(&self.pool)
                .await?,
        )
    }

    /// Sets a node's default Claude token; false when there is no such node.
    pub async fn set_default_claude_token(&self, node_id: &str, name: &str) -> Result<bool> {
        let done = sqlx::query("UPDATE nodes SET default_claude_token = ?2 WHERE id = ?1")
            .bind(node_id)
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() == 1)
    }

    /// Records who launched session `id` on the node (ADR-0082), and its labels, before the
    /// node is asked to start it; a session the node reported already, as its own, takes
    /// them too.
    pub async fn record_launch(
        &self,
        node_id: &str,
        id: &str,
        launched_by: &str,
        labels: &std::collections::HashMap<String, String>,
    ) -> Result<()> {
        let labels = serde_json::to_string(labels)?;
        sqlx::query("INSERT OR REPLACE INTO launches (node_id, session_id, launched_by, labels) VALUES (?1, ?2, ?3, ?4)")
            .bind(node_id)
            .bind(id)
            .bind(launched_by)
            .bind(&labels)
            .execute(&self.pool)
            .await?;
        sqlx::query(
            "UPDATE sessions SET launched_by = ?3, labels = ?4
             WHERE node_id = ?1 AND id = ?2 AND launched_by LIKE 'node:%'",
        )
        .bind(node_id)
        .bind(id)
        .bind(launched_by)
        .bind(&labels)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// The stored policies (phase item 8), by id: id, text, when and by whom last set.
    pub async fn policies(&self) -> Result<Vec<(String, String, i64, String)>> {
        let rows = sqlx::query("SELECT id, text, updated_ms, updated_by FROM policies ORDER BY id")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(|r| (r.get("id"), r.get("text"), r.get("updated_ms"), r.get("updated_by"))).collect())
    }

    pub async fn set_policy(&self, id: &str, text: &str, now_ms: i64, by: &str) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO policies (id, text, updated_ms, updated_by) VALUES (?1, ?2, ?3, ?4)")
            .bind(id)
            .bind(text)
            .bind(now_ms)
            .bind(by)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Whether there was one to delete.
    pub async fn delete_policy(&self, id: &str) -> Result<bool> {
        let done = sqlx::query("DELETE FROM policies WHERE id = ?1").bind(id).execute(&self.pool).await?;
        Ok(done.rows_affected() > 0)
    }

    /// Records the token the hub started or resumed a session under.
    pub async fn set_session_claude_token(
        &self,
        node_id: &str,
        id: &str,
        name: &str,
    ) -> Result<()> {
        sqlx::query("UPDATE sessions SET claude_token = ?3 WHERE node_id = ?1 AND id = ?2")
            .bind(node_id)
            .bind(id)
            .bind(name)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Brings the token record up to the file the hub read at start: a new name is first
    /// seen now; a known name whose fingerprint changed is a renewed token, first seen now
    /// with its hand-set expiry, reminder and usage cleared. Names no longer in the file
    /// keep their rows.
    pub async fn sync_claude_tokens(
        &self,
        fingerprints: &[(String, String)],
        now_ms: i64,
    ) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        for (name, fp) in fingerprints {
            sqlx::query(
                "INSERT INTO claude_tokens (name, fingerprint, first_seen_ms) VALUES (?1, ?2, ?3)
                 ON CONFLICT (name) DO UPDATE SET
                   fingerprint = excluded.fingerprint, first_seen_ms = excluded.first_seen_ms,
                   expires_ms = NULL, reminded_ms = NULL, usage_ms = NULL,
                   five_hour_pct = NULL, five_hour_resets_s = NULL,
                   seven_day_pct = NULL, seven_day_resets_s = NULL
                 WHERE claude_tokens.fingerprint <> excluded.fingerprint",
            )
            .bind(name)
            .bind(fp)
            .bind(now_ms)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }

    /// The named tokens' records, in the order given (the names the hub holds now).
    pub async fn claude_tokens(&self, names: &[&str]) -> Result<Vec<ClaudeToken>> {
        let mut out = Vec::new();
        for name in names {
            let Some(r) = sqlx::query("SELECT * FROM claude_tokens WHERE name = ?1")
                .bind(name)
                .fetch_optional(&self.pool)
                .await?
            else {
                continue;
            };
            let first_seen_ms: i64 = r.get("first_seen_ms");
            let set: Option<i64> = r.get("expires_ms");
            out.push(ClaudeToken {
                name: r.get("name"),
                first_seen_ms,
                expires_ms: set.unwrap_or(first_seen_ms + crate::claude_tokens::LIFETIME_MS),
                expiry_set: set.is_some(),
                usage_ms: r.get::<Option<i64>, _>("usage_ms").unwrap_or(0),
                five_hour_pct: r.get("five_hour_pct"),
                five_hour_resets_s: r.get("five_hour_resets_s"),
                seven_day_pct: r.get("seven_day_pct"),
                seven_day_resets_s: r.get("seven_day_resets_s"),
                default_model: r.get("default_model"),
            });
        }
        Ok(out)
    }

    /// Sets a token's default model (ADR-0079), empty to clear; false when the hub has no
    /// such token.
    pub async fn set_claude_token_model(&self, name: &str, model: &str) -> Result<bool> {
        let done = sqlx::query("UPDATE claude_tokens SET default_model = ?2 WHERE name = ?1")
            .bind(name)
            .bind(model)
            .execute(&self.pool)
            .await?;
        Ok(done.rows_affected() == 1)
    }

    /// A token's default model; empty when it has none or the hub has no record of it.
    pub async fn claude_token_model(&self, name: &str) -> Result<String> {
        let model: Option<String> = sqlx::query_scalar("SELECT default_model FROM claude_tokens WHERE name = ?1")
            .bind(name)
            .fetch_optional(&self.pool)
            .await?;
        Ok(model.unwrap_or_default())
    }

    /// Sets (or with None clears) a token's expiry by hand, and lets its reminder go out
    /// again for the new date; false when the hub has no such token.
    pub async fn set_claude_token_expiry(
        &self,
        name: &str,
        expires_ms: Option<i64>,
    ) -> Result<bool> {
        let done = sqlx::query(
            "UPDATE claude_tokens SET expires_ms = ?2, reminded_ms = NULL WHERE name = ?1",
        )
        .bind(name)
        .bind(expires_ms)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() == 1)
    }

    /// Claims the expiry reminder of a token due one by `now_ms`: true once, when it is
    /// within `REMIND_MS` of expiring and has not been reminded for this date.
    pub async fn claim_expiry_reminder(&self, name: &str, now_ms: i64) -> Result<bool> {
        let done = sqlx::query(
            "UPDATE claude_tokens SET reminded_ms = ?2
             WHERE name = ?1 AND reminded_ms IS NULL
               AND coalesce(expires_ms, first_seen_ms + ?3) - ?4 <= ?2",
        )
        .bind(name)
        .bind(now_ms)
        .bind(crate::claude_tokens::LIFETIME_MS)
        .bind(crate::claude_tokens::REMIND_MS)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() == 1)
    }

    /// Records the windows a session under a token reported; older reports than the one
    /// stored are ignored.
    pub async fn record_claude_token_usage(&self, name: &str, w: &Windows) -> Result<()> {
        sqlx::query(
            "UPDATE claude_tokens SET usage_ms = ?2, five_hour_pct = ?3, five_hour_resets_s = ?4,
               seven_day_pct = ?5, seven_day_resets_s = ?6
             WHERE name = ?1 AND coalesce(usage_ms, 0) <= ?2",
        )
        .bind(name)
        .bind(w.time_ms)
        .bind(w.five_hour_pct)
        .bind(w.five_hour_resets_s)
        .bind(w.seven_day_pct)
        .bind(w.seven_day_resets_s)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Sessions of one node, or of every node when `node_id` is empty, that `filter` keeps.
    pub async fn sessions(&self, node_id: &str, include_ended: bool, filter: &Filter) -> Result<Vec<Session>> {
        let rows = sqlx::query(concat!(
            "SELECT * FROM sessions s WHERE (?1 = '' OR node_id = ?1) AND (?2 OR ended_ms IS NULL) AND ",
            filter_sql!("3", "4"),
            " ORDER BY started_ms",
        ))
        .bind(node_id)
        .bind(include_ended)
        .bind(&filter.launched_by)
        .bind(serde_json::to_string(&filter.labels)?)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(session_from).collect())
    }

    /// Sessions started before `before_ms`, newest first, at most `limit`: only those with a
    /// transcript line, since one without (a tool's throwaway run that kept none) has no
    /// conversation to show or resume.
    pub async fn history(&self, node_id: &str, before_ms: i64, limit: i64, filter: &Filter) -> Result<Vec<Session>> {
        let rows = sqlx::query(concat!(
            "SELECT * FROM sessions s WHERE (?1 = '' OR node_id = ?1) AND started_ms < ?2
               AND EXISTS (SELECT 1 FROM events e WHERE e.node_id = s.node_id
                           AND e.session_id = s.id AND e.source = ?4) AND ",
            filter_sql!("5", "6"),
            " ORDER BY started_ms DESC LIMIT ?3",
        ))
        .bind(node_id)
        .bind(before_ms)
        .bind(limit)
        .bind(EventSource::Transcript.as_str_name())
        .bind(&filter.launched_by)
        .bind(serde_json::to_string(&filter.labels)?)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(session_from).collect())
    }

    /// One session as stored.
    pub async fn session(&self, node_id: &str, id: &str) -> Result<Option<Session>> {
        let row = sqlx::query("SELECT * FROM sessions WHERE node_id = ?1 AND id = ?2")
            .bind(node_id)
            .bind(id)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.as_ref().map(session_from))
    }

    /// A session's first `n` transcript `user` lines that may hold its first prompt: not a
    /// tool's result, not text injected for the model (`isMeta`), and no longer than
    /// `max_bytes`. Tool results are `user` lines too and can run to megabytes (a
    /// screenshot, a whole file), which once made a page of history too big for the app.
    pub async fn opening(&self, node_id: &str, session_id: &str, n: i64, max_bytes: i64) -> Result<Vec<Event>> {
        let rows = sqlx::query(
            "SELECT seq, session_id, time_ms, source, kind, payload, line_id FROM events
             WHERE node_id = ?1 AND session_id = ?2 AND source = ?3 AND kind = 'user'
               AND length(payload) <= ?5
               AND CASE WHEN json_valid(payload) THEN
                     coalesce(json_extract(payload, '$.isMeta'), 0) = 0
                     AND coalesce(json_extract(payload, '$.message.content[0].type'), '') <> 'tool_result'
                   ELSE 0 END
             ORDER BY seq LIMIT ?4",
        )
        .bind(node_id)
        .bind(session_id)
        .bind(EventSource::Transcript.as_str_name())
        .bind(n)
        .bind(max_bytes)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(event_from).collect())
    }

    /// The opening of Claude's latest reply in a session, from its latest assistant line that
    /// has text; empty when there is none.
    pub async fn last_reply(&self, node_id: &str, session_id: &str) -> Result<String> {
        const PREVIEW: usize = 280;
        let payload: Option<String> = sqlx::query_scalar(
            "SELECT payload FROM events
             WHERE node_id = ?1 AND session_id = ?2 AND source = ?3 AND kind = 'assistant'
               AND payload LIKE '%\"type\":\"text\"%'
             ORDER BY seq DESC LIMIT 1",
        )
        .bind(node_id)
        .bind(session_id)
        .bind(EventSource::Transcript.as_str_name())
        .fetch_optional(&self.pool)
        .await?;
        let v: serde_json::Value = payload.and_then(|p| serde_json::from_str(&p).ok()).unwrap_or_default();
        let text: String = v["message"]["content"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|b| b["type"] == "text")
            .filter_map(|b| b["text"].as_str())
            .collect::<Vec<_>>()
            .join(" ");
        let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
        Ok(match flat.char_indices().nth(PREVIEW) {
            Some((at, _)) => format!("{}…", &flat[..at]),
            None => flat,
        })
    }

    /// A session's title, where it came from and its git branch, from a `meta` event; the
    /// session as it now stands, if the hub has it.
    pub async fn set_session_meta(
        &self,
        node_id: &str,
        session_id: &str,
        title: &str,
        source: &str,
        branch: &str,
        parent_id: &str,
    ) -> Result<Option<Session>> {
        sqlx::query("UPDATE sessions SET title = ?3, title_source = ?4, git_branch = ?5, parent_id = ?6 WHERE node_id = ?1 AND id = ?2")
            .bind(node_id)
            .bind(session_id)
            .bind(title)
            .bind(source)
            .bind(branch)
            .bind(parent_id)
            .execute(&self.pool)
            .await?;
        self.index_title(node_id, session_id).await?;
        self.session(node_id, session_id).await
    }

    /// A page of a session's transcript lines as the node sent them, after `after_seq`, in
    /// order: each with its seq, to ask for the next page from.
    pub async fn transcript(
        &self,
        node_id: &str,
        session_id: &str,
        after_seq: i64,
        limit: i64,
    ) -> Result<Vec<(i64, String)>> {
        let rows: Vec<(i64, String)> = sqlx::query_as(
            "SELECT seq, payload FROM events
             WHERE node_id = ?1 AND session_id = ?2 AND source = ?3 AND seq > ?4
             ORDER BY seq LIMIT ?5",
        )
        .bind(node_id)
        .bind(session_id)
        .bind(EventSource::Transcript.as_str_name())
        .bind(after_seq)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows)
    }

    /// The payload of a session's latest daemon `usage` event: its totals by model.
    pub async fn last_usage(&self, node_id: &str, session_id: &str) -> Result<Option<String>> {
        let row = sqlx::query(
            "SELECT payload FROM events
             WHERE node_id = ?1 AND session_id = ?2 AND source = ?3 AND kind = 'usage'
             ORDER BY seq DESC LIMIT 1",
        )
        .bind(node_id)
        .bind(session_id)
        .bind(EventSource::Daemon.as_str_name())
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|r| r.get("payload")))
    }

    pub async fn add_push_endpoint(&self, client: &str, endpoint: &str, now_ms: i64) -> Result<()> {
        sqlx::query(
            "INSERT INTO push_endpoints (endpoint, client, created_ms) VALUES (?1, ?2, ?3)
             ON CONFLICT (endpoint) DO UPDATE SET client = excluded.client",
        )
        .bind(endpoint)
        .bind(client)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn remove_push_endpoint(&self, endpoint: &str) -> Result<()> {
        sqlx::query("DELETE FROM push_endpoints WHERE endpoint = ?1")
            .bind(endpoint)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Records a usage alert as sent; false if it already was (ADR-0032).
    pub async fn claim_usage_alert(&self, key: &str, node_id: &str, now_ms: i64) -> Result<bool> {
        let done = sqlx::query(
            "INSERT INTO usage_alerts (key, node_id, created_ms) VALUES (?1, ?2, ?3) ON CONFLICT DO NOTHING",
        )
        .bind(key)
        .bind(node_id)
        .bind(now_ms)
        .execute(&self.pool)
        .await?;
        Ok(done.rows_affected() == 1)
    }

    pub async fn push_endpoints(&self) -> Result<Vec<String>> {
        Ok(
            sqlx::query_scalar("SELECT endpoint FROM push_endpoints ORDER BY created_ms")
                .fetch_all(&self.pool)
                .await?,
        )
    }

    /// Every push endpoint with the client that registered it.
    pub async fn push_endpoints_by_client(&self) -> Result<Vec<(String, String)>> {
        Ok(
            sqlx::query_as("SELECT endpoint, client FROM push_endpoints ORDER BY created_ms")
                .fetch_all(&self.pool)
                .await?,
        )
    }

    /// Every stored seq for a node, in order.
    pub async fn event_seqs(&self, node_name: &str) -> Result<Vec<u64>> {
        let seqs: Vec<i64> = sqlx::query_scalar(
            "SELECT seq FROM events JOIN nodes ON nodes.id = events.node_id WHERE nodes.name = ?1 ORDER BY seq",
        )
        .bind(node_name)
        .fetch_all(&self.pool)
        .await?;
        Ok(seqs.into_iter().map(|s| s as u64).collect())
    }

    /// Session ids stored for a node.
    pub async fn session_ids(&self, node_name: &str) -> Result<Vec<String>> {
        Ok(sqlx::query_scalar(
            "SELECT sessions.id FROM sessions JOIN nodes ON nodes.id = sessions.node_id WHERE nodes.name = ?1 ORDER BY 1",
        )
        .bind(node_name)
        .fetch_all(&self.pool)
        .await?)
    }
}

fn pending_from_row(r: &SqliteRow) -> PendingRequest {
    PendingRequest {
        id: r.get("id"),
        session_id: r.get("session_id"),
        kind: PendingKind::from_str_name(r.get("kind"))
            .unwrap_or_default()
            .into(),
        tool_name: r.get("tool_name"),
        tool_input_json: r.get("tool_input"),
        created_ms: r.get("created_ms"),
    }
}

fn event_from(r: &sqlx::sqlite::SqliteRow) -> Event {
    Event {
        session_id: r.get("session_id"),
        seq: r.get::<i64, _>("seq") as u64,
        time_ms: r.get("time_ms"),
        source: EventSource::from_str_name(r.get("source"))
            .unwrap_or_default()
            .into(),
        kind: r.get("kind"),
        payload_json: r.get("payload"),
        line_id: r.get::<Option<String>, _>("line_id").unwrap_or_default(),
    }
}

/// [`Filter`] as SQL, its launcher bound to `?$by` and its labels, as a JSON object, to
/// `?$labels`. A launcher ending in `:` matches by prefix, compared with `substr` so a name
/// holding `%` or `_` stays literal.
macro_rules! filter_sql {
    ($by:literal, $labels:literal) => {
        concat!(
            "(?", $by, " = '' OR s.launched_by = ?", $by,
            " OR (substr(?", $by, ", -1) = ':' AND substr(s.launched_by, 1, length(?", $by, ")) = ?", $by, "))",
            " AND NOT EXISTS (SELECT 1 FROM json_each(?", $labels, ") w",
            " WHERE json_extract(s.labels, '$.\"' || w.key || '\"') IS NOT w.value)"
        )
    };
}
use filter_sql;

/// Which sessions a list, the history, a search or a watch keeps (phase PHS-20261126-f3e51h
/// item 6): those `launched_by` names, exactly or, ending in `:`, every launcher of that kind;
/// and those with every one of `labels`. The default keeps all.
#[derive(Debug, Default, Clone)]
pub struct Filter {
    pub launched_by: String,
    pub labels: std::collections::HashMap<String, String>,
}

impl Filter {
    pub fn matches(&self, s: &Session) -> bool {
        let by = &self.launched_by;
        let launcher = by.is_empty()
            || if by.ends_with(':') { s.launched_by.starts_with(by.as_str()) } else { s.launched_by == *by };
        launcher && self.labels.iter().all(|(k, v)| s.labels.get(k) == Some(v))
    }
}


pub(crate) fn session_from(r: &sqlx::sqlite::SqliteRow) -> Session {
    Session {
        id: r.get("id"),
        node_id: r.get("node_id"),
        harness: r.get("harness"),
        cwd: r.get("cwd"),
        transcript_path: r.get("transcript_path"),
        launch: Launch::from_str_name(r.get("launch"))
            .unwrap_or_default()
            .into(),
        pid: r.get::<i64, _>("pid") as u32,
        account_label: r.get("account_label"),
        started_ms: r.get("started_ms"),
        ended_ms: r.get("ended_ms"),
        status: SessionStatus::from_str_name(r.get("status"))
            .unwrap_or_default()
            .into(),
        profile: r.get("profile"),
        claude_token: r.get("claude_token"),
        title: r.get("title"),
        title_source: r.get("title_source"),
        git_branch: r.get("git_branch"),
        last_reply: String::new(),
        parent_id: r.get("parent_id"),
        launched_by: r.get("launched_by"),
        labels: serde_json::from_str(r.get("labels")).unwrap_or_default(),
    }
}

/// An account's rate-limit windows as one `rate_limit_event` reported them (ADR-0042).
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Windows {
    pub time_ms: i64,
    pub five_hour_pct: Option<f64>,
    pub five_hour_resets_s: Option<i64>,
    pub seven_day_pct: Option<f64>,
    pub seven_day_resets_s: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[tokio::test]
    async fn a_node_keeps_the_tailnet_it_last_linked_over_until_it_links_over_tcp() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("hub.db")).await.unwrap();
        let (node, _) = db.register_node("mac", "0.4.0", 1).await.unwrap();
        let row = db.node(&node).await.unwrap().unwrap();
        assert_eq!((row.tailnet.as_str(), row.tailnet_peer.as_str()), ("", ""));
        db.set_node_tailnet(&node, "home", "mac (me@github)").await.unwrap();
        db.register_node("mac", "0.4.0", 2).await.unwrap();
        let row = db.nodes().await.unwrap().remove(0);
        assert_eq!((row.tailnet.as_str(), row.tailnet_peer.as_str()), ("home", "mac (me@github)"));
        db.set_node_tailnet(&node, "", "").await.unwrap();
        assert_eq!(db.node(&node).await.unwrap().unwrap().tailnet, "");
    }

    #[tokio::test]
    async fn a_session_counts_as_just_ended_once() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("hub.db")).await.unwrap();
        let (node, _) = db.register_node("mac", "0.1.0", 1).await.unwrap();
        let live = Session {
            id: "s1".into(),
            cwd: "/w".into(),
            ..Default::default()
        };
        assert!(!db.store_session(&node, &live).await.unwrap(), "a live session");
        let ended = Session {
            ended_ms: Some(9),
            ..live.clone()
        };
        assert!(db.store_session(&node, &ended).await.unwrap(), "the first update that ends it");
        assert!(!db.store_session(&node, &ended).await.unwrap(), "not again");
        let unknown = Session {
            id: "s2".into(),
            ended_ms: Some(9),
            ..Default::default()
        };
        assert!(!db.store_session(&node, &unknown).await.unwrap(), "never seen live");
    }

    #[tokio::test]
    async fn the_opening_leaves_out_tool_results_meta_lines_and_outsized_ones() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("hub.db")).await.unwrap();
        let (node, _) = db.register_node("mac", "0.1.0", 1).await.unwrap();
        let lines = [
            serde_json::json!({"type": "user", "isMeta": true, "message": {"role": "user", "content": "skill body"}}),
            serde_json::json!({"type": "user", "message": {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "x".repeat(100)}]}}),
            serde_json::json!({"type": "user", "message": {"role": "user", "content": "y".repeat(200)}}),
            serde_json::json!({"type": "user", "message": {"role": "user", "content": "fix the tests"}}),
            serde_json::json!({"type": "user", "message": {"role": "user", "content": [
                {"type": "text", "text": "and the docs"}]}}),
        ];
        for (i, l) in lines.iter().enumerate() {
            let e = Event {
                seq: i as u64 + 1,
                session_id: "s1".into(),
                source: EventSource::Transcript.into(),
                kind: "user".into(),
                payload_json: l.to_string(),
                ..Default::default()
            };
            db.store_event(&node, &e).await.unwrap();
        }
        let opening = db.opening(&node, "s1", 8, 150).await.unwrap();
        let seqs: Vec<u64> = opening.iter().map(|e| e.seq).collect();
        assert_eq!(seqs, [4, 5], "only the typed prompts that fit");
    }

    #[tokio::test]
    async fn the_hub_alone_says_who_launched_a_session() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("hub.db")).await.unwrap();
        let (node, _) = db.register_node("mac", "0.1.0", 1).await.unwrap();
        let by = |id: &'static str| {
            let db = db.clone();
            let node = node.clone();
            async move { db.session(&node, id).await.unwrap().unwrap().launched_by }
        };
        let s = |id: &str| Session { id: id.into(), cwd: "/w".into(), ..Default::default() };
        // Started at the node: the node's, whatever the node reports.
        db.store_session(&node, &Session { launched_by: "client:me".into(), ..s("own") }).await.unwrap();
        assert_eq!(by("own").await, "node:mac");
        // Spawned by a client: recorded before the node reports it, kept through every report.
        db.record_launch(&node, "spawned", "client:phone", &Default::default()).await.unwrap();
        db.store_session(&node, &s("spawned")).await.unwrap();
        db.store_session(&node, &Session { ended_ms: Some(5), ..s("spawned") }).await.unwrap();
        assert_eq!(by("spawned").await, "client:phone");
        // A fork named after the node's reply takes its resumer, even reported first.
        db.store_session(&node, &s("fork")).await.unwrap();
        db.record_launch(&node, "fork", "client:ctl", &Default::default()).await.unwrap();
        assert_eq!(by("fork").await, "client:ctl");
        // A client's session is never handed to another by a later record.
        db.record_launch(&node, "spawned", "client:other", &Default::default()).await.unwrap();
        assert_eq!(by("spawned").await, "client:phone");
    }

    #[tokio::test]
    async fn labels_come_with_the_launch_and_a_list_matches_them() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("hub.db")).await.unwrap();
        let (node, _) = db.register_node("mac", "0.1.0", 1).await.unwrap();
        let map = |kv: &[(&str, &str)]| kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect::<HashMap<_, _>>();
        let s = |id: &str| Session { id: id.into(), cwd: "/w".into(), ..Default::default() };
        db.record_launch(&node, "a", "client:app", &map(&[("job", "1234"), ("app/run.id", "7")])).await.unwrap();
        db.store_session(&node, &s("a")).await.unwrap();
        db.record_launch(&node, "b", "client:app", &map(&[("job", "99")])).await.unwrap();
        db.store_session(&node, &s("b")).await.unwrap();
        db.store_session(&node, &s("c")).await.unwrap();
        let ids = |l: HashMap<String, String>| {
            let db = db.clone();
            let f = Filter { labels: l, ..Default::default() };
            async move { db.sessions("", true, &f).await.unwrap().into_iter().map(|s| s.id).collect::<Vec<_>>() }
        };
        assert_eq!(db.session(&node, "a").await.unwrap().unwrap().labels, map(&[("job", "1234"), ("app/run.id", "7")]));
        assert_eq!(db.session(&node, "c").await.unwrap().unwrap().labels, map(&[]));
        assert_eq!(ids(map(&[])).await, ["a", "b", "c"]);
        assert_eq!(ids(map(&[("job", "1234")])).await, ["a"]);
        assert_eq!(ids(map(&[("job", "1234"), ("app/run.id", "7")])).await, ["a"]);
        assert_eq!(ids(map(&[("job", "1234"), ("app/run.id", "8")])).await, Vec::<String>::new());
        assert_eq!(ids(map(&[("nope", "")])).await, Vec::<String>::new());
    }

    #[tokio::test]
    async fn a_session_keeps_its_claude_token_when_reported_without_one() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("hub.db")).await.unwrap();
        let (node, _) = db.register_node("mac", "0.1.0", 1).await.unwrap();
        let s = Session {
            id: "s1".into(),
            cwd: "/w".into(),
            claude_token: "work".into(),
            ..Default::default()
        };
        db.store_session(&node, &s).await.unwrap();
        let without = Session {
            claude_token: String::new(),
            ..s.clone()
        };
        db.store_session(&node, &without).await.unwrap();
        assert_eq!(db.session(&node, "s1").await.unwrap().unwrap().claude_token, "work");
        let other = Session {
            claude_token: "personal".into(),
            ..s
        };
        db.store_session(&node, &other).await.unwrap();
        assert_eq!(db.session(&node, "s1").await.unwrap().unwrap().claude_token, "personal");
    }

    #[tokio::test]
    async fn history_leaves_out_a_session_that_kept_no_transcript() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("hub.db")).await.unwrap();
        let (node, _) = db.register_node("mac", "0.1.0", 1).await.unwrap();
        for (id, started_ms) in [("talked", 1), ("throwaway", 2)] {
            let s = Session {
                id: id.into(),
                cwd: format!("/tmp/{id}"),
                started_ms,
                ended_ms: Some(9),
                ..Default::default()
            };
            db.store_session(&node, &s).await.unwrap();
        }
        // Both have a hook event; only one has a transcript line.
        for (seq, (id, source)) in [
            ("talked", EventSource::Hook),
            ("talked", EventSource::Transcript),
            ("throwaway", EventSource::Hook),
        ]
        .into_iter()
        .enumerate()
        {
            let e = Event {
                seq: seq as u64 + 1,
                session_id: id.into(),
                source: source.into(),
                kind: "user".into(),
                payload_json: "{}".into(),
                ..Default::default()
            };
            db.store_event(&node, &e).await.unwrap();
        }
        let ids: Vec<String> = db
            .history("", i64::MAX, 50, &Filter::default())
            .await
            .unwrap()
            .into_iter()
            .map(|s| s.id)
            .collect();
        assert_eq!(ids, ["talked"]);
    }

    #[tokio::test]
    async fn a_usage_alert_is_claimed_once_across_nodes() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("hub.db")).await.unwrap();
        assert!(db.claim_usage_alert("seven_day:29889300:pace", "n1", 1).await.unwrap());
        assert!(!db.claim_usage_alert("seven_day:29889300:pace", "n2", 2).await.unwrap());
        assert!(db.claim_usage_alert("seven_day:29889300:near80", "n1", 3).await.unwrap());
    }

    #[tokio::test]
    async fn a_renewed_token_starts_its_year_again_and_reminds_once_per_expiry() {
        use crate::claude_tokens::{LIFETIME_MS, REMIND_MS};
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("hub.db")).await.unwrap();
        let fp = |n: &str, f: &str| vec![(n.to_string(), f.to_string())];
        db.sync_claude_tokens(&fp("work", "aaaa"), 1_000)
            .await
            .unwrap();
        // The same token at the next start: nothing changes.
        db.sync_claude_tokens(&fp("work", "aaaa"), 2_000)
            .await
            .unwrap();
        let t = &db.claude_tokens(&["work", "gone"]).await.unwrap()[0];
        assert_eq!(
            (t.first_seen_ms, t.expires_ms),
            (1_000, 1_000 + LIFETIME_MS)
        );

        let due = 1_000 + LIFETIME_MS - REMIND_MS;
        assert!(
            !db.claim_expiry_reminder("work", due - 1).await.unwrap(),
            "not yet"
        );
        assert!(
            db.claim_expiry_reminder("work", due).await.unwrap(),
            "a month before"
        );
        assert!(
            !db.claim_expiry_reminder("work", due + 1).await.unwrap(),
            "once"
        );
        // A new date by hand: reminded again when that one is near.
        assert!(
            db.set_claude_token_expiry("work", Some(due + REMIND_MS + 10))
                .await
                .unwrap()
        );
        assert!(!db.claim_expiry_reminder("work", due).await.unwrap());
        assert!(db.claim_expiry_reminder("work", due + 10).await.unwrap());

        let w = Windows {
            time_ms: 50,
            five_hour_pct: Some(4.0),
            ..Default::default()
        };
        db.record_claude_token_usage("work", &w).await.unwrap();
        let older = Windows {
            time_ms: 40,
            five_hour_pct: Some(99.0),
            ..Default::default()
        };
        db.record_claude_token_usage("work", &older).await.unwrap();
        assert_eq!(
            db.claude_tokens(&["work"]).await.unwrap()[0].five_hour_pct,
            Some(4.0)
        );

        // Renewed: a new fingerprint under the name starts over, its hand-set date and
        // usage gone.
        db.sync_claude_tokens(&fp("work", "bbbb"), 9_000)
            .await
            .unwrap();
        let t = &db.claude_tokens(&["work"]).await.unwrap()[0];
        assert_eq!(
            (t.first_seen_ms, t.expires_ms, t.expiry_set, t.usage_ms),
            (9_000, 9_000 + LIFETIME_MS, false, 0)
        );
        assert!(!db.set_claude_token_expiry("nope", None).await.unwrap());
    }
}
