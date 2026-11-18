//! Intervention end to end on loopback: a PermissionRequest posted to a real node's hook
//! endpoint is held, reaches a control client through the hub, and is answered by that
//! client's resolution; questions get their answers as `updatedInput`; an ignored request
//! falls through to the terminal; a hub restart keeps the held request open; a prompt
//! reaches the session's messaging socket; commands the node cannot carry out come back
//! refused, and an unknown client is turned away.

mod common;

use std::time::Duration;

use common::{HubProc, post_hook, start_node};
use saarthi_proto::control::v1::control_service_client::ControlServiceClient;
use saarthi_proto::control::v1::*;
use saarthi_proto::event::v1::{
    AnswerValue, Answers, PendingKind, ReleaseReason, Resolution, resolution,
};
use serde_json::{Value, json};
use tonic::transport::Channel;

const LONG_HOLD: Duration = Duration::from_secs(30);

async fn client(addr: &str) -> ControlServiceClient<Channel> {
    ControlServiceClient::connect(format!("http://{addr}"))
        .await
        .unwrap()
}

fn req<T>(body: T) -> tonic::Request<T> {
    let mut r = tonic::Request::new(body);
    r.metadata_mut()
        .insert("authorization", "Bearer t-client".parse().unwrap());
    r
}

fn permission(session: &str) -> String {
    json!({"hook_event_name": "PermissionRequest", "session_id": session, "cwd": "/w",
           "tool_name": "Bash", "tool_input": {"command": "make deploy"}})
    .to_string()
}

