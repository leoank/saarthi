//! Policies from two places (phase PHS-20261126-f3e51h item 8) and nodes under the same
//! engine (item 9), end to end on loopback. Stored policies are listed beside the bootstrap
//! set, apply at once, are refused when they do not fit the schema and outlive a restart;
//! the bootstrap file is read again when it changes. A node gets a Claude token, and its
//! sessions reach the API, only under the accounts the policies give it.

mod common;

use std::time::Duration;

use common::{HubProc, fake_claude_api, start_node, through_socket};
use saarthi_proto::control::v1::control_service_client::ControlServiceClient;
use saarthi_proto::control::v1::*;
use tonic::transport::Channel;

fn req<T>(body: T) -> tonic::Request<T> {
    let mut r = tonic::Request::new(body);
    r.metadata_mut().insert("authorization", "Bearer t-client".parse().unwrap());
    r
}

async fn node_id(c: &mut ControlServiceClient<Channel>) -> String {
    for _ in 0..250 {
        let nodes = c.list_nodes(req(ListNodesRequest {})).await.unwrap().into_inner().nodes;
        if let Some(n) = nodes.into_iter().find(|n| n.connected) {
            return n.id;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("no node connected");
}

const NO_SHELLS: &str = r#"@id("no-shells") forbid (principal, action == Saarthi::Action::"ListShells", resource);"#;

#[tokio::test(flavor = "multi_thread")]
async fn stored_policies_apply_at_once_are_checked_and_outlive_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("hub.db");
    let (hub, addr) = HubProc::start(&db, "127.0.0.1:0");
    let _node = start_node(&addr, "t-node", &dir.path().join(".claude"), Duration::from_secs(5)).await;
    let mut c = ControlServiceClient::connect(format!("http://{addr}")).await.unwrap();
    let node = node_id(&mut c).await;
    let shells = |c: &mut ControlServiceClient<Channel>| {
        let mut c = c.clone();
        let node = node.clone();
        async move { c.list_shells(req(ListShellsRequest { node_id: node })).await.map(|_| ()).map_err(|e| e.code()) }
    };

    let listed = c.list_policies(req(ListPoliciesRequest {})).await.unwrap().into_inner();
    assert_eq!(listed.bootstrap_source, "built in");
    assert!(listed.bootstrap.contains(r#"@id("admin")"#));
    assert!(listed.stored.is_empty());
    assert_ne!(shells(&mut c).await, Err(tonic::Code::PermissionDenied));

    // Taken, and in force for the next call.
    let set = c.set_policy(req(SetPolicyRequest { id: "no-shells".into(), text: NO_SHELLS.into() })).await.unwrap().into_inner();
    assert_eq!(set.stored.iter().map(|p| (p.id.as_str(), p.updated_by.as_str())).collect::<Vec<_>>(), [("no-shells", "test-client")]);
    assert_eq!(shells(&mut c).await, Err(tonic::Code::PermissionDenied));

    // One that does not fit the schema is refused, and nothing changes.
    let bad = SetPolicyRequest { id: "bad".into(), text: "forbid (principal, action, resource) when { resource.nope };".into() };
    let e = c.set_policy(req(bad)).await.unwrap_err();
    assert_eq!(e.code(), tonic::Code::InvalidArgument);
    assert!(e.message().starts_with("policy bad:"), "{}", e.message());
    let ids = |l: ListPoliciesResponse| l.stored.into_iter().map(|p| p.id).collect::<Vec<_>>();
    assert_eq!(ids(c.list_policies(req(ListPoliciesRequest {})).await.unwrap().into_inner()), ["no-shells"]);
    let e = c.set_policy(req(SetPolicyRequest { id: "has space".into(), text: NO_SHELLS.into() })).await.unwrap_err();
    assert_eq!(e.code(), tonic::Code::InvalidArgument);

    // It outlives a restart.
    drop(c);
    hub.stop();
    let (hub, addr) = HubProc::start(&db, "127.0.0.1:0");
    let _node = start_node(&addr, "t-node", &dir.path().join(".claude"), Duration::from_secs(5)).await;
    let mut c = ControlServiceClient::connect(format!("http://{addr}")).await.unwrap();
    let node = node_id(&mut c).await;
    let shells = |c: &mut ControlServiceClient<Channel>| {
        let mut c = c.clone();
        let node = node.clone();
        async move { c.list_shells(req(ListShellsRequest { node_id: node })).await.map(|_| ()).map_err(|e| e.code()) }
    };
    assert_eq!(shells(&mut c).await, Err(tonic::Code::PermissionDenied));

    // Removed, and the call is allowed again.
    let left = c.delete_policy(req(DeletePolicyRequest { id: "no-shells".into() })).await.unwrap().into_inner();
    assert!(left.stored.is_empty());
    assert_ne!(shells(&mut c).await, Err(tonic::Code::PermissionDenied));
    let e = c.delete_policy(req(DeletePolicyRequest { id: "no-shells".into() })).await.unwrap_err();
    assert_eq!(e.code(), tonic::Code::NotFound);
    hub.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn the_bootstrap_file_is_read_again_when_it_changes() {
    use saarthi_hub::authz::{Authz, DEFAULT_POLICIES, Res};
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("policies.cedar");
    std::fs::write(&file, DEFAULT_POLICIES).unwrap();
    let authz = std::sync::Arc::new(Authz::new(DEFAULT_POLICIES, ["phone"], []).unwrap());
    authz.set_node("n1", &[]).unwrap();
    authz.set_bootstrap(DEFAULT_POLICIES, &file.display().to_string()).unwrap();
    tokio::spawn(saarthi_hub::authz::watch_file(authz.clone(), file.clone()));
    let lists = || authz.allows("phone", "ListShells", Res::Node("n1"), serde_json::json!({}));
    assert!(lists());
    std::fs::write(&file, format!("{DEFAULT_POLICIES}\n{NO_SHELLS}")).unwrap();
    let mut waited = 0;
    while lists() && waited < 80 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        waited += 1;
    }
    assert!(!lists(), "the changed file applied");
    // A broken edit is refused; the set in force stays.
    std::fs::write(&file, "permit (principal, action, resource").unwrap();
    tokio::time::sleep(Duration::from_secs(6)).await;
    assert!(!lists());
    assert!(authz.allows("phone", "ViewNode", Res::Node("n1"), serde_json::json!({})));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_node_runs_only_under_the_accounts_policies_give_it() {
    let dir = tempfile::tempdir().unwrap();
    let not_personal = r#"
        @id("not-personal")
        forbid (principal is Saarthi::Node, action in [Saarthi::Action::"GetClaudeToken", Saarthi::Action::"UseClaudeApi"], resource)
        when { resource == Saarthi::ClaudeAccount::"personal" };
    "#;
    let policies = format!("{}\n{not_personal}", saarthi_hub::authz::DEFAULT_POLICIES);
    let (hub, addr) = HubProc::start_relaying_with_policies(
        &dir.path().join("hub.db"),
        "127.0.0.1:0",
        "work tok-work\npersonal tok-personal\n",
        &fake_claude_api().await,
        &policies,
    );
    let _node = start_node(&addr, "t-node", &dir.path().join(".claude"), Duration::from_secs(5)).await;
    let mut c = ControlServiceClient::connect(format!("http://{addr}")).await.unwrap();
    node_id(&mut c).await;
    let hub_addr: saarthi_node::link::HubAddr = format!("http://{addr}").into();
    let ask = |name: &str| saarthi_node::link::claude_token(&hub_addr, "t-node", Some(name.to_string()));
    assert!(ask("work").await.unwrap().claude_token.is_some());
    let refused = ask("personal").await.unwrap_err();
    assert!(refused.contains("not allowed: GetClaudeToken on this Claude account"), "{refused}");

    // Through a proxy to the hub, as a session's requests go (ADR-0051).
    let api = |name: &str| {
        let api = saarthi_node::claude_api::HubApi::new(hub_addr.clone(), "t-node".into());
        let socket = dir.path().join(format!("{name}.sock"));
        let proxy = saarthi_node::claude_api::Proxy::start(
            &socket,
            saarthi_node::claude_api::Upstream::Hub { api, name: name.into() },
        )
        .unwrap();
        (proxy, socket)
    };
    let (_work, socket) = api("work");
    let (status, json) = through_socket(&socket, "/v1/messages").await;
    assert!(status.contains("200"), "{status}");
    assert_eq!(json["auth"], "Bearer tok-work");
    let (_personal, socket) = api("personal");
    let (status, _) = through_socket(&socket, "/v1/messages").await;
    assert!(!status.contains("200"), "{status}");
    hub.stop();
}
