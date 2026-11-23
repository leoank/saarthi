//! The shell holder (ADR-0078) on a socket of its own: a new shell, detaching and attaching
//! again with the screen restored, two clients at once seeing the same, the size following
//! the last client to attach, the list, and a kill.

use std::time::Duration;

use saarthi_proto::shells::v1::shells_client::ShellsClient;
use saarthi_proto::shells::v1::{AttachIn, AttachOut, KillRequest, ListRequest, Open, Resize, attach_in, attach_out};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;

struct Client {
    up: mpsc::Sender<AttachIn>,
    down: tonic::Streaming<AttachOut>,
    id: String,
    started: bool,
    seen: String,
}

async fn attach(c: &mut ShellsClient<Channel>, id: &str, cols: u32, rows: u32) -> Client {
    let (up, rx) = mpsc::channel(16);
    let size = Resize { cols, rows, ..Default::default() };
    up.send(AttachIn { body: Some(attach_in::Body::Open(Open { id: id.into(), program: "/bin/sh".into(), size: Some(size) })) })
        .await
        .unwrap();
    let mut down = c.attach(ReceiverStream::new(rx)).await.unwrap().into_inner();
    let Some(attach_out::Body::Attached(a)) = down.message().await.unwrap().unwrap().body else { panic!("Attached comes first") };
    Client { up, down, id: a.id, started: a.started, seen: String::new() }
}

impl Client {
    async fn typed(&self, s: &str) {
        self.up.send(AttachIn { body: Some(attach_in::Body::Input(s.as_bytes().to_vec())) }).await.unwrap();
    }

    /// Reads until `want` has come; the reason, if the shell ended first.
    async fn until(&mut self, want: &str) -> Option<String> {
        let wait = tokio::time::timeout(Duration::from_secs(10), async {
            while !self.seen.contains(want) {
                match self.down.message().await.unwrap()?.body {
                    Some(attach_out::Body::Output(b)) => self.seen.push_str(&String::from_utf8_lossy(&b)),
                    Some(attach_out::Body::Ended(why)) => return Some(why),
                    _ => {}
                }
            }
            None
        });
        wait.await.unwrap_or_else(|_| panic!("never saw {want:?} in {:?}", self.seen))
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn shells_outlive_their_clients_and_come_back_as_they_were() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("shells.sock");
    let s = socket.clone();
    tokio::spawn(async move { saarthi_node::shells::serve(&s).await });
    let mut c = loop {
        if let Ok(c) = saarthi_node::shells::connect(&socket).await {
            break c;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };

    // A new shell, and output from it.
    let mut a = attach(&mut c, "", 80, 24).await;
    assert!(a.started);
    a.typed("echo first-$((20 + 1))\n").await;
    assert_eq!(a.until("first-21").await, None);
    let id = a.id.clone();

    // The client goes; the shell does not.
    drop(a);
    tokio::time::sleep(Duration::from_millis(200)).await;
    let list = c.list(ListRequest {}).await.unwrap().into_inner().shells;
    assert_eq!(list.len(), 1);
    assert_eq!((list[0].id.as_str(), list[0].clients), (id.as_str(), 0));

    // Attaching again: the screen as it was, then the shell as it goes on.
    let mut b = attach(&mut c, &id, 100, 30).await;
    assert!(!b.started);
    assert_eq!(b.until("first-21").await, None, "the screen came back");
    b.typed("stty size\n").await;
    assert_eq!(b.until("30 100").await, None, "the shell took the new client's size");

    // A second client at once: each sees what the other types.
    let mut d = attach(&mut c, &id, 100, 30).await;
    d.typed("echo from-$((1 + 1))-d\n").await;
    assert_eq!(b.until("from-2-d").await, None);
    assert_eq!(d.until("from-2-d").await, None);
    assert_eq!(c.list(ListRequest {}).await.unwrap().into_inner().shells[0].clients, 2);

    // A kill ends it for both, saying so.
    assert!(c.kill(KillRequest { id: id.clone() }).await.unwrap().into_inner().killed);
    assert!(b.until("\u{0}never").await.is_some());
    assert!(d.until("\u{0}never").await.is_some());
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(c.list(ListRequest {}).await.unwrap().into_inner().shells.is_empty());
    assert!(!c.kill(KillRequest { id }).await.unwrap().into_inner().killed);
}
