//! Two nodes built as a library (ADR-0080) in one process, against a real hub on loopback:
//! each registers with a token of its own and lists only the sessions it starts; those run
//! in the node's own Claude config directory, only under the hub's Claude tokens, through a
//! proxy whose socket is in the node's state directory; an app's labels come back with the
//! session and find it (phase item 5); stopping a node ends its sessions.

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::time::Duration;

use common::{HubProc, fake_claude_api, through_socket};
use saarthi_node::node::{Hub, Node, Options};
use saarthi_proto::control::v1::control_service_client::ControlServiceClient;
use saarthi_proto::control::v1::*;
use tonic::transport::Channel;

/// Records where it runs and under what, then answers each `user` line with a `result`;
/// exits on EOF.
const FAKE: &str = r#"#!/bin/sh
echo "config=${CLAUDE_CONFIG_DIR:-none} token=${CLAUDE_CODE_OAUTH_TOKEN:-none} socket=${ANTHROPIC_UNIX_SOCKET:-none}" >> LOG
while IFS= read -r line; do
  case "$line" in *'"type":"user"'*) echo '{"type":"result","subtype":"success"}' ;; esac
done
"#;

fn req<T>(body: T) -> tonic::Request<T> {
    let mut r = tonic::Request::new(body);
    r.metadata_mut().insert("authorization", "Bearer t-client".parse().unwrap());
    r
}

async fn node(state: &Path, name: &str, token: &str, hub: &str, claude: &Path, root: &Path) -> Node {
    let mut o = Options::new(name, state.to_path_buf());
    o.hub = Some(Hub::direct(format!("http://{hub}"), token));
    o.claude = claude.to_string_lossy().into_owned();
    o.shell = "/bin/sh".into();
    o.spawn_roots = vec![root.to_path_buf()];
    o.user_claude = state.join("no-user-claude");
    o.transcript_poll = Duration::from_millis(20);
    Node::start(o).await.unwrap()
}

async fn node_ids(c: &mut ControlServiceClient<Channel>) -> (String, String) {
    for _ in 0..250 {
        let nodes = c.list_nodes(req(ListNodesRequest {})).await.unwrap().into_inner().nodes;
        let id = |name: &str| nodes.iter().find(|n| n.name == name && n.connected).map(|n| n.id.clone());
        // The hub names a node by its token, whatever it registers as.
        if let (Some(a), Some(b)) = (id("test-node"), id("other-node")) {
            return (a, b);
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the two nodes never both connected");
}

/// The nodes that list session `id`, and whether it has ended on each.
async fn listed(c: &mut ControlServiceClient<Channel>, id: &str) -> Vec<(String, bool)> {
    c.list_sessions(req(ListSessionsRequest { node_id: String::new(), include_ended: true, labels: Default::default() }))
        .await
        .unwrap()
        .into_inner()
        .sessions
        .into_iter()
        .filter(|s| s.id == id)
        .map(|s| (s.node_id, s.ended_ms.is_some()))
        .collect()
}

async fn wait_for(mut check: impl AsyncFnMut() -> bool, what: &str) {
    for _ in 0..250 {
        if check().await {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("never: {what}");
}

#[tokio::test(flavor = "multi_thread")]
async fn two_embedded_nodes_in_one_process_keep_to_their_own_sessions() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("work");
    std::fs::create_dir_all(root.join("repo")).unwrap();
    let log = dir.path().join("starts.log");
    let fake = dir.path().join("claude");
    std::fs::write(&fake, FAKE.replace("LOG", &format!("'{}'", log.display()))).unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (hub, addr) =
        HubProc::start_relaying(&dir.path().join("hub.db"), "127.0.0.1:0", "work tok-work\n", &fake_claude_api().await);

    let (sa, sb) = (dir.path().join("a"), dir.path().join("b"));
    let a = node(&sa, "app-a", "t-node", &addr, &fake, &root).await;
    let b = node(&sb, "app-b", "t-node-2", &addr, &fake, &root).await;
    let mut c = ControlServiceClient::connect(format!("http://{addr}")).await.unwrap();
    let (ida, idb) = node_ids(&mut c).await;
    let spawn = |claude_token: Option<&str>| SpawnRequest {
        node_id: ida.clone(),
        cwd: root.canonicalize().unwrap().join("repo").to_string_lossy().into_owned(),
        prompt: "hello".into(),
        model: String::new(),
        create: false,
        permission_mode: String::new(),
        profile: String::new(),
        claude_token: claude_token.map(str::to_string),
        // An app's own labels, to find its work again (phase item 5).
        labels: [("job".to_string(), "1234".to_string())].into(),
    };

    // Its config dir has no login: without a hub token there is nothing to run under.
    let r = c.spawn(req(spawn(None))).await.unwrap().into_inner();
    assert!(!r.ok);
    assert!(r.error.contains("only under the hub's Claude tokens"), "{}", r.error);
    assert!(!log.exists(), "nothing started");

    let r = c.spawn(req(spawn(Some("work")))).await.unwrap().into_inner();
    assert!(r.ok, "{}", r.error);
    let id = r.session_id;
    wait_for(async || log.exists(), "the session started").await;
    let start = std::fs::read_to_string(&log).unwrap();
    let start = start.lines().next().unwrap();
    assert!(start.starts_with(&format!("config={} token=saarthi-proxied socket=", sa.join("claude").display())), "{start}");
    let socket = Path::new(start.rsplit_once("socket=").unwrap().1);
    assert_eq!(socket.parent().unwrap(), sa.join("api"));
    let (status, json) = through_socket(socket, "/v1/messages?beta=true").await;
    assert!(status.contains("200"), "{status}");
    assert_eq!(json["auth"], "Bearer tok-work");

    // Listed under the node that started it and no other.
    wait_for(async || listed(&mut c, &id).await == [(ida.clone(), false)], "listed under a alone").await;
    assert_ne!(ida, idb);

    // With the app's labels, and found by them; launched by the app's client.
    let by_label = |job: &str| ListSessionsRequest {
        node_id: String::new(),
        include_ended: true,
        labels: [("job".to_string(), job.to_string())].into(),
    };
    let found = c.list_sessions(req(by_label("1234"))).await.unwrap().into_inner().sessions;
    assert_eq!(found.iter().map(|s| s.id.as_str()).collect::<Vec<_>>(), [id.as_str()]);
    assert_eq!(found[0].labels["job"], "1234");
    assert_eq!(found[0].launched_by, "client:test-client");
    assert!(c.list_sessions(req(by_label("9"))).await.unwrap().into_inner().sessions.is_empty());
    let bad = SpawnRequest { labels: [("no spaces".to_string(), String::new())].into(), ..spawn(Some("work")) };
    let e = c.spawn(req(bad)).await.unwrap_err();
    assert_eq!(e.code(), tonic::Code::InvalidArgument, "{e}");

    // Stopping a ends its session; b goes on.
    a.stop().await;
    wait_for(async || listed(&mut c, &id).await == [(ida.clone(), true)], "the session ended").await;
    let nodes = c.list_nodes(req(ListNodesRequest {})).await.unwrap().into_inner().nodes;
    assert!(nodes.iter().any(|n| n.id == idb && n.connected));
    b.stop().await;
    hub.stop();
}
