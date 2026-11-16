//! The service relay end to end on loopback (ADR-0052, ADR-0057): a client's request to a
//! node's local service on a unix socket, through the hub and the node, and the answer back
//! as it streams; a request body; an answer larger than a piece; the node's services in its
//! listing; an unknown service and node refused; a service started on the first request and
//! stopped when idle.

mod common;

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::routing::{get, post};
use common::{HubProc, start_serving_node};
use saarthi_node::services::Services;
use saarthi_proto::control::v1::control_service_client::ControlServiceClient;
use saarthi_proto::control::v1::*;
use saarthi_proto::node::v1::{HttpHeader, ServiceRequestHead};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Channel;

const BIG: usize = 3 * 1024 * 1024 + 17;

/// The service: what a request saw, an echo, a streamed answer, a large one.
fn app() -> axum::Router {
    axum::Router::new()
        .route(
            "/seen",
            get(|h: axum::http::HeaderMap, uri: axum::http::Uri| async move {
                let get = |k: &str| h.get(k).and_then(|v| v.to_str().ok()).unwrap_or("").to_string();
                format!("host={} x={} query={}", get("host"), get("x-test"), uri.query().unwrap_or(""))
            }),
        )
        .route("/echo", post(|body: axum::body::Bytes| async move { body }))
        .route(
            "/stream",
            get(|| async {
                // One line now, the next a second later: the first must arrive on its own.
                let (tx, rx) = mpsc::channel::<Result<String, std::io::Error>>(2);
                tokio::spawn(async move {
                    let _ = tx.send(Ok("one\n".into())).await;
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    let _ = tx.send(Ok("two\n".into())).await;
                });
                Body::from_stream(ReceiverStream::new(rx))
            }),
        )
        .route("/big", get(|| async { vec![7u8; BIG] }))
}

async fn serve(socket: &std::path::Path) {
    let _ = std::fs::remove_file(socket);
    let listener = tokio::net::UnixListener::bind(socket).unwrap();
    axum::serve(listener, app()).await.unwrap();
}

/// Not a test: the service, run by the node as its start command (`SERVICE_SOCKET` names
/// the socket), so a service started on demand is a real process.
#[tokio::test]
#[ignore]
async fn helper_service() {
    if let Some(socket) = std::env::var_os("SERVICE_SOCKET") {
        serve(std::path::Path::new(&socket)).await;
    }
}

fn req<T>(body: T) -> tonic::Request<T> {
    let mut r = tonic::Request::new(body);
    r.metadata_mut()
        .insert("authorization", "Bearer t-client".parse().unwrap());
    r
}

