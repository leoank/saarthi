//! What the phone reads besides lists and pending requests: a session's events, stored
//! ones first and then live, health history, and notifications to an ntfy topic and to a
//! registered UnifiedPush endpoint.

mod common;

use std::sync::{Arc, Mutex};
use std::time::Duration;

use common::{HubProc, post_hook, start_node};
use saarthi_proto::control::v1::control_service_client::ControlServiceClient;
use saarthi_proto::control::v1::*;
use saarthi_proto::event::v1::{EventSource, HealthSample, Metric};
use serde_json::json;

fn req<T>(body: T) -> tonic::Request<T> {
    let mut r = tonic::Request::new(body);
    r.metadata_mut()
        .insert("authorization", "Bearer t-client".parse().unwrap());
    r
}

type Got = Arc<Mutex<Vec<(String, String)>>>;

/// An ntfy stand-in: `/saarthi` is the topic and keeps each POST's Title header and body;
/// `/up` is a UnifiedPush endpoint and keeps each body (title "up").
async fn fake_ntfy() -> (String, Got) {
    use axum::http::HeaderMap;
    let got: Got = Arc::new(Mutex::new(Vec::new()));
    let (keep, keep_up) = (got.clone(), got.clone());
    let app = axum::Router::new()
        .route(
            "/saarthi",
            axum::routing::post(move |headers: HeaderMap, body: String| {
                let keep = keep.clone();
                async move {
                    let title = headers["title"].to_str().unwrap().to_string();
                    keep.lock().unwrap().push((title, body));
                }
            }),
        )
        .route(
            "/up",
            axum::routing::post(move |body: String| {
                let keep = keep_up.clone();
                async move { keep.lock().unwrap().push(("up".into(), body)) }
            }),
        );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/saarthi", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    (url, got)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_streams_a_session_reads_health_and_is_notified() {
    let dir = tempfile::tempdir().unwrap();
    let (ntfy_url, pushed) = fake_ntfy().await;
    let (hub, addr) = HubProc::start_with(
        &dir.path().join("hub.db"),
        "127.0.0.1:0",
        Some(ntfy_url.clone()),
    );
    let node = start_node(&addr, "t-node", dir.path(), Duration::from_millis(500)).await;
    let mut c = ControlServiceClient::connect(format!("http://{addr}"))
        .await
        .unwrap();

    let hook = |kind: &str, extra: serde_json::Value| {
        let mut v = json!({"hook_event_name": kind, "session_id": "s1", "cwd": "/w/saarthi"});
        v.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        v.to_string()
    };
    post_hook(
        node.hooks,
        hook("UserPromptSubmit", json!({"prompt": "one"})),
    )
    .await;
    post_hook(node.hooks, hook("Stop", json!({}))).await;
    let node_id = loop {
        let nodes = c
            .list_nodes(req(ListNodesRequest {}))
            .await
            .unwrap()
            .into_inner()
            .nodes;
        if let Some(n) = nodes.into_iter().find(|n| n.connected) {
            break n.id;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    // Over plain TCP the hub names no tailnet for the call (ADR-0076).
    let listed = c.list_nodes(req(ListNodesRequest {})).await.unwrap().into_inner();
    assert_eq!(listed.tailnet, "");

    // Stored events first, then live ones as they are stored.
    let mut stream = c
        .stream_session(req(StreamSessionRequest {
            node_id: node_id.clone(),
            session_id: "s1".into(),
            after_seq: 0,
            ..Default::default()
        }))
        .await
        .unwrap()
        .into_inner();
    let mut kinds = Vec::new();
    while kinds.len() < 2 {
        kinds.push(stream.message().await.unwrap().unwrap().kind);
    }
    post_hook(
        node.hooks,
        hook("UserPromptSubmit", json!({"prompt": "two"})),
    )
    .await;
    let live = stream.message().await.unwrap().unwrap();
    kinds.push(live.kind.clone());
    assert_eq!(kinds, ["UserPromptSubmit", "Stop", "UserPromptSubmit"]);
    assert_eq!(live.source(), EventSource::Hook);
    // A stream that starts after a seq gets only what follows it.
    let mut later = c
        .stream_session(req(StreamSessionRequest {
            node_id: node_id.clone(),
            session_id: "s1".into(),
            after_seq: live.seq - 1,
            ..Default::default()
        }))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(later.message().await.unwrap().unwrap().seq, live.seq);

    // Health history.
    let sample = HealthSample {
        time_ms: 1_000,
        metrics: vec![
            Metric {
                name: "cpu.util_pct".into(),
                value: 12.5,
            },
            Metric {
                name: "mem.used_bytes".into(),
                value: 2e9,
            },
        ],
    };
    node.outbox.health(sample.clone()).await;
    let samples = loop {
        let req_ = GetHealthRequest {
            node_id: node_id.clone(),
            since_ms: 0,
        };
        let s = c.get_health(req(req_)).await.unwrap().into_inner().samples;
        if !s.is_empty() {
            break s;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert_eq!(samples, [sample]);

    // A phone registers its UnifiedPush endpoint; a bad one is refused.
    let up = ntfy_url.replace("/saarthi", "/up");
    let bad = RegisterPushRequest {
        endpoint: "not a url".into(),
        unregister: false,
    };
    assert!(c.register_push(req(bad)).await.is_err());
    let register = RegisterPushRequest {
        endpoint: up.clone(),
        unregister: false,
    };
    c.register_push(req(register)).await.unwrap();

    // A held request and the finished turn went to the topic, and the request to the phone.
    let held = json!({"hook_event_name": "PermissionRequest", "session_id": "s1",
                      "tool_name": "Bash", "tool_input": {"command": "make deploy"}});
    assert_eq!(post_hook(node.hooks, held.to_string()).await, "{}");
    let start = std::time::Instant::now();
    while pushed.lock().unwrap().len() < 3 {
        assert!(
            start.elapsed() < Duration::from_secs(10),
            "{:?}",
            pushed.lock().unwrap()
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let got = pushed.lock().unwrap().clone();
    assert!(
        got.contains(&("test-node: turn finished".into(), "/w/saarthi".into())),
        "{got:?}"
    );
    assert!(
        got.contains(&("test-node: allow Bash?".into(), "make deploy".into())),
        "{got:?}"
    );
    let to_phone: serde_json::Value = got
        .iter()
        .find(|(t, _)| t == "up")
        .map(|(_, b)| serde_json::from_str(b).unwrap())
        .unwrap();
    assert_eq!(
        (&to_phone["kind"], &to_phone["tool"], &to_phone["node_id"]),
        (&json!("permission"), &json!("Bash"), &json!(node_id))
    );

    // Holding the notifications stream (ADR-0075), the phone gets the request there and
    // nothing at its endpoint; let go, the endpoint gets them again.
    let mut stream = c.notifications(req(NotificationsRequest {})).await.unwrap().into_inner();
    tokio::time::sleep(Duration::from_millis(100)).await;
    pushed.lock().unwrap().clear();
    post_hook(node.hooks, held.to_string()).await;
    let first = loop {
        let n = tokio::time::timeout(Duration::from_secs(10), stream.message()).await.unwrap().unwrap().unwrap();
        if !n.push_json.is_empty() {
            break n;
        }
    };
    let on_stream: serde_json::Value = serde_json::from_str(&first.push_json).unwrap();
    assert_eq!((&on_stream["kind"], &on_stream["tool"]), (&json!("permission"), &json!("Bash")));
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(pushed.lock().unwrap().iter().all(|(t, _)| t != "up"), "{:?}", pushed.lock().unwrap());
    drop(stream);
    tokio::time::sleep(Duration::from_millis(200)).await;
    pushed.lock().unwrap().clear();
    post_hook(node.hooks, held.to_string()).await;
    let start = std::time::Instant::now();
    while !pushed.lock().unwrap().iter().any(|(t, _)| t == "up") {
        assert!(start.elapsed() < Duration::from_secs(10), "{:?}", pushed.lock().unwrap());
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    // Unregistered, the phone gets nothing more.
    let unregister = RegisterPushRequest {
        endpoint: up,
        unregister: true,
    };
    c.register_push(req(unregister)).await.unwrap();
    pushed.lock().unwrap().clear();
    post_hook(node.hooks, held.to_string()).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert!(pushed.lock().unwrap().iter().all(|(t, _)| t != "up"));
    hub.stop();
}
