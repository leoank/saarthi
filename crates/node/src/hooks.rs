//! The HTTP hook endpoint Claude Code posts every hook event to (EXP-0001). Every event is
//! recorded and answered with `{}` at once, which leaves Claude's own behaviour unchanged,
//! except PermissionRequest while a hub is linked: that one is held until a person decides
//! on another device or the hold runs out (`pending`).

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::IntoResponse;
use axum::routing::post;
use saarthi_proto::event::v1::{EventSource, ReleaseReason};
use serde_json::{Value, json};

use crate::outbox::Outbox;
use crate::pending::Pending;
use crate::sessions::Sessions;

/// The events the node subscribes to, in the order Claude Code documents them.
pub const EVENTS: [&str; 9] = [
    "SessionStart",
    "UserPromptSubmit",
    "PreToolUse",
    "PermissionRequest",
    "PostToolUse",
    "Notification",
    "Stop",
    "SubagentStop",
    "SessionEnd",
];

/// PermissionRequest hooks are held only when someone can resolve them: with a hub link.
#[derive(Clone)]
pub struct Hold {
    pub pending: Arc<Pending>,
    /// How long an unwrapped session's dialog waits for a resolution before it falls
    /// through to the terminal.
    pub duration: Duration,
    /// The same for a session under `saarthi run`, where a keystroke at the terminal releases
    /// the hold at once, and for a spawned one, which has no terminal; bounded only so it
    /// ends before Claude Code's hook timeout.
    pub wrapped_duration: Duration,
    /// Where the node saves photos sent with prompts (ADR-0071): a session's `Read` of one
    /// saved for it is allowed without asking anyone.
    pub upload_roots: Vec<std::path::PathBuf>,
}

#[derive(Clone)]
struct Hooks {
    outbox: Outbox,
    sessions: Arc<Sessions>,
    hold: Option<Hold>,
}

pub fn router(outbox: Outbox, sessions: Arc<Sessions>, hold: Option<Hold>) -> Router {
    Router::new().route("/hook", post(hook)).with_state(Hooks {
        outbox,
        sessions,
        hold,
    })
}

async fn hook(State(h): State<Hooks>, body: String) -> impl IntoResponse {
    let json = [(header::CONTENT_TYPE, "application/json")];
    let Ok(input) = serde_json::from_str::<Value>(&body) else {
        return (StatusCode::BAD_REQUEST, json, "{}".to_string());
    };
    let (Some(session_id), Some(name)) = (
        input["session_id"].as_str(),
        input["hook_event_name"].as_str(),
    ) else {
        return (StatusCode::BAD_REQUEST, json, "{}".to_string());
    };
    if h.sessions.ignored(input["cwd"].as_str().unwrap_or_default()) {
        // Not ours: nothing recorded, nothing held, so a dialog falls through at once.
        return (StatusCode::OK, json, "{}".to_string());
    }
    h.sessions.apply_hook(&input).await;
    h.outbox
        .event(session_id, EventSource::Hook, name, body.clone())
        .await;
    let response = match (name, &h.hold) {
        ("PermissionRequest", Some(hold))
            if input["tool_name"] == "Read"
                && input["tool_input"]["file_path"].as_str().is_some_and(|p| {
                    crate::uploads::saved_for(std::path::Path::new(p), session_id, &hold.upload_roots)
                }) =>
        {
            json!({"hookSpecificOutput": {"hookEventName": "PermissionRequest", "decision": {"behavior": "allow"}}}).to_string()
        }
        ("PermissionRequest", Some(hold)) => {
            // A spawned session has no terminal to fall through to either (ADR-0034).
            let duration = if h.sessions.is_wrapped(session_id).await
                || h.sessions.is_spawned(session_id).await
            {
                hold.wrapped_duration
            } else {
                hold.duration
            };
            hold.pending.hold(&input, duration).await.to_string()
        }
        ("SessionEnd", Some(hold)) => {
            hold.pending
                .release_session(session_id, ReleaseReason::SessionEnded)
                .await;
            "{}".to_string()
        }
        _ => "{}".to_string(),
    };
    (StatusCode::OK, json, response)
}

