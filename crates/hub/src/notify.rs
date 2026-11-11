//! Push to the phone (ADR-0008, ADR-0025, ADR-0026): a held permission request or question,
//! and a turn that finished, so the person knows a session is waiting; and the account's
//! rate limits running out (ADR-0032). Two destinations:
//!
//! - each UnifiedPush endpoint a client registered (`RegisterPush`): a JSON message the app
//!   turns into its own notification, with Allow and Deny on a permission request;
//! - the ntfy topic of `--ntfy http(s)://host/topic`, if given: title, body and priority, for
//!   anyone subscribed with the ntfy app itself.
//!
//! The PermissionRequest itself is the trigger, since Claude Code's own Notification hook
//! arrives only after it returns (EXP-0001). Sending never blocks the node's stream: each
//! message goes out on its own task and a failure is only logged; an endpoint that answers
//! 404 or 410 is gone and forgotten, as UnifiedPush servers signal it.

use saarthi_proto::event::v1::{Event, EventSource, HealthSample, Metric, PendingKind, PendingRequest, Session};
use serde_json::{Value, json};

use crate::db::Db;
use crate::node_service::now_ms;

/// How much of a command or question goes into a notification.
const BODY_CHARS: usize = 200;

pub struct Notifier {
    off: Vec<String>,
    client: reqwest::Client,
    topic_url: Option<String>,
    db: Db,
}

/// One notification: ntfy's title, body and priority (1 to 5), and what the app receives.
#[derive(Debug, PartialEq)]
pub struct Message {
    pub title: String,
    pub body: String,
    pub priority: u8,
    pub push: Value,
}

impl Notifier {
    /// `off` names the kinds not to send: `permission`, `question`, `turn`, `ended`
    /// (ADR-0029), `usage` (ADR-0032).
    pub fn new(topic_url: Option<String>, db: Db, off: Vec<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            topic_url,
            db,
            off,
        }
    }

    /// A session that ended, by exit or by a kill.
    pub fn ended(&self, node: &str, node_id: &str, s: &Session) {
        self.send(for_ended(node, node_id, s));
    }

    pub fn pending(&self, node: &str, node_id: &str, p: &PendingRequest) {
        self.send(for_pending(node, node_id, p));
    }

    pub fn event(&self, node: &str, node_id: &str, e: &Event) {
        if let Some(m) = for_event(node, node_id, e) {
            self.send(m);
        }
    }

    /// Rate-limit alerts from a health sample, each sent once whichever node reports it.
    pub async fn health(&self, node: &str, node_id: &str, h: &HealthSample) {
        let now = now_ms();
        for (key, m) in for_usage(node, node_id, &h.metrics, now / 1000) {
            match self.db.claim_usage_alert(&key, node_id, now).await {
                Ok(true) => self.send(m),
                Ok(false) => {}
                Err(e) => tracing::warn!(error = %e, "recording a usage alert failed"),
            }
        }
    }

    /// Rate-limit alerts for the account behind a Claude token (ADR-0042), from the windows
    /// a session under it reported; each once, keyed by the token, whichever node reports it.
    pub async fn token_usage(&self, name: &str, node_id: &str, w: &crate::db::Windows) {
        let now = now_ms();
        for (key, m) in for_token_usage(name, node_id, w, now / 1000) {
            match self.db.claim_usage_alert(&key, node_id, now).await {
                Ok(true) => self.send(m),
                Ok(false) => {}
                Err(e) => tracing::warn!(error = %e, "recording a usage alert failed"),
            }
        }
    }

    /// The reminder that a Claude token expires soon (ADR-0039).
    pub fn token_expiry(&self, name: &str, expires_ms: i64, now_ms: i64) {
        self.send(for_token_expiry(name, expires_ms, now_ms));
    }

    fn send(&self, m: Message) {
        if m.push["kind"].as_str().is_some_and(|k| self.off.iter().any(|o| o == k)) {
            return;
        }
        let (client, topic, db) = (self.client.clone(), self.topic_url.clone(), self.db.clone());
        tokio::spawn(async move {
            if let Some(topic) = topic {
                let sent = client
                    .post(&topic)
                    .header("Title", &m.title)
                    .header("Priority", m.priority.to_string())
                    .body(m.body.clone())
                    .send()
                    .await
                    .and_then(|r| r.error_for_status());
                if let Err(e) = sent {
                    tracing::warn!(error = %e, "ntfy publish failed");
                }
            }
            let endpoints = db.push_endpoints().await.unwrap_or_default();
            let body = m.push.to_string();
            for endpoint in endpoints {
                match client.post(&endpoint).body(body.clone()).send().await {
                    Ok(r) if r.status() == 404 || r.status() == 410 => {
                        tracing::info!(%endpoint, "push endpoint gone; forgetting it");
                        let _ = db.remove_push_endpoint(&endpoint).await;
                    }
                    Ok(r) if !r.status().is_success() => {
                        tracing::warn!(%endpoint, status = %r.status(), "push failed")
                    }
                    Ok(_) => {}
                    Err(e) => tracing::warn!(%endpoint, error = %e, "push failed"),
                }
            }
        });
    }
}

