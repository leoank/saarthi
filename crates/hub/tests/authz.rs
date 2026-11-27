//! Authorization with Cedar end to end (ADR-0081, phase PHS-20261126-f3e51h item 7): a hub
//! whose policies add to the defaults that no session labelled keep=true may be killed,
//! that sessions labelled hidden=true cannot be read, and that no one lists shells. A refused
//! call says which action; lists and watch leave out what may not be read; everything else
//! works as before.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::time::Duration;

use common::{HubProc, start_spawning_node};
use saarthi_proto::control::v1::control_service_client::ControlServiceClient;
use saarthi_proto::control::v1::*;
use tokio_stream::StreamExt;

const FAKE: &str = r#"#!/bin/sh
while IFS= read -r line; do
  case "$line" in *'"type":"user"'*) echo '{"type":"result","subtype":"success"}' ;; esac
done
"#;

const POLICIES: &str = r#"
@id("keep")
forbid (principal, action in [Saarthi::Action::"Kill", Saarthi::Action::"DeleteSession"], resource is Saarthi::Session)
when { resource.hasTag("keep") && resource.getTag("keep") == "true" };

@id("hidden")
forbid (principal, action in Saarthi::Action::"read", resource is Saarthi::Session)
when { resource.hasTag("hidden") && resource.getTag("hidden") == "true" };

@id("no-shells")
forbid (principal, action == Saarthi::Action::"ListShells", resource);
"#;

fn req<T>(body: T) -> tonic::Request<T> {
    let mut r = tonic::Request::new(body);
    r.metadata_mut().insert("authorization", "Bearer t-client".parse().unwrap());
    r
}

#[tokio::test(flavor = "multi_thread")]
async fn policies_refuse_calls_and_hide_what_may_not_be_read() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("work");
    std::fs::create_dir_all(&root).unwrap();
    let fake = dir.path().join("claude");
    std::fs::write(&fake, FAKE).unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let policies = format!("{}\n{POLICIES}", saarthi_hub::authz::DEFAULT_POLICIES);
    let (hub, addr) = HubProc::start_with_policies(&dir.path().join("hub.db"), "127.0.0.1:0", &policies);
    let _node = start_spawning_node(
        &addr,
        "t-node",
        &dir.path().join(".claude"),
        Duration::from_secs(30),
        &fake.to_string_lossy(),
        std::slice::from_ref(&root),
        Default::default(),
    )
    .await;
    let mut c = ControlServiceClient::connect(format!("http://{addr}")).await.unwrap();
    let node = loop {
        let nodes = c.list_nodes(req(ListNodesRequest {})).await.unwrap().into_inner().nodes;
        if let Some(n) = nodes.into_iter().find(|n| n.connected) {
            break n.id;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    let mut watch = c.watch(req(WatchRequest::default())).await.unwrap().into_inner();
    let spawn = |label: &str| SpawnRequest {
        node_id: node.clone(),
        cwd: root.canonicalize().unwrap().to_string_lossy().into_owned(),
        prompt: "hello".into(),
        claude_token: Some(String::new()),
        labels: [(label.to_string(), "true".to_string())].into(),
        ..Default::default()
    };
    let kept = c.spawn(req(spawn("keep"))).await.unwrap().into_inner().session_id;
    let hidden = c.spawn(req(spawn("hidden"))).await.unwrap().into_inner().session_id;
    assert!(!kept.is_empty() && !hidden.is_empty());

    // The watch sends the kept session and nothing of the hidden one.
    let mut seen = std::collections::HashSet::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(3);
    while let Ok(Some(Ok(u))) = tokio::time::timeout_at(deadline, watch.next()).await {
        if let Some(update::Body::Session(s)) = u.body {
            seen.insert(s.id);
        }
    }
    assert!(seen.contains(&kept), "{seen:?}");
    assert!(!seen.contains(&hidden), "{seen:?}");

    // Nor does a list.
    let listed: Vec<String> = c
        .list_sessions(req(ListSessionsRequest { include_ended: true, ..Default::default() }))
        .await
        .unwrap()
        .into_inner()
        .sessions
        .into_iter()
        .map(|s| s.id)
        .collect();
    assert!(listed.contains(&kept) && !listed.contains(&hidden), "{listed:?}");

    // Reading it is refused, naming the action.
    let Err(e) = c
        .stream_session(req(StreamSessionRequest { node_id: node.clone(), session_id: hidden.clone(), ..Default::default() }))
        .await
    else {
        panic!("streamed a hidden session");
    };
    assert_eq!(e.code(), tonic::Code::PermissionDenied);
    assert_eq!(e.message(), "not allowed: StreamSession on this session");

    // The kept one may be prompted, not killed.
    let r = c
        .prompt(req(PromptRequest { node_id: node.clone(), session_id: kept.clone(), text: "more".into(), ..Default::default() }))
        .await
        .unwrap()
        .into_inner();
    assert!(r.ok, "{}", r.error);
    let e = c.kill(req(KillRequest { node_id: node.clone(), session_id: kept.clone() })).await.unwrap_err();
    assert_eq!(e.code(), tonic::Code::PermissionDenied);
    assert_eq!(e.message(), "not allowed: Kill on this session");

    // A node's shells: no one lists them.
    let e = c.list_shells(req(ListShellsRequest { node_id: node.clone() })).await.unwrap_err();
    assert_eq!(e.message(), "not allowed: ListShells on this node");

    // Everything else as before: the hidden one can be killed, being unreadable is not
    // being unkillable.
    assert!(c.kill(req(KillRequest { node_id: node.clone(), session_id: hidden.clone() })).await.unwrap().into_inner().ok);
    hub.stop();
}
