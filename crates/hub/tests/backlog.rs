//! A node that comes back with a large buffer resends it on one link. It used to resend the
//! whole buffer before reading any of the hub's acks: the acks piled up until the link
//! broke, about eleven thousand events in, and the node reconnected and broke again, so a
//! node with a big backlog (rogue's ran to 26,000 after a restart re-read its transcripts)
//! flapped for days.

mod common;

use std::sync::Arc;
use std::time::Duration;

use common::HubProc;
use saarthi_hub::db::Db;
use saarthi_node::link;
use saarthi_node::outbox::Outbox;
use saarthi_node::pending::Pending;
use saarthi_node::sessions::Sessions;
use saarthi_proto::control::v1::control_service_client::ControlServiceClient;
use saarthi_proto::control::v1::{ListNodesRequest, WatchRequest, update};
use saarthi_proto::event::v1::EventSource;

const EVENTS: usize = 30_000;

fn req<T>(body: T) -> tonic::Request<T> {
    let mut r = tonic::Request::new(body);
    r.metadata_mut()
        .insert("authorization", "Bearer t-client".parse().unwrap());
    r
}

#[tokio::test(flavor = "multi_thread")]
async fn a_large_backlog_is_resent_on_one_link() {
    let dir = tempfile::tempdir().unwrap();
    // A port with no hub on it yet: the node buffers everything until there is one.
    let addr = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().to_string();
    let (outbox, rx) = Outbox::new(1024);
    let sessions = Arc::new(Sessions::new(dir.path().join(".claude"), outbox.clone(), Duration::from_millis(20), Vec::new()));
    let pending = Pending::new(outbox.clone());
    let parts = link::Parts {
        sessions: sessions.clone(),
        pending: pending.clone(),
        wrappers: saarthi_node::wrapper::Wrappers::new(sessions.clone(), pending.clone(), outbox.clone()),
        spawner: saarthi_node::spawner::Spawner::new("claude".into(), "/bin/sh".into(), &[], None, Default::default(), sessions.clone()),
        keep_awake: Default::default(),
        services: Default::default(),
        queue: saarthi_node::queue::Queue::new(sessions.clone()),
    };
    let cfg = link::Config {
        hub: format!("http://{addr}"),
        token: "t-node".into(),
        name: "test-node".into(),
        buffer_cap: 100_000,
        terminal: None,
        shells: Default::default(),
    };
    tokio::spawn(link::run(cfg, rx, parts));
    for i in 0..EVENTS {
        outbox
            .line("s1", EventSource::Transcript, "user", format!(r#"{{"type":"user","uuid":"u{i}"}}"#), format!("u{i}"))
            .await;
    }

    let db_path = dir.path().join("hub.db");
    let (hub, _) = HubProc::start(&db_path, &addr);
    let mut c = ControlServiceClient::connect(format!("http://{addr}")).await.unwrap();
    // Watch first, then wait for the node: any disconnect after it joined shows here.
    let mut watch = c.watch(req(WatchRequest {})).await.unwrap().into_inner();
    let node_id = loop {
        let nodes = c.list_nodes(req(ListNodesRequest {})).await.unwrap().into_inner().nodes;
        if let Some(n) = nodes.into_iter().find(|n| n.connected) {
            break n.id;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    };
    let dropped = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    tokio::spawn({
        let dropped = dropped.clone();
        async move {
            while let Ok(Some(u)) = watch.message().await {
                if let Some(update::Body::Node(n)) = u.body
                    && !n.connected
                {
                    dropped.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                }
            }
        }
    });

    let db = Db::open(&db_path).await.unwrap();
    let mut stored = 0;
    for _ in 0..600 {
        stored = db.session_events(&node_id, "s1", 0, EVENTS as i64 + 1).await.unwrap().len();
        if stored == EVENTS {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(stored, EVENTS, "the backlog never arrived whole");
    assert_eq!(dropped.load(std::sync::atomic::Ordering::SeqCst), 0, "the link broke while resending");
    hub.stop();
}
