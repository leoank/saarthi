//! A node linking to the hub over a tailnet, each through an identity of its own (ADR-0048).
//! Tests cannot reach a tailnet's control server, so this runs only when
//! `SAARTHI_TAILNET_AUTHKEY_FILE` names a file holding a reusable, pre-approved auth key;
//! both identities are ephemeral and leave the tailnet soon after the test.

mod common;

use std::sync::Arc;
use std::time::Duration;

use saarthi_hub::claude_tokens::ClaudeTokens;
use saarthi_hub::db::Db;
use saarthi_hub::live::Live;
use saarthi_hub::node_service::Hub;
use saarthi_hub::tokens::Tokens;
use saarthi_tailnet::{Options, Tailnet};

#[tokio::test(flavor = "multi_thread")]
async fn a_node_links_over_the_tailnet_and_the_hub_records_who_it_is() {
    let Ok(key) = std::env::var("SAARTHI_TAILNET_AUTHKEY_FILE") else {
        eprintln!("skipped: SAARTHI_TAILNET_AUTHKEY_FILE is not set");
        return;
    };
    let dir = tempfile::tempdir().unwrap();
    // Fresh names each run, so a dial never reaches an earlier run's machine.
    let run = uuid::Uuid::new_v4().simple().to_string()[..6].to_string();
    let (hub_host, node_host) = (format!("saarthi-test-hub-{run}"), format!("saarthi-test-node-{run}"));
    let start = |name: &str, host: &str| {
        Tailnet::start(Options {
            name: name.to_string(),
            dir: dir.path().join(name),
            hostname: host.to_string(),
            authkey: Some(saarthi_tailnet::AuthKey::File(key.clone().into())),
            ephemeral: true,
        })
    };
    let (hub_tailnet, node_tailnet) = tokio::join!(start("home", &hub_host), start("hub", &node_host));
    let (hub_tailnet, node_tailnet) = (hub_tailnet.unwrap(), node_tailnet.unwrap());

    let db = Db::open(&dir.path().join("hub.db")).await.unwrap();
    let hub = Hub {
        notifier: Arc::new(saarthi_hub::notify::Notifier::new(None, db.clone(), Vec::new())),
        db: db.clone(),
        node_tokens: Arc::new(Tokens::parse(common::TOKENS).unwrap()),
        client_tokens: Arc::new(Tokens::parse(common::CLIENT_TOKENS).unwrap()),
        claude_tokens: Arc::new(ClaudeTokens::parse("").unwrap()),
        claude_api: Arc::new(saarthi_hub::claude_api::ClaudeApi::new(saarthi_hub::claude_api::UPSTREAM)),
        live: Live::new(),
    };
    let serving = hub_tailnet.clone();
    tokio::spawn(async move { saarthi_hub::serve_tailnet(&serving, 7700, hub).await });

    saarthi_node::link::dial_via_tailnet();
    saarthi_node::link::tailnet_up(node_tailnet);
    let claude = tempfile::tempdir().unwrap();
    let _node = common::start_node(&format!("{hub_host}:7700"), "t-node", claude.path(), Duration::from_secs(5)).await;

    let deadline = tokio::time::Instant::now() + Duration::from_secs(60);
    let node = loop {
        let nodes = db.nodes().await.unwrap();
        if let Some(n) = nodes.into_iter().find(|n| !n.tailnet.is_empty()) {
            break n;
        }
        assert!(tokio::time::Instant::now() < deadline, "the node never linked over the tailnet");
        tokio::time::sleep(Duration::from_millis(200)).await;
    };
    assert_eq!(node.name, "test-node");
    assert_eq!(node.tailnet, "home");
    assert!(
        node.tailnet_peer.starts_with(&format!("{node_host} (")),
        "peer recorded as {:?}",
        node.tailnet_peer
    );
}
