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
            .list_sessions(req(ListSessionsRequest { node_id: String::new(), include_ended: true, ..Default::default() }))
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

#[tokio::test(flavor = "multi_thread")]
async fn killing_a_session_its_node_no_longer_runs_ends_it() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("hub.db");
    let (hub, addr) = HubProc::start(&db_path, "127.0.0.1:0");
    let _node = start_node(&addr, "t-node", &dir.path().join(".claude"), Duration::from_secs(30)).await;
    let mut c = ControlServiceClient::connect(format!("http://{addr}")).await.unwrap();
    let node_id = loop {
        let nodes = c.list_nodes(req(saarthi_proto::control::v1::ListNodesRequest {})).await.unwrap().into_inner().nodes;
        if let Some(n) = nodes.into_iter().find(|n| n.connected) {
            break n.id;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    // The hub has it live, as after a node restarted on 0.12.0 or older.
    let db = saarthi_hub::db::Db::open(&db_path).await.unwrap();
    let ghost = Session { id: "ghost".into(), cwd: "/w".into(), started_ms: 1, pid: 99, ..Default::default() };
    db.store_session(&node_id, &ghost).await.unwrap();
    session(&addr, "ghost", false).await;

    let kill = saarthi_proto::control::v1::KillRequest { node_id: node_id.clone(), session_id: "ghost".into() };
    let r = c.kill(req(kill)).await.unwrap().into_inner();
    assert!(r.ok, "{}", r.error);
    assert!(r.note.contains("not running"), "{}", r.note);
    session(&addr, "ghost", true).await;
    hub.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_carries_its_title_and_branch_from_its_transcript() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, addr) = HubProc::start(&dir.path().join("hub.db"), "127.0.0.1:0");
    let claude_dir = dir.path().join(".claude");
    let node = start_node(&addr, "t-node", &claude_dir, Duration::from_secs(30)).await;
    let transcript = saarthi_node::sessions::transcript_path(&claude_dir, "/w", "titled");
    std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
    std::fs::write(
        &transcript,
        concat!(
            // Resumed from "earlier": its lines come first, under its id.
            r#"{"type":"user","uuid":"u0","sessionId":"earlier","message":{"role":"user","content":"before"}}"#, "\n",
            r#"{"type":"user","uuid":"u1","gitBranch":"feat/parser","message":{"role":"user","content":"fix it"}}"#, "\n",
            r#"{"type":"ai-title","aiTitle":"Fix the parser","sessionId":"titled"}"#, "\n",
            r#"{"type":"agent-name","agentName":"Parser rework","sessionId":"titled"}"#, "\n",
            r#"{"type":"assistant","uuid":"a1","message":{"id":"m1","role":"assistant","content":[{"type":"text","text":"Fixed it:\n the parser   now handles tabs."}]}}"#, "\n",
        ),
    )
    .unwrap();
    node.sessions.apply_registry(&[record("titled")]).await;
    let mut s = session(&addr, "titled", false).await;
    for _ in 0..250 {
        if !s.title.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
        s = session(&addr, "titled", false).await;
    }
    assert_eq!((s.title.as_str(), s.title_source.as_str(), s.git_branch.as_str()), ("Parser rework", "name", "feat/parser"));
    assert_eq!(s.last_reply, "Fixed it: the parser now handles tabs.");
    assert_eq!(s.parent_id, "earlier");
    // Search finds it by a word of the reply, once the index has caught up (ADR-0063).
    let mut c = ControlServiceClient::connect(format!("http://{addr}")).await.unwrap();
    let mut hits = Vec::new();
    for _ in 0..250 {
        let q = saarthi_proto::control::v1::SearchRequest { query: "handles ta".into(), node_id: String::new(), limit: 0, ..Default::default() };
        hits = c.search(req(q)).await.unwrap().into_inner().hits;
        if !hits.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert_eq!(hits.len(), 1, "never found by search");
    assert_eq!(hits[0].session.as_ref().unwrap().id, "titled");
    assert!(hits[0].snippet.contains("handles tabs"), "{}", hits[0].snippet);
    // What the hub holds of it, for About this session.
    let about = c
        .session_about(req(saarthi_proto::control::v1::SessionAboutRequest { node_id: s.node_id.clone(), session_id: "titled".into() }))
        .await
        .unwrap()
        .into_inner();
    assert!(about.transcript_lines >= 3 && about.events >= about.transcript_lines && about.bytes > 100, "{about:?}");
    assert!(about.first_ms > 0 && about.last_ms >= about.first_ms);
    // The node's next report of the session, without a title, keeps the hub's.
    let mut busy = record("titled");
    busy.status = "busy".into();
    node.sessions.apply_registry(&[busy]).await;
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(session(&addr, "titled", false).await.title, "Parser rework");
    hub.stop();
}
