//! Push interrupts only when it should (phase PHS-20261117-f22kxe item 5): a held request's
//! alert waits and goes out only if nobody answered it meanwhile, and a finished turn is
//! alerted only when it ran long; a turn whose start the hub never saw is alerted.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use saarthi_hub::db::Db;
use saarthi_hub::notify::Notifier;
use saarthi_proto::event::v1::{Event, EventSource, PendingKind, PendingRequest, ReleaseReason};

type Got = Arc<Mutex<Vec<String>>>;

/// An ntfy stand-in keeping each alert's title.
async fn fake_ntfy() -> (String, Got) {
    let got: Got = Arc::default();
    let keep = got.clone();
    let app = axum::Router::new().route(
        "/saarthi",
        axum::routing::post(move |headers: axum::http::HeaderMap| {
            let keep = keep.clone();
            async move { keep.lock().unwrap().push(headers["title"].to_str().unwrap().to_string()) }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/saarthi", listener.local_addr().unwrap());
    tokio::spawn(async move { axum::serve(listener, app).await });
    (url, got)
}

fn hook(seq: u64, session: &str, kind: &str, time_ms: i64) -> Event {
    Event {
        seq,
        session_id: session.into(),
        time_ms,
        source: EventSource::Hook.into(),
        kind: kind.into(),
        payload_json: format!(r#"{{"hook_event_name":"{kind}","session_id":"{session}","cwd":"/w"}}"#),
        ..Default::default()
    }
}

fn question(id: &str) -> PendingRequest {
    PendingRequest {
        id: id.into(),
        session_id: "s1".into(),
        kind: PendingKind::Question.into(),
        tool_name: "AskUserQuestion".into(),
        tool_input_json: r#"{"questions":[{"question":"Which?"}]}"#.into(),
        ..Default::default()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn only_unanswered_requests_and_long_turns_are_alerted() {
    let dir = tempfile::tempdir().unwrap();
    let db = Db::open(&dir.path().join("hub.db")).await.unwrap();
    let (node_id, _) = db.register_node("n", "0", 1).await.unwrap();
    let (url, got) = fake_ntfy().await;
    let notifier = Arc::new(
        Notifier::new(Some(url), db.clone(), Vec::new()).quiet(Duration::from_millis(300), Duration::from_secs(60)),
    );

    // A question answered within the hold is not alerted; one left open is.
    for id in ["answered", "open"] {
        let q = question(id);
        db.store_pending(&node_id, &q).await.unwrap();
        notifier.pending("n", &node_id, &q);
    }
    db.close_pending(&node_id, "answered", ReleaseReason::Resolved, 2).await.unwrap();

    // A ten-second turn is not alerted, a two-minute one is, and so is one with no prompt seen.
    let t = 1_000_000;
    for e in [
        hook(1, "quick", "UserPromptSubmit", t),
        hook(2, "quick", "Stop", t + 10_000),
        hook(3, "long", "UserPromptSubmit", t),
        hook(4, "long", "Stop", t + 120_000),
        hook(5, "unknown", "Stop", t),
    ] {
        db.store_event(&node_id, &e).await.unwrap();
        notifier.event("n", &node_id, &e);
    }

    tokio::time::sleep(Duration::from_millis(1500)).await;
    let mut titles = got.lock().unwrap().clone();
    titles.sort();
    assert_eq!(titles.len(), 3, "{titles:?}");
    assert_eq!(titles.iter().filter(|t| *t == "n: turn finished").count(), 2, "{titles:?}");
    assert!(titles.contains(&"n: Claude asks".to_string()), "{titles:?}");
}