pub fn for_pending(node: &str, node_id: &str, p: &PendingRequest) -> Message {
    let input: Value = serde_json::from_str(&p.tool_input_json).unwrap_or_default();
    let question = p.kind() == PendingKind::Question;
    let (title, body) = if question {
        let q = input["questions"][0]["question"]
            .as_str()
            .unwrap_or("a question");
        (format!("{node}: Claude asks"), clip(q))
    } else {
        // The part of the input a person decides on: the command, or the file it touches.
        let what = ["command", "file_path", "url", "pattern"]
            .iter()
            .find_map(|k| input[k].as_str())
            .map(str::to_string)
            .unwrap_or_else(|| p.tool_input_json.clone());
        (format!("{node}: allow {}?", p.tool_name), clip(&what))
    };
    let push = json!({
        "kind": if question { "question" } else { "permission" },
        "node_id": node_id, "node": node, "pending_id": p.id, "session_id": p.session_id,
        "tool": p.tool_name, "title": title, "body": body,
    });
    Message {
        title,
        body,
        priority: 4,
        push,
    }
}

/// A session that ended: the person may be waiting for it, or it may have died.
pub fn for_ended(node: &str, node_id: &str, s: &Session) -> Message {
    let title = format!("{node}: session ended");
    let body = clip(if s.cwd.is_empty() { &s.id } else { &s.cwd });
    let push = json!({
        "kind": "ended", "node_id": node_id, "node": node, "session_id": s.id,
        "title": title, "body": body,
    });
    Message {
        title,
        body,
        priority: 3,
        push,
    }
}

/// A finished turn: the Stop hook. Other events do not notify.
pub fn for_event(node: &str, node_id: &str, e: &Event) -> Option<Message> {
    if e.source() != EventSource::Hook || e.kind != "Stop" {
        return None;
    }
    let input: Value = serde_json::from_str(&e.payload_json).unwrap_or_default();
    let title = format!("{node}: turn finished");
    let body = clip(input["cwd"].as_str().unwrap_or(&e.session_id));
    let push = json!({
        "kind": "turn", "node_id": node_id, "node": node, "session_id": e.session_id,
        "title": title, "body": body,
    });
    Some(Message {
        title,
        body,
        priority: 3,
        push,
    })
}

/// The rate-limit windows the node reports: the metric's middle, its name in a message, its
/// length in seconds, and whether its pace is watched (a week's is; a 5-hour window spent in
/// one sitting is how it is meant to be used).
const WINDOWS: [(&str, &str, i64, bool); 3] = [
    ("five_hour", "5-hour limit", 5 * 3600, false),
    ("seven_day", "7-day limit", 7 * 86400, true),
    ("seven_day_fable", "7-day Fable limit", 7 * 86400, true),
];
/// Percent used from which a window is near its limit, highest first.
const NEAR: [f64; 2] = [95.0, 80.0];
/// Below this much used, a fast start to the week is not worth a notification.
const PACE_FROM: f64 = 20.0;

