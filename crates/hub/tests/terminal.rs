//! A shell on a node through the hub (ADR-0054), end to end on loopback: the node offers a
//! terminal and says so; a client opens one at a size, a command's output comes back, a
//! resize reaches the shell, and the shell exiting ends the stream with the reason.

mod common;

use std::time::Duration;

use common::{HubProc, start_node};
use saarthi_proto::control::v1::control_service_client::ControlServiceClient;
use saarthi_proto::control::v1::{ListNodesRequest, TerminalUp};
use saarthi_proto::node::v1::TerminalIn;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;

fn req<T>(body: T) -> tonic::Request<T> {
    let mut r = tonic::Request::new(body);
    r.metadata_mut().insert("authorization", "Bearer t-client".parse().unwrap());
    r
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_runs_a_shell_on_a_node() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, addr) = HubProc::start(&dir.path().join("hub.db"), "127.0.0.1:0");
    let _node = start_node(&addr, "t-node", &dir.path().join(".claude"), Duration::from_secs(30)).await;
    let mut c = ControlServiceClient::connect(format!("http://{addr}")).await.unwrap();
    let node = loop {
        let nodes = c.list_nodes(req(ListNodesRequest {})).await.unwrap().into_inner().nodes;
        if let Some(n) = nodes.into_iter().find(|n| n.connected) {
            break n;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    assert!(node.terminal, "the node offers a terminal");

    let (up, up_rx) = mpsc::channel(8);
    let size = |cols, rows| TerminalIn { cols, rows, ..Default::default() };
    up.send(TerminalUp { node_id: node.id.clone(), r#in: Some(size(80, 24)) }).await.unwrap();
    let mut down = c.terminal(req(ReceiverStream::new(up_rx))).await.unwrap().into_inner();
    let typed = |s: &str| TerminalUp { node_id: String::new(), r#in: Some(TerminalIn { input: s.as_bytes().to_vec(), ..Default::default() }) };
    let mut seen = String::new();
    // Reads until `want` has come, or the shell ended; the end's reason is returned.
    async fn until(down: &mut tonic::Streaming<saarthi_proto::node::v1::TerminalOut>, seen: &mut String, want: &str) -> Option<String> {
        let wait = tokio::time::timeout(Duration::from_secs(10), async {
            while !seen.contains(want) {
                let m = down.message().await.unwrap()?;
                seen.push_str(&String::from_utf8_lossy(&m.output));
                if !m.ended.is_empty() {
                    return Some(m.ended);
                }
            }
            None
        });
        wait.await.unwrap_or_else(|_| panic!("never saw {want:?} in {seen:?}"))
    }

    up.send(typed("echo hello-$((40 + 2))\n")).await.unwrap();
    assert_eq!(until(&mut down, &mut seen, "hello-42").await, None);
    // A resize reaches the shell's terminal.
    up.send(TerminalUp { node_id: String::new(), r#in: Some(size(100, 30)) }).await.unwrap();
    up.send(typed("stty size\n")).await.unwrap();
    assert_eq!(until(&mut down, &mut seen, "30 100").await, None);
    // The shell exiting ends the stream, saying so.
    up.send(typed("exit\n")).await.unwrap();
    let ended = until(&mut down, &mut seen, "\u{0}never").await.expect("the end comes");
    assert_eq!(ended, "the shell exited");
    hub.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_hanging_up_ends_the_shell() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, addr) = HubProc::start(&dir.path().join("hub.db"), "127.0.0.1:0");
    let _node = start_node(&addr, "t-node", &dir.path().join(".claude"), Duration::from_secs(30)).await;
    let mut c = ControlServiceClient::connect(format!("http://{addr}")).await.unwrap();
    let node_id = loop {
        let nodes = c.list_nodes(req(ListNodesRequest {})).await.unwrap().into_inner().nodes;
        if let Some(n) = nodes.into_iter().find(|n| n.connected) {
            break n.id;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    let marker = dir.path().join("still-running");
    let (up, up_rx) = mpsc::channel(8);
    up.send(TerminalUp { node_id, r#in: Some(TerminalIn { cols: 80, rows: 24, ..Default::default() }) }).await.unwrap();
    let mut down = c.terminal(req(ReceiverStream::new(up_rx))).await.unwrap().into_inner();
    let line = format!("echo started; sleep 2; touch {}\n", marker.display());
    up.send(TerminalUp { node_id: String::new(), r#in: Some(TerminalIn { input: line.into_bytes(), ..Default::default() }) }).await.unwrap();
    let mut seen = String::new();
    while !seen.contains("started\r\n") {
        let m = down.message().await.unwrap().expect("output");
        seen.push_str(&String::from_utf8_lossy(&m.output));
    }
    // The client hangs up while the shell sleeps: the shell goes, the file never comes.
    drop(up);
    drop(down);
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert!(!marker.exists(), "the shell outlived its client");
    hub.stop();
}
