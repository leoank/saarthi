//! A session the hub has as live that its node no longer has, because the node's machine
//! rebooted or its disk was wiped, ends when the node registers again, so it can be resumed;
//! one the node still runs stays live.

mod common;

use std::time::Duration;

use common::{HubProc, start_node};
use saarthi_proto::control::v1::control_service_client::ControlServiceClient;
use saarthi_proto::control::v1::ListSessionsRequest;
use saarthi_proto::event::v1::Session;

fn req<T>(body: T) -> tonic::Request<T> {
    let mut r = tonic::Request::new(body);
    r.metadata_mut()
        .insert("authorization", "Bearer t-client".parse().unwrap());
    r
}

fn record(id: &str) -> saarthi_node::registry::Record {
    saarthi_node::registry::Record {
        pid: std::process::id(),
        session_id: id.into(),
        cwd: "/w".into(),
        started_at: 1,
        status: "idle".into(),
        messaging_socket_path: String::new(),
        claude_dir: None,
        profile: String::new(),
    }
}

async fn session(addr: &str, id: &str, ended: bool) -> Session {
    let mut c = ControlServiceClient::connect(format!("http://{addr}")).await.unwrap();
    for _ in 0..250 {
        let all = c
            .list_sessions(req(ListSessionsRequest { node_id: String::new(), include_ended: true }))
            .await
            .unwrap()
            .into_inner()
            .sessions;
        if let Some(s) = all.into_iter().find(|s| s.id == id && s.ended_ms.is_some() == ended) {
            return s;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("session {id} never showed with ended={ended}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_the_node_lost_ends_when_it_registers_again() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, addr) = HubProc::start(&dir.path().join("hub.db"), "127.0.0.1:0");
    let claude_dir = dir.path().join(".claude");

    let node = start_node(&addr, "t-node", &claude_dir, Duration::from_secs(30)).await;
    node.sessions.apply_registry(&[record("lost"), record("kept")]).await;
    session(&addr, "lost", false).await;
    session(&addr, "kept", false).await;
    node.task.abort();

    // The machine comes back: its node runs "kept" still, and knows nothing of "lost".
    let again = start_node(&addr, "t-node", &claude_dir, Duration::from_secs(30)).await;
    again.sessions.apply_registry(&[record("kept")]).await;
    let lost = session(&addr, "lost", true).await;
    assert!(lost.ended_ms.unwrap() > 0);
    // "kept" was reported after the registration, so it may end and come back; it ends live.
    tokio::time::sleep(Duration::from_millis(300)).await;
    session(&addr, "kept", false).await;
    hub.stop();
}