/// The rate-limit alerts a health sample calls for, each with the key that makes it once per
/// window, reset and rule (ADR-0032): a window at its limit, one past 95% or 80%, and a week
/// used faster than it lasts, on course to run out before it resets.
pub fn for_usage(node: &str, node_id: &str, metrics: &[Metric], now_s: i64) -> Vec<(String, Message)> {
    let get = |name: String| metrics.iter().find(|m| m.name == name).map(|m| m.value);
    let mut out = Vec::new();
    for (window, label, length, pace) in WINDOWS {
        let (Some(pct), Some(resets)) = (get(format!("usage.{window}_pct")), get(format!("usage.{window}_resets_s")))
        else {
            continue;
        };
        let left = resets as i64 - now_s;
        if left <= 0 {
            // A cached value from before the window reset.
            continue;
        }
        let alert = if pct >= 100.0 {
            Some(("hit".to_string(), format!("{node}: Claude {label} reached"), format!("Resets in {}.", span(left)), 5))
        } else if let Some(near) = NEAR.iter().find(|n| pct >= **n) {
            Some((format!("near{near}"), format!("{node}: Claude {label} at {pct:.0}%"), format!("Resets in {}.", span(left)), 4))
        } else if pace && pct >= PACE_FROM {
            // At the rate so far, the time until the rest is used.
            let elapsed = (length - left).max(1);
            let to_full = ((100.0 - pct) * elapsed as f64 / pct) as i64;
            (to_full < left).then(|| {
                (
                    "pace".to_string(),
                    format!("{node}: Claude {label} going fast"),
                    format!("{pct:.0}% used with {} to go; at this pace it runs out in {}.", span(left), span(to_full)),
                    4,
                )
            })
        } else {
            None
        };
        if let Some((rule, title, body, priority)) = alert {
            let key = format!("{window}:{}:{rule}", (resets as i64 + 30) / 60);
            let push = json!({
                "kind": "usage", "node_id": node_id, "node": node, "window": window,
                "key": key, "title": title, "body": body,
            });
            out.push((key, Message { title, body, priority, push }));
        }
    }
    out
}

/// The alerts of `for_usage` for the account behind a Claude token: titled with the token's
/// name and keyed by it, so two accounts whose windows reset together alert separately.
pub fn for_token_usage(
    name: &str,
    node_id: &str,
    w: &crate::db::Windows,
    now_s: i64,
) -> Vec<(String, Message)> {
    let mut metrics = Vec::new();
    let mut add = |n: &str, v: Option<f64>| {
        if let Some(value) = v {
            metrics.push(Metric {
                name: n.to_string(),
                value,
            });
        }
    };
    add("usage.five_hour_pct", w.five_hour_pct);
    add(
        "usage.five_hour_resets_s",
        w.five_hour_resets_s.map(|s| s as f64),
    );
    add("usage.seven_day_pct", w.seven_day_pct);
    add(
        "usage.seven_day_resets_s",
        w.seven_day_resets_s.map(|s| s as f64),
    );
    for_usage(&format!("{name} account"), node_id, &metrics, now_s)
        .into_iter()
        .map(|(key, mut m)| {
            let key = format!("token:{name}:{key}");
            m.push["key"] = json!(key);
            m.push["claude_token"] = json!(name);
            (key, m)
        })
        .collect()
}

/// "Claude token work expires in 30 days": the reminder, sent once per expiry date.
pub fn for_token_expiry(name: &str, expires_ms: i64, now_ms: i64) -> Message {
    let left = (expires_ms - now_ms) / 1000;
    let (title, body) = if left > 0 {
        (
            format!("Claude token {name} expires in {}", span(left)),
            "Make a new one with `claude setup-token` and replace it in the hub's token file."
                .to_string(),
        )
    } else {
        (
            format!("Claude token {name} has expired"),
            "Sessions under it will fail. Make a new one with `claude setup-token`.".to_string(),
        )
    };
    let push = json!({
        "kind": "token", "claude_token": name, "expires_ms": expires_ms,
        "key": format!("token-expiry:{name}:{expires_ms}"),
        "title": title, "body": body,
    });
    Message {
        title,
        body,
        priority: 4,
        push,
    }
}

