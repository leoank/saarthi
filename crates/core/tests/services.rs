//! The core's service calls against a real hub and node on loopback, as the app's notebook
//! section uses them (ADR-0052, ADR-0053): the node's services listed, a node's notebooks
//! read from its `yajna` service, a request with a body, and an answer read as it streams.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::routing::{get, post};
use saarthi_core::{CoreError, HeaderInfo, Hub};
use saarthi_hub::db::Db;
use saarthi_hub::live::Live;
use saarthi_hub::tokens::Tokens;
use saarthi_node::outbox::Outbox;
use saarthi_node::pending::Pending;
use saarthi_node::services::Services;
use saarthi_node::sessions::Sessions;
use saarthi_node::wrapper::Wrappers;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

/// Stands in for `yajna serve --socket`.
fn yajna() -> axum::Router {
    axum::Router::new()
        .route(
            "/api/notebooks",
            get(|| async {
                r#"{"notebooks":[{"namespace":"lab","path":"/w/lab","shape":"lab","url":"/n/lab/"}]}"#
            }),
        )
        .route(
            "/n/lab/api/reviews/r/comment",
            post(|h: axum::http::HeaderMap, body: axum::body::Bytes| async move {
                let t = h.get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
                format!("{t} {}", String::from_utf8_lossy(&body))
            }),
        )
        .route(
            "/n/lab/api/events",
            get(|| async {
                let (tx, rx) = mpsc::channel::<Result<String, std::io::Error>>(2);
                tokio::spawn(async move {
                    let _ = tx.send(Ok("data: one\n\n".into())).await;
                    tokio::time::sleep(Duration::from_secs(30)).await;
                    let _ = tx.send(Ok("data: two\n\n".into())).await;
                });
                ([("content-type", "text/event-stream")], Body::from_stream(ReceiverStream::new(rx)))
            }),
        )
}

#[tokio::test(flavor = "multi_thread")]
async fn the_core_reads_a_nodes_notebooks_and_streams_its_answers() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("yajna.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    tokio::spawn(async move { axum::serve(listener, yajna()).await });

    let tcp = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = tcp.local_addr().unwrap();
    let db = Db::open(&dir.path().join("hub.db")).await.unwrap();
    let hub = saarthi_hub::node_service::Hub {
        notifier: Arc::new(saarthi_hub::notify::Notifier::new(None, db.clone(), Vec::new())),
        db,
        node_tokens: Arc::new(Tokens::parse("mac t-node").unwrap()),
        client_tokens: Arc::new(Tokens::parse("phone t-phone").unwrap()),
        claude_tokens: Default::default(),
        claude_api: Arc::new(saarthi_hub::claude_api::ClaudeApi::new(saarthi_hub::claude_api::UPSTREAM)),
        live: Live::new(),
        authz: Arc::new(saarthi_hub::authz::Authz::new(saarthi_hub::authz::DEFAULT_POLICIES, ["phone"], []).unwrap()),
    };
    tokio::spawn(saarthi_hub::serve(tcp, hub));

    let (outbox, rx) = Outbox::new(256);
    let sessions = Arc::new(Sessions::new(dir.path().into(), outbox.clone(), Duration::from_millis(20), Vec::new()));
    let pending = Pending::new(outbox.clone());
    let services = Services::parse(
        &format!("[services.yajna]\nsocket = \"{}\"\n", socket.display()),
        dir.path(),
    )
    .unwrap();
    let parts = saarthi_node::link::Parts {
        sessions: sessions.clone(),
        pending: pending.clone(),
        wrappers: Wrappers::new(sessions.clone(), pending.clone(), outbox.clone()),
        spawner: saarthi_node::spawner::Spawner::new(
            "claude".into(),
            "/bin/sh".into(),
            &[],
            None,
            Default::default(),
            sessions.clone(),
        ),
        keep_awake: Default::default(),
        services: Arc::new(services),
        queue: saarthi_node::queue::Queue::new(sessions.clone()),
        uploads: std::env::temp_dir().join("saarthi-test-uploads"),
    };
    let cfg = saarthi_node::link::Config {
        hub: format!("http://{addr}").into(),
        token: "t-node".into(),
        name: "mac".into(),
        buffer_cap: 1000,
        terminal: None,
        shells: Default::default(),
    };
    tokio::spawn(saarthi_node::link::run(cfg, rx, parts));

    let core = Hub::connect(format!("http://{addr}"), "t-phone".into()).await.unwrap();
    let mut node = None;
    for _ in 0..500 {
        node = core.nodes().await.unwrap().into_iter().find(|n| n.connected);
        if node.is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let node = node.expect("the node never connected");
    assert_eq!(node.services, ["yajna"]);

    let books = core.notebooks(node.id.clone()).await.unwrap();
    assert_eq!(books.len(), 1);
    assert_eq!((books[0].namespace.as_str(), books[0].url.as_str()), ("lab", "/n/lab/"));

    // A write from a page: its body and type arrive.
    let r = core
        .service_request(
            node.id.clone(),
            "yajna".into(),
            "POST".into(),
            "/n/lab/api/reviews/r/comment".into(),
            vec![HeaderInfo { name: "content-type".into(), value: "application/json".into() }],
            Some(br#"{"text":"hi"}"#.to_vec()),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), 200);
    let mut body = Vec::new();
    while let Some(p) = r.read().await.unwrap() {
        body.extend(p);
    }
    assert_eq!(String::from_utf8(body).unwrap(), r#"application/json {"text":"hi"}"#);

    // An event stream: the first event arrives while the stream stays open, and a cancel
    // ends a read that is waiting.
    let r = Arc::new(
        core.service_request(node.id.clone(), "yajna".into(), "GET".into(), "/n/lab/api/events".into(), Vec::new(), None)
            .await
            .unwrap(),
    );
    assert!(r.headers().iter().any(|h| h.name == "content-type" && h.value == "text/event-stream"));
    let first = tokio::time::timeout(Duration::from_secs(5), r.read()).await.expect("the first event held back");
    assert_eq!(first.unwrap().unwrap(), b"data: one\n\n");
    let waiting = tokio::spawn({
        let r = r.clone();
        async move { r.read().await }
    });
    tokio::time::sleep(Duration::from_millis(100)).await;
    r.cancel();
    let ended = tokio::time::timeout(Duration::from_secs(5), waiting).await.expect("cancel did not end the read");
    assert_eq!(ended.unwrap().unwrap(), None);

    // A node the hub does not know.
    let e = core.notebooks("no-such-node".into()).await.err().unwrap();
    assert!(matches!(e, CoreError::Connection { .. } | CoreError::Hub { .. }), "{e:?}");
}