/// The open requests, once there are `n` of them.
async fn pending(c: &mut ControlServiceClient<Channel>, n: usize) -> Vec<PendingOnNode> {
    for _ in 0..200 {
        let p = c
            .list_pending(req(ListPendingRequest {}))
            .await
            .unwrap()
            .into_inner()
            .pending;
        if p.len() == n {
            return p;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
    panic!("never {n} open requests");
}

fn resolve(p: &PendingOnNode, decision: resolution::Decision) -> ResolveRequest {
    ResolveRequest {
        node_id: p.node_id.clone(),
        resolution: Some(Resolution {
            pending_id: p.request.as_ref().unwrap().id.clone(),
            decision: Some(decision),
        }),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_allows_and_answers_what_the_node_holds() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, addr) = HubProc::start(&dir.path().join("hub.db"), "127.0.0.1:0");
    let node = start_node(&addr, "t-node", dir.path(), LONG_HOLD).await;
    let mut c = client(&addr).await;
    let mut watch = c.watch(req(WatchRequest {})).await.unwrap().into_inner();

    // A permission request: held until the client allows it.
    let hook = tokio::spawn(post_hook(node.hooks, permission("s1")));
    let open = pending(&mut c, 1).await;
    let request = open[0].request.clone().unwrap();
    assert_eq!(
        (request.kind(), request.tool_name.as_str()),
        (PendingKind::Permission, "Bash")
    );
    assert!(!hook.is_finished(), "answered before the client decided");
    let allow = resolve(&open[0], resolution::Decision::Allow(Default::default()));
    let r = c.resolve(req(allow)).await.unwrap().into_inner();
    assert!(r.ok, "{}", r.error);
    let reply: Value = serde_json::from_str(&hook.await.unwrap()).unwrap();
    assert_eq!(
        reply["hookSpecificOutput"]["decision"],
        json!({"behavior": "allow"})
    );
    pending(&mut c, 0).await;

    // The watcher saw it open and close as resolved.
    let mut seen = Vec::new();
    while seen.len() < 2 {
        match watch.message().await.unwrap().unwrap().body.unwrap() {
            update::Body::Pending(p) => seen.push(format!("open {}", p.request.unwrap().id)),
            update::Body::PendingClosed(c) => {
                let closed = c.closed.unwrap();
                seen.push(format!("{:?} {}", closed.reason(), closed.pending_id))
            }
            _ => {}
        }
    }
    assert_eq!(
        seen,
        [
            format!("open {}", request.id),
            format!("{:?} {}", ReleaseReason::Resolved, request.id)
        ]
    );

    // A question: answered with a label and a free-text answer.
    let question = json!({"hook_event_name": "PermissionRequest", "session_id": "s1",
        "tool_name": "AskUserQuestion", "tool_input": {"questions": [
            {"question": "Ship it?", "header": "Ship", "multiSelect": false,
             "options": [{"label": "yes"}, {"label": "no"}]}]}});
    let hook = tokio::spawn(post_hook(node.hooks, question.to_string()));
    let open = pending(&mut c, 1).await;
    assert_eq!(
        open[0].request.as_ref().unwrap().kind(),
        PendingKind::Question
    );
    let wrong = Answers {
        answers: [(
            "Ship?".to_string(),
            AnswerValue {
                values: vec!["yes".into()],
            },
        )]
        .into(),
    };
    let r = c
        .resolve(req(resolve(&open[0], resolution::Decision::Answers(wrong))))
        .await
        .unwrap()
        .into_inner();
    assert!(!r.ok && r.error.contains("no answer"), "{}", r.error);
    let right = Answers {
        answers: [(
            "Ship it?".to_string(),
            AnswerValue {
                values: vec!["after lunch".into()],
            },
        )]
        .into(),
    };
    let r = c
        .resolve(req(resolve(&open[0], resolution::Decision::Answers(right))))
        .await
        .unwrap()
        .into_inner();
    assert!(r.ok, "{}", r.error);
    let reply: Value = serde_json::from_str(&hook.await.unwrap()).unwrap();
    assert_eq!(
        reply["hookSpecificOutput"]["decision"]["updatedInput"]["answers"],
        json!({"Ship it?": "after lunch"})
    );

    // Commands the node cannot carry out yet are refused with a reason.
    let prompt = PromptRequest {
        node_id: open[0].node_id.clone(),
        session_id: "s1".into(),
        text: "hi".into(),
        queue: false,
    };
    let r = c.prompt(req(prompt)).await.unwrap().into_inner();
    assert!(!r.ok && !r.error.is_empty());

    // An unknown client is turned away.
    let mut stranger = tonic::Request::new(ListPendingRequest {});
    stranger
        .metadata_mut()
        .insert("authorization", "Bearer nope".parse().unwrap());
    let refused = c.list_pending(stranger).await.unwrap_err();
    assert_eq!(refused.code(), tonic::Code::Unauthenticated);
    hub.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn an_ignored_request_falls_through_and_a_held_one_survives_a_hub_restart() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("hub.db");
    let (hub, addr) = HubProc::start(&db, "127.0.0.1:0");
    let short = start_node(&addr, "t-node", dir.path(), Duration::from_millis(300)).await;
    let mut c = client(&addr).await;

    // Nobody decides: the dialog is left to the terminal, and resolving it later fails.
    assert_eq!(post_hook(short.hooks, permission("s1")).await, "{}");
    let gone = pending(&mut c, 0).await;
    assert!(gone.is_empty());
    let mut list = c
        .list_nodes(req(ListNodesRequest {}))
        .await
        .unwrap()
        .into_inner()
        .nodes;
    assert_eq!(list.len(), 1);
    let node_id = list.remove(0).id;
    short.task.abort();

    // A request held across a hub restart is still open afterwards and can be denied.
    let node = start_node(&addr, "t-node", dir.path(), LONG_HOLD).await;
    let hook = tokio::spawn(post_hook(node.hooks, permission("s2")));
    pending(&mut c, 1).await;
    hub.stop();
    let (hub, _) = HubProc::start(&db, &addr);
    let mut c = client(&addr).await;
    let open = pending(&mut c, 1).await;
    assert_eq!(open[0].node_id, node_id);
    let deny = resolution::Decision::Deny(saarthi_proto::event::v1::Deny {
        message: "use the staging target".into(),
    });
    let mut answered = None;
    for _ in 0..100 {
        // Until the node has re-linked, the hub cannot reach it.
        match c.resolve(req(resolve(&open[0], deny.clone()))).await {
            Ok(r) => {
                answered = Some(r.into_inner());
                break;
            }
            Err(s) if s.code() == tonic::Code::Unavailable => {
                tokio::time::sleep(Duration::from_millis(100)).await
            }
            Err(s) => panic!("{s}"),
        }
    }
    assert!(answered.unwrap().ok);
    let reply: Value = serde_json::from_str(&hook.await.unwrap()).unwrap();
    assert_eq!(
        reply["hookSpecificOutput"]["decision"],
        json!({"behavior": "deny", "message": "use the staging target"})
    );
    hub.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_prompt_reaches_the_sessions_messaging_socket() {
    use tokio::io::AsyncReadExt;
    let dir = tempfile::tempdir().unwrap();
    let (hub, addr) = HubProc::start(&dir.path().join("hub.db"), "127.0.0.1:0");
    let node = start_node(&addr, "t-node", dir.path(), LONG_HOLD).await;
    let socket = dir.path().join("s.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let hook = json!({"hook_event_name": "UserPromptSubmit", "session_id": "s1", "cwd": "/w",
                      "permission_mode": "bypassPermissions"});
    node.sessions.apply_hook(&hook).await;
    let record = saarthi_node::registry::Record {
        pid: std::process::id(),
        session_id: "s1".into(),
        cwd: "/w".into(),
        started_at: 1,
        status: "idle".into(),
        messaging_socket_path: socket.to_string_lossy().into_owned(),
        claude_dir: None,
        profile: String::new(),
    };
    node.sessions.apply_registry(&[record]).await;

    let mut c = client(&addr).await;
    let mut nodes = Vec::new();
    while !nodes.first().is_some_and(|n: &Node| n.connected) {
        tokio::time::sleep(Duration::from_millis(20)).await;
        nodes = c
            .list_nodes(req(ListNodesRequest {}))
            .await
            .unwrap()
            .into_inner()
            .nodes;
    }
    let prompt = PromptRequest {
        node_id: nodes[0].id.clone(),
        session_id: "s1".into(),
        text: "now run the linter".into(),
        queue: false,
    };
    let r = c.prompt(req(prompt)).await.unwrap().into_inner();
    assert!(r.ok, "{}", r.error);
    assert!(r.note.contains("approved at the terminal"), "{}", r.note);
    let (mut conn, _) = listener.accept().await.unwrap();
    let mut text = String::new();
    conn.read_to_string(&mut text).await.unwrap();
    let line: Value = serde_json::from_str(text.trim()).unwrap();
    assert_eq!(line["message"]["content"], "now run the linter");
    assert_eq!(line["session_id"], "s1");
    hub.stop();
}
