//! Phase 2 step 1 end to end on loopback (ADR-0034): a client spawns a session on a node
//! allowed to, with a stand-in for claude; the session is listed as spawned under the id
//! the spawn returned, takes a prompt and a kill over its input and ends; spawning outside
//! the node's roots, with an unknown permission mode, and on a node without roots is
//! refused; keep-awake is switched. A node's launch profiles reach clients, and a session
//! spawned under one is listed with it (ADR-0038).

mod common;

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use common::{HubProc, start_node, start_spawning_node};
use saarthi_node::containment::Profiles;
use saarthi_node::spawner::Contained;
use saarthi_proto::control::v1::control_service_client::ControlServiceClient;
use saarthi_proto::control::v1::*;
use saarthi_proto::event::v1::{Launch, Session};
use tonic::transport::Channel;

const HOLD: Duration = Duration::from_secs(30);

/// A profile that runs as the node's user, so the test runs where seatbelt does not.
const PROFILES: &str = r#"
[profiles.plain]
backend = "host"
description = "As the node's user, under a name"
"#;

/// Logs every input line and answers each `user` line with a `result`; exits on EOF.
const FAKE: &str = r#"#!/bin/sh
while IFS= read -r line; do
  echo "$line" >> LOG
  case "$line" in
    *'"type":"user"'*) echo '{"type":"result","subtype":"success"}' ;;
  esac
done
"#;

fn req<T>(body: T) -> tonic::Request<T> {
    let mut r = tonic::Request::new(body);
    r.metadata_mut()
        .insert("authorization", "Bearer t-client".parse().unwrap());
    r
}