async fn node(c: &mut ControlServiceClient<Channel>) -> Node {
    for _ in 0..250 {
        let nodes = c.list_nodes(req(ListNodesRequest {})).await.unwrap().into_inner().nodes;
        if let Some(n) = nodes.into_iter().find(|n| n.connected) {
            return n;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("the node never connected");
}

struct Answer {
    status: u32,
    headers: Vec<HttpHeader>,
    /// Each body piece with when it arrived.
    pieces: Vec<(tokio::time::Instant, Vec<u8>)>,
}

impl Answer {
    fn body(&self) -> Vec<u8> {
        self.pieces.iter().flat_map(|(_, b)| b.clone()).collect()
    }
}

async fn call(
    c: &mut ControlServiceClient<Channel>,
    node_id: &str,
    service: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: Option<Vec<u8>>,
) -> Result<Answer, tonic::Status> {
    let head = ServiceRequestHead {
        service: service.into(),
        method: method.into(),
        path: path.into(),
        headers: headers
            .iter()
            .map(|(n, v)| HttpHeader {
                name: n.to_string(),
                value: v.as_bytes().to_vec(),
            })
            .collect(),
        has_body: body.is_some(),
    };
    let mut parts = vec![ServiceRequestPart {
        node_id: node_id.into(),
        head: Some(head),
        body: Vec::new(),
    }];
    for piece in body.unwrap_or_default().chunks(1 << 20) {
        parts.push(ServiceRequestPart {
            body: piece.to_vec(),
            ..Default::default()
        });
    }
    let mut stream = c.service_request(req(tokio_stream::iter(parts))).await?.into_inner();
    let first = stream.message().await?.expect("an answer");
    let head = first.head.expect("the answer's head first");
    let mut pieces = Vec::new();
    while let Some(m) = stream.message().await? {
        pieces.push((tokio::time::Instant::now(), m.body));
    }
    Ok(Answer {
        status: head.status,
        headers: head.headers,
        pieces,
    })
}

#[tokio::test(flavor = "multi_thread")]
async fn a_client_reaches_a_nodes_local_service_through_the_hub() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("svc.sock");
    tokio::spawn({
        let socket = socket.clone();
        async move { serve(&socket).await }
    });
    let services = Services::parse(
        &format!("[services.notes]\nsocket = \"{}\"\n", socket.display()),
        dir.path(),
    )
    .unwrap();
    let (hub, addr) = HubProc::start(&dir.path().join("hub.db"), "127.0.0.1:0");
    let _node = start_serving_node(&addr, "t-node", &dir.path().join(".claude"), Arc::new(services)).await;
    let mut c = ControlServiceClient::connect(format!("http://{addr}")).await.unwrap();
    let n = node(&mut c).await;
    assert_eq!(n.services, ["notes"]);

    // A plain request: the service sees a loopback Host and the client's own headers.
    let a = call(&mut c, &n.id, "notes", "GET", "/seen?a=1", &[("x-test", "yes"), ("host", "evil")], None)
        .await
        .unwrap();
    assert_eq!(a.status, 200);
    assert_eq!(String::from_utf8(a.body()).unwrap(), "host=localhost x=yes query=a=1");

    // A body, in two pieces.
    let sent: Vec<u8> = (0..(1 << 20) + 5).map(|i| (i % 251) as u8).collect();
    let a = call(&mut c, &n.id, "notes", "POST", "/echo", &[], Some(sent.clone())).await.unwrap();
    assert_eq!(a.body(), sent);

    // An answer larger than a piece arrives whole, its length passed on.
    let a = call(&mut c, &n.id, "notes", "GET", "/big", &[], None).await.unwrap();
    assert_eq!(a.body().len(), BIG);
    assert!(a.pieces.len() >= 4, "{} pieces", a.pieces.len());
    let length = a.headers.iter().find(|h| h.name == "content-length").expect("content-length");
    assert_eq!(length.value, BIG.to_string().as_bytes());

    // A streamed answer arrives as it is written, not at its end.
    let a = call(&mut c, &n.id, "notes", "GET", "/stream", &[], None).await.unwrap();
    assert_eq!(String::from_utf8(a.body()).unwrap(), "one\ntwo\n");
    let first = a.pieces.iter().find(|(_, b)| b.starts_with(b"one")).unwrap().0;
    let last = a.pieces.last().unwrap().0;
    assert!(last - first >= Duration::from_millis(800), "the stream was held back");

    // Missing things are refused before anything is sent.
    let e = call(&mut c, &n.id, "nope", "GET", "/", &[], None).await.err().unwrap();
    assert_eq!(e.code(), tonic::Code::NotFound);
    let e = call(&mut c, "no-such-node", "notes", "GET", "/", &[], None).await.err().unwrap();
    assert_eq!(e.code(), tonic::Code::Unavailable);
    let e = call(&mut c, &n.id, "notes", "GET", "no-slash", &[], None).await.err().unwrap();
    assert_eq!(e.code(), tonic::Code::InvalidArgument);
    hub.stop();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_service_is_started_on_demand_and_stopped_when_idle() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("lazy.sock");
    let down = dir.path().join("down.sock");
    let exe = std::env::current_exe().unwrap();
    let services = Arc::new(
        Services::parse(
            &format!(
                "[services.lazy]\nsocket = \"{s}\"\nidle = \"1s\"\nstart = [\"/bin/sh\", \"-c\", \"SERVICE_SOCKET={s} exec {exe} --exact helper_service --ignored\"]\n\n[services.down]\nsocket = \"{d}\"\n",
                s = socket.display(),
                d = down.display(),
                exe = exe.display(),
            ),
            dir.path(),
        )
        .unwrap(),
    );
    let (hub, addr) = HubProc::start(&dir.path().join("hub.db"), "127.0.0.1:0");
    let _node = start_serving_node(&addr, "t-node", &dir.path().join(".claude"), services.clone()).await;
    let mut c = ControlServiceClient::connect(format!("http://{addr}")).await.unwrap();
    let n = node(&mut c).await;

    assert!(!services.started("lazy").await);
    let a = call(&mut c, &n.id, "lazy", "GET", "/seen", &[], None).await.unwrap();
    assert_eq!(a.status, 200);
    assert!(services.started("lazy").await);

    // Busy within its idle time: kept; idle past it: stopped, and started again when asked.
    services.reap_once().await;
    assert!(services.started("lazy").await);
    tokio::time::sleep(Duration::from_millis(1300)).await;
    services.reap_once().await;
    assert!(!services.started("lazy").await);
    let a = call(&mut c, &n.id, "lazy", "GET", "/seen", &[], None).await.unwrap();
    assert_eq!(a.status, 200);

    // A service with no command and nothing on its socket: the node's reason, as an error.
    let e = call(&mut c, &n.id, "down", "GET", "/", &[], None).await.err().unwrap();
    assert_eq!(e.code(), tonic::Code::Unavailable);
    assert!(e.message().contains("nothing answers"), "{}", e.message());
    hub.stop();
}