/// "3 days" (to the nearest day), "1 day 4 h", "2 h 20 min", "40 min".
fn span(secs: i64) -> String {
    let (d, h, m) = (secs / 86400, secs % 86400 / 3600, secs % 3600 / 60);
    match (d, h, m) {
        (2.., _, _) => format!("{} days", (secs + 43200) / 86400),
        (1, 0, _) => "1 day".into(),
        (1, _, _) => format!("1 day {h} h"),
        (0, 1.., 0) => format!("{h} h"),
        (0, 1.., _) => format!("{h} h {m} min"),
        _ => format!("{m} min"),
    }
}

fn clip(s: &str) -> String {
    match s.char_indices().nth(BODY_CHARS) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pending(kind: PendingKind, tool: &str, input: &str) -> PendingRequest {
        PendingRequest {
            id: "p1".into(),
            session_id: "s1".into(),
            kind: kind.into(),
            tool_name: tool.into(),
            tool_input_json: input.into(),
            ..Default::default()
        }
    }

    fn usage(metrics: &[(&str, f64)]) -> Vec<Metric> {
        metrics.iter().map(|(n, v)| Metric { name: n.to_string(), value: *v }).collect()
    }

    #[test]
    fn a_week_used_faster_than_it_lasts_alerts_once_per_reset() {
        let day = 86400;
        let resets = 1_793_358_000.0;
        // 40% used two days into the week: at that rate the rest goes in three days, two
        // before the reset. The 5-hour window's pace is not watched.
        let now = resets as i64 - 5 * day;
        let m = usage(&[
            ("usage.five_hour_pct", 70.0),
            ("usage.five_hour_resets_s", (now + 3600) as f64),
            ("usage.seven_day_pct", 40.0),
            ("usage.seven_day_resets_s", resets),
        ]);
        let got = for_usage("rogue", "n1", &m, now);
        assert_eq!(got.len(), 1);
        let (key, msg) = &got[0];
        assert_eq!(key, &format!("seven_day:{}:pace", 1_793_358_000 / 60));
        assert_eq!(msg.title, "rogue: Claude 7-day limit going fast");
        assert_eq!(msg.body, "40% used with 5 days to go; at this pace it runs out in 3 days.");
        assert_eq!(msg.push["kind"], json!("usage"));
        // The same share five days in is on course; under 20% a fast start is not news.
        assert!(for_usage("rogue", "n1", &m, resets as i64 - 2 * day).is_empty());
        let early = usage(&[("usage.seven_day_pct", 15.0), ("usage.seven_day_resets_s", resets)]);
        assert!(for_usage("rogue", "n1", &early, resets as i64 - 6 * day - 3600).is_empty());
        assert_eq!(span(5 * day - 3600), "5 days");
    }

    #[test]
    fn a_window_near_or_at_its_limit_alerts_by_level_and_a_stale_one_not_at_all() {
        let now = 1_000_000;
        let m = usage(&[
            ("usage.five_hour_pct", 100.0),
            ("usage.five_hour_resets_s", (now + 2 * 3600 + 20 * 60) as f64),
            ("usage.seven_day_fable_pct", 83.0),
            ("usage.seven_day_fable_resets_s", (now + 86400 + 4 * 3600) as f64),
            ("usage.seven_day_pct", 99.0),
            ("usage.seven_day_resets_s", (now - 60) as f64),
        ]);
        let got = for_usage("rogue", "n1", &m, now);
        let shown: Vec<_> = got.iter().map(|(k, m)| (k.rsplit(':').next().unwrap(), m.title.as_str(), m.body.as_str(), m.priority)).collect();
        assert_eq!(
            shown,
            [
                ("hit", "rogue: Claude 5-hour limit reached", "Resets in 2 h 20 min.", 5),
                ("near80", "rogue: Claude 7-day Fable limit at 83%", "Resets in 1 day 4 h.", 4),
            ]
        );
    }

    #[test]
    fn an_ended_session_is_a_message_of_its_own_kind() {
        let s = Session {
            id: "s9".into(),
            cwd: "/w/saarthi".into(),
            ended_ms: Some(5),
            ..Default::default()
        };
        let m = for_ended("mac", "n1", &s);
        assert_eq!((m.title.as_str(), m.body.as_str()), ("mac: session ended", "/w/saarthi"));
        assert_eq!(m.push["kind"], json!("ended"));
        assert_eq!(m.push["session_id"], json!("s9"));
    }

    #[test]
    fn messages_say_what_waits_and_where() {
        let p = pending(
            PendingKind::Permission,
            "Bash",
            r#"{"command":"make deploy"}"#,
        );
        let m = for_pending("mac", "n1", &p);
        assert_eq!(
            (m.title.as_str(), m.body.as_str(), m.priority),
            ("mac: allow Bash?", "make deploy", 4)
        );
        assert_eq!(
            m.push,
            json!({"kind": "permission", "node_id": "n1", "node": "mac", "pending_id": "p1",
                   "session_id": "s1", "tool": "Bash", "title": "mac: allow Bash?",
                   "body": "make deploy"})
        );
        let q = pending(
            PendingKind::Question,
            "AskUserQuestion",
            r#"{"questions":[{"question":"Ship it?"}]}"#,
        );
        let m = for_pending("mac", "n1", &q);
        assert_eq!(
            (m.body.as_str(), &m.push["kind"]),
            ("Ship it?", &json!("question"))
        );
        let long = format!(r#"{{"command":"{}"}}"#, "x".repeat(500));
        let clipped = for_pending(
            "mac",
            "n1",
            &pending(PendingKind::Permission, "Bash", &long),
        )
        .body;
        assert_eq!(clipped.chars().count(), BODY_CHARS + 1);

        let stop = Event {
            session_id: "s1".into(),
            source: EventSource::Hook.into(),
            kind: "Stop".into(),
            payload_json: r#"{"cwd":"/w/saarthi"}"#.into(),
            ..Default::default()
        };
        let m = for_event("mac", "n1", &stop).unwrap();
        assert_eq!(
            (m.body.as_str(), &m.push["kind"]),
            ("/w/saarthi", &json!("turn"))
        );
        let other = Event {
            kind: "PreToolUse".into(),
            ..stop
        };
        assert!(for_event("mac", "n1", &other).is_none());
    }

    #[test]
    fn a_token_s_usage_alerts_name_the_account_and_are_keyed_by_it() {
        let now = 1_800_000_000;
        let w = crate::db::Windows {
            time_ms: 0,
            five_hour_pct: Some(96.0),
            five_hour_resets_s: Some(now + 3600),
            seven_day_pct: Some(10.0),
            seven_day_resets_s: Some(now + 6 * 86400),
        };
        let got = for_token_usage("work", "n1", &w, now);
        assert_eq!(got.len(), 1, "{got:?}");
        let (key, m) = &got[0];
        assert!(key.starts_with("token:work:five_hour:"), "{key}");
        assert_eq!(m.title, "work account: Claude 5-hour limit at 96%");
        assert_eq!(m.push["claude_token"], "work");
        assert_eq!(m.push["key"], key.as_str());
        let other = for_token_usage("personal", "n1", &w, now);
        assert_ne!(other[0].0, *key);
    }

    #[test]
    fn the_expiry_reminder_says_when() {
        let day = 86_400_000;
        let m = for_token_expiry("work", 30 * day, 0);
        assert_eq!(m.title, "Claude token work expires in 30 days");
        assert_eq!(m.push["kind"], "token");
        assert_eq!(
            for_token_expiry("work", 0, day).title,
            "Claude token work has expired"
        );
    }
}