async fn connected(c: &mut ControlServiceClient<Channel>) -> Node {
    loop {
        let nodes = c
            .list_nodes(req(ListNodesRequest {}))
            .await
            .unwrap()
            .into_inner()
            .nodes;
        if let Some(n) = nodes.into_iter().find(|n| n.connected) {
            return n;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

async fn session(c: &mut ControlServiceClient<Channel>, id: &str, ended: bool) -> Session {
    for _ in 0..250 {
        let all = c
            .list_sessions(req(ListSessionsRequest {
                node_id: String::new(),
                include_ended: true,
            }))
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

async fn log_has(log: &std::path::Path, text: &str) {
    for _ in 0..250 {
        if std::fs::read_to_string(log).unwrap_or_default().contains(text) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("{text:?} never reached the fake claude");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_spawns_prompts_and_kills_a_headless_session() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("work");
    std::fs::create_dir_all(root.join("repo")).unwrap();
    let log = dir.path().join("input.log");
    let fake = dir.path().join("claude");
    std::fs::write(&fake, FAKE.replace("LOG", &format!("'{}'", log.display()))).unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();

    let (hub, addr) = HubProc::start(&dir.path().join("hub.db"), "127.0.0.1:0");
    let _node = start_spawning_node(
        &addr,
        "t-node",
        &dir.path().join(".claude"),
        HOLD,
        &fake.to_string_lossy(),
        std::slice::from_ref(&root),
        Contained {
            profiles: Profiles::parse(PROFILES, dir.path()).unwrap(),
            ..Default::default()
        },
    )
    .await;
    let mut c = ControlServiceClient::connect(format!("http://{addr}"))
        .await
        .unwrap();
    let node = connected(&mut c).await;
    let root = root.canonicalize().unwrap();
    assert_eq!(node.spawn_roots, vec![root.to_string_lossy().into_owned()]);
    assert_eq!(node.profiles.len(), 1);
    assert_eq!(node.profiles[0].name, "plain");
    assert_eq!(node.profiles[0].description, "As the node's user, under a name");
    assert!(!node.profiles[0].sandboxed);

    let cwd = root.join("repo").to_string_lossy().into_owned();
    let spawn = SpawnRequest {
        node_id: node.id.clone(),
        cwd: cwd.clone(),
        prompt: "write the tests".into(),
        model: String::new(),
        create: false,
        permission_mode: String::new(),
        profile: "plain".into(),
        claude_token: None,
    };
    let r = c.spawn(req(spawn)).await.unwrap().into_inner();
    assert!(r.ok, "{}", r.error);
    assert!(uuid::Uuid::parse_str(&r.session_id).is_ok(), "{}", r.session_id);
    let id = r.session_id;
    log_has(&log, "write the tests").await;
    let s = session(&mut c, &id, false).await;
    assert_eq!(s.launch(), Launch::Spawned);
    assert_eq!(s.cwd, cwd);
    assert_eq!(s.profile, "plain");

    let prompt = PromptRequest {
        node_id: node.id.clone(),
        session_id: id.clone(),
        text: "and run them".into(),
    };
    let r = c.prompt(req(prompt)).await.unwrap().into_inner();
    assert!(r.ok && r.note.is_empty(), "{} {}", r.error, r.note);
    log_has(&log, "and run them").await;

    let kill = KillRequest {
        node_id: node.id.clone(),
        session_id: id.clone(),
    };
    let r = c.kill(req(kill)).await.unwrap().into_inner();
    assert!(r.ok, "{}", r.error);
    session(&mut c, &id, true).await;

    // Outside the roots: refused, with the reason.
    let outside = SpawnRequest {
        node_id: node.id.clone(),
        cwd: dir.path().to_string_lossy().into_owned(),
        prompt: "hi".into(),
        model: String::new(),
        create: false,
        permission_mode: String::new(),
        profile: String::new(),
        claude_token: None,
    };
    let r = c.spawn(req(outside)).await.unwrap().into_inner();
    assert!(!r.ok && r.error.contains("spawn root"), "{}", r.error);
    assert!(r.session_id.is_empty());

    // A permission mode claude does not list: refused, so no other flag gets through.
    let flag = SpawnRequest {
        node_id: node.id.clone(),
        cwd: cwd.clone(),
        prompt: "hi".into(),
        model: String::new(),
        create: false,
        permission_mode: "--yolo".into(),
        profile: String::new(),
        claude_token: None,
    };
    let r = c.spawn(req(flag)).await.unwrap().into_inner();
    assert!(!r.ok && r.error.contains("permission mode"), "{}", r.error);

    // A launch profile the node does not have: refused by the node, the name carried there.
    let unknown = SpawnRequest {
        node_id: node.id.clone(),
        cwd: cwd.clone(),
        prompt: "hi".into(),
        model: String::new(),
        create: false,
        permission_mode: String::new(),
        profile: "boxed".into(),
        claude_token: None,
    };
    let r = c.spawn(req(unknown)).await.unwrap().into_inner();
    assert!(!r.ok && r.error.contains("no launch profile boxed"), "{}", r.error);

    let on = SetKeepAwakeRequest {
        node_id: node.id.clone(),
        on: true,
    };
    let r = c.set_keep_awake(req(on)).await.unwrap().into_inner();
    assert!(r.ok, "{}", r.error);
    hub.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_node_without_spawn_roots_refuses_to_spawn() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, addr) = HubProc::start(&dir.path().join("hub.db"), "127.0.0.1:0");
    let _node = start_node(&addr, "t-node", dir.path(), HOLD).await;
    let mut c = ControlServiceClient::connect(format!("http://{addr}"))
        .await
        .unwrap();
    let node = connected(&mut c).await;
    assert!(node.spawn_roots.is_empty());
    let spawn = SpawnRequest {
        node_id: node.id,
        cwd: dir.path().to_string_lossy().into_owned(),
        prompt: "hi".into(),
        model: String::new(),
        create: false,
        permission_mode: String::new(),
        profile: String::new(),
        claude_token: None,
    };
    let r = c.spawn(req(spawn)).await.unwrap().into_inner();
    assert!(!r.ok && r.error.contains("--spawn-root"), "{}", r.error);
    hub.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn history_lists_a_session_with_its_first_prompt_and_usage_and_it_resumes_in_place() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("work");
    std::fs::create_dir_all(root.join("repo")).unwrap();
    let log = dir.path().join("input.log");
    let fake = dir.path().join("claude");
    std::fs::write(&fake, FAKE.replace("LOG", &format!("'{}'", log.display()))).unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let claude_dir = dir.path().join(".claude");

    let (hub, addr) = HubProc::start(&dir.path().join("hub.db"), "127.0.0.1:0");
    let _node = start_spawning_node(
        &addr,
        "t-node",
        &claude_dir,
        HOLD,
        &fake.to_string_lossy(),
        std::slice::from_ref(&root),
        Default::default(),
    )
    .await;
    let mut c = ControlServiceClient::connect(format!("http://{addr}"))
        .await
        .unwrap();
    let node = connected(&mut c).await;
    let cwd = root.canonicalize().unwrap().join("repo").to_string_lossy().into_owned();
    let spawn = SpawnRequest {
        node_id: node.id.clone(),
        cwd: cwd.clone(),
        prompt: String::new(),
        model: String::new(),
        create: false,
        permission_mode: String::new(),
        profile: String::new(),
        claude_token: None,
    };
    let id = c.spawn(req(spawn)).await.unwrap().into_inner().session_id;

    // What claude would write: a typed prompt and a reply that used tokens.
    let transcript = saarthi_node::sessions::transcript_path(&claude_dir, &cwd, &id);
    std::fs::create_dir_all(transcript.parent().unwrap()).unwrap();
    let lines = [
        serde_json::json!({"type": "user", "uuid": "u1", "message": {"role": "user", "content": "write the tests"}}),
        serde_json::json!({"type": "assistant", "uuid": "a1", "message": {"id": "msg_1", "model": "claude-haiku-4-5",
            "role": "assistant", "content": [{"type": "text", "text": "done"}],
            "usage": {"input_tokens": 100, "output_tokens": 20}}}),
    ];
    let text: String = lines.iter().map(|l| format!("{l}\n")).collect();
    std::fs::write(&transcript, text).unwrap();

    let kill = KillRequest {
        node_id: node.id.clone(),
        session_id: id.clone(),
    };
    assert!(c.kill(req(kill)).await.unwrap().into_inner().ok);
    session(&mut c, &id, true).await;

    let mut found = None;
    for _ in 0..250 {
        let h = c
            .history(req(HistoryRequest {
                node_id: String::new(),
                before_ms: 0,
                limit: 10,
            }))
            .await
            .unwrap()
            .into_inner()
            .sessions;
        if let Some(h) = h.into_iter().find(|h| {
            h.session.as_ref().is_some_and(|s| s.id == id) && !h.usage_json.is_empty() && !h.opening.is_empty()
        }) {
            found = Some(h);
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let h = found.expect("the session in history with its opening and usage");
    assert!(h.opening[0].payload_json.contains("write the tests"));
    let usage: serde_json::Value = serde_json::from_str(&h.usage_json).unwrap();
    assert_eq!(usage["claude-haiku-4-5"]["output_tokens"], 20);

    let resume = ResumeRequest {
        node_id: node.id.clone(),
        session_id: id.clone(),
        prompt: "and run them".into(),
        model: String::new(),
        claude_token: None,
    };
    let r = c.resume(req(resume.clone())).await.unwrap().into_inner();
    assert!(r.ok, "{}", r.error);
    log_has(&log, "and run them").await;
    let s = session(&mut c, &id, false).await;
    assert_eq!(s.launch(), Launch::Spawned);
    // Live again: a second resume is turned away by the hub.
    let again = c.resume(req(resume)).await.unwrap_err();
    assert_eq!(again.code(), tonic::Code::FailedPrecondition);
    hub.stop();
}