/// The `hooks` block for a sandboxed session (ADR-0038): loopback TCP is closed inside, so
/// every event goes as a command hook posting to the node's unix socket (EXP-20261107-413q82).
pub fn socket_settings(socket: &std::path::Path, timeout_secs: u32) -> Value {
    let command = format!(
        "/usr/bin/curl -s --max-time {timeout_secs} --unix-socket '{}' -X POST \
         -H 'Content-Type: application/json' --data-binary @- http://hook/hook",
        socket.display()
    );
    let entry = json!([{ "hooks": [{ "type": "command", "command": command, "timeout": timeout_secs }] }]);
    let hooks: serde_json::Map<String, Value> = EVENTS
        .iter()
        .map(|e| (e.to_string(), entry.clone()))
        .collect();
    json!({ "hooks": hooks })
}

/// The `hooks` block for `~/.claude/settings.json` that points every event at the node.
/// SessionStart fires only from the settings file, not from `--settings` (EXP-0001).
pub fn settings(addr: SocketAddr, timeout_secs: u32) -> Value {
    let url = format!("http://{addr}/hook");
    let entry = json!([{ "hooks": [{ "type": "http", "url": url, "timeout": timeout_secs }] }]);
    let hooks: serde_json::Map<String, Value> = EVENTS
        .iter()
        .map(|e| (e.to_string(), entry.clone()))
        .collect();
    json!({ "hooks": hooks })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn settings_cover_every_event() {
        let s = settings("127.0.0.1:7777".parse().unwrap(), 600);
        for e in EVENTS {
            assert_eq!(
                s["hooks"][e][0]["hooks"][0]["url"],
                "http://127.0.0.1:7777/hook"
            );
        }
    }

    #[test]
    fn socket_settings_post_every_event_to_the_socket() {
        let s = socket_settings(std::path::Path::new("/s/hook.sock"), 600);
        for e in EVENTS {
            let h = &s["hooks"][e][0]["hooks"][0];
            assert_eq!(h["type"], "command");
            let c = h["command"].as_str().unwrap();
            assert!(c.contains("--unix-socket '/s/hook.sock'"), "{c}");
            assert!(c.ends_with("http://hook/hook"), "{c}");
        }
    }

    #[tokio::test]
    async fn a_posted_hook_is_recorded_and_answered_empty() {
        let dir = tempfile::tempdir().unwrap();
        let (outbox, mut rx) = Outbox::new(64);
        let sessions = Arc::new(Sessions::new(
            dir.path().to_path_buf(),
            outbox.clone(),
            Duration::from_millis(10),
            Vec::new(),
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, router(outbox, sessions, None)).await });

        let body =
            r#"{"hook_event_name":"PreToolUse","session_id":"s1","cwd":"/w","tool_name":"Bash"}"#;
        let reply = post_raw(addr, body).await;
        assert!(reply.starts_with("HTTP/1.1 200"), "{reply}");
        assert!(reply.ends_with("{}"), "{reply}");
        assert!(post_raw(addr, "nope").await.starts_with("HTTP/1.1 400"));

        let mut events = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            if let Some(saarthi_proto::node::v1::node_message::Body::Event(e)) = msg.body {
                events.push(e);
            }
        }
        assert_eq!(events.len(), 1);
        assert_eq!(
            (events[0].kind.as_str(), events[0].payload_json.as_str()),
            ("PreToolUse", body)
        );
    }

    /// A bare HTTP/1.1 POST, so the test needs no HTTP client.
    #[tokio::test]
    async fn a_hook_from_an_ignored_directory_is_answered_empty_and_not_recorded() {
        let dir = tempfile::tempdir().unwrap();
        let (outbox, mut rx) = Outbox::new(64);
        let sessions = Arc::new(Sessions::new(
            dir.path().to_path_buf(),
            outbox.clone(),
            Duration::from_millis(10),
            vec![glob::Pattern::new("*/target/live-work-*").unwrap()],
        ));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let shared = sessions.clone();
        tokio::spawn(async move { axum::serve(listener, router(outbox, shared, None)).await });

        let body = r#"{"hook_event_name":"PermissionRequest","session_id":"t1","cwd":"/w/target/live-work-3","tool_name":"Bash"}"#;
        let reply = post_raw(addr, body).await;
        assert!(reply.starts_with("HTTP/1.1 200") && reply.ends_with("{}"), "{reply}");
        assert!(rx.try_recv().is_err(), "nothing recorded");
        assert!(sessions.snapshot().await.is_empty());
    }

    async fn post_raw(addr: SocketAddr, body: &str) -> String {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        let request = format!(
            "POST /hook HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut reply = String::new();
        stream.read_to_string(&mut reply).await.unwrap();
        reply
    }
}
