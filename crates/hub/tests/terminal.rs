//! A shell on a node through the hub (ADR-0054), end to end on loopback: the node offers a
//! terminal and says so; a client opens one at a size, a command's output comes back, a
//! resize reaches the shell, and the shell exiting ends the stream with the reason. A client
//! hanging up leaves the shell running in the holder (ADR-0078).

mod common;

use std::time::Duration;

use common::{HubProc, start_node};
use saarthi_proto::control::v1::control_service_client::ControlServiceClient;
use saarthi_proto::control::v1::{KillShellRequest, ListNodesRequest, ListShellsRequest, TerminalUp};
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
    up.send(TerminalUp { node_id: node.id.clone(), r#in: Some(size(80, 24)), shell_id: String::new() }).await.unwrap();
    let mut down = c.terminal(req(ReceiverStream::new(up_rx))).await.unwrap().into_inner();
    let typed = |s: &str| TerminalUp { node_id: String::new(), r#in: Some(TerminalIn { input: s.as_bytes().to_vec(), ..Default::default() }), shell_id: String::new() };
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
    up.send(TerminalUp { node_id: String::new(), r#in: Some(size(100, 30)), shell_id: String::new() }).await.unwrap();
    up.send(typed("stty size\n")).await.unwrap();
    assert_eq!(until(&mut down, &mut seen, "30 100").await, None);
    // The shell exiting ends the stream, saying so.
    up.send(typed("exit\n")).await.unwrap();
    let ended = until(&mut down, &mut seen, "\u{0}never").await.expect("the end comes");
    assert_eq!(ended, "the shell exited");
    hub.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_hanging_up_leaves_the_shell_running() {
    let dir = tempfile::tempdir().unwrap();
    let (hub, addr) = HubProc::start(&dir.path().join("hub.db"), "127.0.0.1:0");
    let node = start_node(&addr, "t-node", &dir.path().join(".claude"), Duration::from_secs(30)).await;
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
    up.send(TerminalUp { node_id, r#in: Some(TerminalIn { cols: 80, rows: 24, ..Default::default() }), shell_id: String::new() }).await.unwrap();
    let mut down = c.terminal(req(ReceiverStream::new(up_rx))).await.unwrap().into_inner();
    let line = format!("echo started; sleep 2; touch {}\n", marker.display());
    up.send(TerminalUp { node_id: String::new(), r#in: Some(TerminalIn { input: line.into_bytes(), ..Default::default() }), shell_id: String::new() }).await.unwrap();
    let mut seen = String::new();
    let mut shell = String::new();
    while !seen.contains("started\r\n") {
        let m = down.message().await.unwrap().expect("output");
        seen.push_str(&String::from_utf8_lossy(&m.output));
        if !m.shell_id.is_empty() {
            shell = m.shell_id;
        }
    }
    assert!(!shell.is_empty(), "the node names the shell it attached to");
    // The client hangs up while the shell sleeps: the shell goes on, and the file comes.
    drop(up);
    drop(down);
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert!(marker.exists(), "the shell went with its client");
    let mut holder = saarthi_node::shells::connect(&node.shells).await.unwrap();
    let held = holder.list(saarthi_proto::shells::v1::ListRequest {}).await.unwrap().into_inner().shells;
    assert_eq!((held.len(), held[0].id.as_str(), held[0].clients), (1, shell.as_str(), 0));
    hub.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_lists_attaches_again_and_kills_a_shell() {
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
    let list = |c: &mut ControlServiceClient<tonic::transport::Channel>| {
        let node_id = node_id.clone();
        let mut c = c.clone();
        async move { c.list_shells(req(ListShellsRequest { node_id })).await.unwrap().into_inner().shells }
    };
    // Opens a terminal on `shell` (empty for a new one); returns its id, what came, and the
    // stream, read until `want`.
    async fn open(
        c: &mut ControlServiceClient<tonic::transport::Channel>,
        node_id: &str,
        shell: &str,
        typed: Option<&str>,
        want: &str,
    ) -> (String, String, mpsc::Sender<TerminalUp>, tonic::Streaming<saarthi_proto::node::v1::TerminalOut>) {
        let (up, up_rx) = mpsc::channel(8);
        let size = TerminalIn { cols: 80, rows: 24, ..Default::default() };
        up.send(TerminalUp { node_id: node_id.into(), r#in: Some(size), shell_id: shell.into() }).await.unwrap();
        let mut down = c.terminal(req(ReceiverStream::new(up_rx))).await.unwrap().into_inner();
        if let Some(t) = typed {
            let input = TerminalIn { input: t.as_bytes().to_vec(), ..Default::default() };
            up.send(TerminalUp { node_id: String::new(), r#in: Some(input), shell_id: String::new() }).await.unwrap();
        }
        let (mut id, mut seen) = (String::new(), String::new());
        tokio::time::timeout(Duration::from_secs(10), async {
            while id.is_empty() || !seen.contains(want) {
                let m = down.message().await.unwrap().expect("output");
                if !m.shell_id.is_empty() {
                    id = m.shell_id;
                }
                seen.push_str(&String::from_utf8_lossy(&m.output));
            }
        })
        .await
        .unwrap_or_else(|_| panic!("never saw {want:?} in {seen:?}"));
        (id, seen, up, down)
    }

    let (id, _, up, down) = open(&mut c, &node_id, "", Some("echo kept-$((6 * 7))\n"), "kept-42").await;
    drop((up, down));
    tokio::time::sleep(Duration::from_millis(300)).await;
    let shells = list(&mut c).await;
    assert_eq!((shells.len(), shells[0].id.as_str()), (1, id.as_str()));

    // Attached again by id: the same shell, its screen as it was.
    let (again, seen, _up, mut down) = open(&mut c, &node_id, &id, None, "kept-42").await;
    assert_eq!(again, id);
    assert!(seen.contains("kept-42"));

    // Killed from the client: the stream says so, and the shell is gone.
    let r = c.kill_shell(req(KillShellRequest { node_id: node_id.clone(), shell_id: id.clone() })).await.unwrap().into_inner();
    assert!(r.ok, "{}", r.error);
    let ended = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let m = down.message().await.unwrap().expect("the end");
            if !m.ended.is_empty() {
                return m.ended;
            }
        }
    })
    .await
    .unwrap();
    assert!(ended.starts_with("the shell exited"), "{ended}");
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(list(&mut c).await.is_empty());
    let again = c.kill_shell(req(KillShellRequest { node_id, shell_id: id })).await.unwrap().into_inner();
    assert!(!again.ok && again.error.contains("no such shell"), "{}", again.error);
    hub.stop();
}
