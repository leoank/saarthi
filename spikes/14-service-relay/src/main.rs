//! EXP-20261114-raqjjy: a throwaway relay in the shape ADR-0052 proposes. A client's HTTP
//! request goes to the hub as `ServiceRequest` pieces; the hub asks the node on its `Link`
//! to open a service; the node dials back a `ServiceStream` naming the request, sends it to
//! a local HTTP service (a unix socket or a loopback port), and streams the answer back.
//!
//!     service-relay hub <listen addr>
//!     service-relay node <hub url> <node name> <service>=<unix:/path | tcp:host:port>...
//!     service-relay proxy <hub url> <port> <node> <service>
//!
//! `proxy` is the client: a loopback HTTP server whose every request goes through the hub,
//! so curl and a browser (or a WebView) can be pointed at it.

use std::collections::HashMap;
use std::convert::Infallible;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use anyhow::Context;
use bytes::Bytes;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::{BodyExt, Full, StreamBody};
use hyper::body::{Frame, Incoming};
use hyper::{Request, Response};
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::{Channel, Server};
use tonic::{Status, Streaming};

pub mod pb {
    tonic::include_proto!("relay");
}
use pb::*;

/// A body piece, as ADR-0051's and ADR-0052's.
const PIECE: usize = 1 << 20;

type Out = ReceiverStream<Result<Piece, Status>>;
type Pending = (mpsc::Receiver<Result<Piece, Status>>, mpsc::Sender<Result<Piece, Status>>);
type Body = UnsyncBoxBody<Bytes, std::io::Error>;

fn passes(name: &str) -> bool {
    !matches!(
        name.to_ascii_lowercase().as_str(),
        "host" | "connection" | "keep-alive" | "proxy-connection" | "te" | "trailer"
            | "transfer-encoding" | "upgrade"
    )
}

fn headers(h: &hyper::HeaderMap) -> Vec<Header> {
    h.iter()
        .filter(|(n, _)| passes(n.as_str()))
        .map(|(n, v)| Header { name: n.as_str().to_string(), value: v.as_bytes().to_vec() })
        .collect()
}

// ---- hub

#[derive(Default, Clone)]
struct Hub {
    nodes: Arc<Mutex<HashMap<String, mpsc::Sender<Result<HubMsg, Status>>>>>,
    pending: Arc<Mutex<HashMap<String, Pending>>>,
    next: Arc<AtomicU64>,
}

#[tonic::async_trait]
impl node_server::Node for Hub {
    type LinkStream = ReceiverStream<Result<HubMsg, Status>>;

    async fn link(
        &self,
        req: tonic::Request<Streaming<NodeMsg>>,
    ) -> Result<tonic::Response<Self::LinkStream>, Status> {
        let mut inbound = req.into_inner();
        let hello = inbound.message().await?.ok_or_else(|| Status::invalid_argument("no hello"))?;
        let (tx, rx) = mpsc::channel(16);
        self.nodes.lock().unwrap().insert(hello.name.clone(), tx);
        eprintln!("hub: node {} linked", hello.name);
        let (nodes, name) = (self.nodes.clone(), hello.name);
        tokio::spawn(async move {
            while let Ok(Some(_)) = inbound.message().await {}
            nodes.lock().unwrap().remove(&name);
            eprintln!("hub: node {name} gone");
        });
        Ok(tonic::Response::new(ReceiverStream::new(rx)))
    }

    type ServiceStreamStream = Out;

    async fn service_stream(
        &self,
        req: tonic::Request<Streaming<Piece>>,
    ) -> Result<tonic::Response<Out>, Status> {
        let mut inbound = req.into_inner();
        let first = inbound.message().await?.ok_or_else(|| Status::invalid_argument("no id"))?;
        let (to_node, to_client) = self
            .pending
            .lock()
            .unwrap()
            .remove(&first.id)
            .ok_or_else(|| Status::not_found("no such request"))?;
        tokio::spawn(async move {
            loop {
                match inbound.message().await {
                    Ok(Some(p)) => {
                        if to_client.send(Ok(p)).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        let _ = to_client.send(Err(e)).await;
                        break;
                    }
                }
            }
        });
        Ok(tonic::Response::new(ReceiverStream::new(to_node)))
    }
}

#[tonic::async_trait]
impl control_server::Control for Hub {
    type ServiceRequestStream = Out;

    async fn service_request(
        &self,
        req: tonic::Request<Streaming<Piece>>,
    ) -> Result<tonic::Response<Out>, Status> {
        let mut inbound = req.into_inner();
        let first = inbound.message().await?.ok_or_else(|| Status::invalid_argument("empty"))?;
        let head = first.head.clone().ok_or_else(|| Status::invalid_argument("no head"))?;
        let node = self
            .nodes
            .lock()
            .unwrap()
            .get(&head.node)
            .cloned()
            .ok_or_else(|| Status::not_found("no such node"))?;
        let id = self.next.fetch_add(1, Ordering::Relaxed).to_string();
        let (to_node, to_node_rx) = mpsc::channel(8);
        let (to_client, to_client_rx) = mpsc::channel(8);
        self.pending.lock().unwrap().insert(id.clone(), (to_node_rx, to_client));
        node.send(Ok(HubMsg { id, service: head.service }))
            .await
            .map_err(|_| Status::unavailable("the node is gone"))?;
        tokio::spawn(async move {
            if to_node.send(Ok(first)).await.is_err() {
                return;
            }
            while let Ok(Some(p)) = inbound.message().await {
                if to_node.send(Ok(p)).await.is_err() {
                    return;
                }
            }
        });
        Ok(tonic::Response::new(ReceiverStream::new(to_client_rx)))
    }
}

async fn hub(addr: &str) -> anyhow::Result<()> {
    let hub = Hub::default();
    Server::builder()
        .add_service(node_server::NodeServer::new(hub.clone()))
        .add_service(control_server::ControlServer::new(hub))
        .serve(addr.parse()?)
        .await?;
    Ok(())
}

// ---- node

async fn node(url: String, name: String, services: HashMap<String, String>) -> anyhow::Result<()> {
    let ch = Channel::from_shared(url)?.connect().await?;
    let (tx, rx) = mpsc::channel(1);
    tx.send(NodeMsg { name }).await?;
    let mut link = node_client::NodeClient::new(ch.clone())
        .link(ReceiverStream::new(rx))
        .await?
        .into_inner();
    eprintln!("node: linked");
    while let Some(m) = link.message().await? {
        let (ch, services) = (ch.clone(), services.clone());
        tokio::spawn(async move {
            if let Err(e) = serve_one(ch, &services, m).await {
                eprintln!("node: {e:#}");
            }
        });
    }
    drop(tx);
    Ok(())
}

async fn serve_one(ch: Channel, services: &HashMap<String, String>, m: HubMsg) -> anyhow::Result<()> {
    let (out, out_rx) = mpsc::channel::<Piece>(8);
    out.send(Piece { id: m.id.clone(), ..Default::default() }).await?;
    let mut inbound = node_client::NodeClient::new(ch)
        .service_stream(ReceiverStream::new(out_rx))
        .await?
        .into_inner();
    let first = inbound.message().await?.context("no request")?;
    let head = first.head.context("no head")?;
    let mut body = first.body;
    while let Some(p) = inbound.message().await? {
        body.extend_from_slice(&p.body);
    }
    let Some(addr) = services.get(&m.service) else {
        let head = Head { status: 404, ..Default::default() };
        out.send(Piece { head: Some(head), ..Default::default() }).await?;
        return Ok(());
    };
    let mut sender = connect(addr).await?;
    let mut rb = Request::builder().method(head.method.as_str()).uri(&head.path);
    for h in &head.headers {
        if passes(&h.name) && !h.name.eq_ignore_ascii_case("content-length") {
            rb = rb.header(&h.name, &h.value[..]);
        }
    }
    let req = rb.header("host", "localhost").body(Full::new(Bytes::from(body)))?;
    let resp = sender.send_request(req).await?;
    let head = Head {
        status: resp.status().as_u16() as u32,
        headers: headers(resp.headers()),
        ..Default::default()
    };
    out.send(Piece { head: Some(head), ..Default::default() }).await?;
    let mut b = resp.into_body();
    while let Some(f) = b.frame().await {
        if let Ok(d) = f?.into_data() {
            for c in d.chunks(PIECE) {
                if out.send(Piece { body: c.to_vec(), ..Default::default() }).await.is_err() {
                    return Ok(());
                }
            }
        }
    }
    Ok(())
}

async fn connect(addr: &str) -> anyhow::Result<hyper::client::conn::http1::SendRequest<Full<Bytes>>> {
    async fn hs<S: AsyncRead + AsyncWrite + Unpin + Send + 'static>(
        s: S,
    ) -> anyhow::Result<hyper::client::conn::http1::SendRequest<Full<Bytes>>> {
        let (sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(s)).await?;
        tokio::spawn(async move {
            let _ = conn.await;
        });
        Ok(sender)
    }
    match addr.strip_prefix("unix:") {
        Some(p) => hs(tokio::net::UnixStream::connect(p).await?).await,
        None => hs(tokio::net::TcpStream::connect(addr.trim_start_matches("tcp:")).await?).await,
    }
}

// ---- client

async fn proxy(url: String, port: u16, node: String, service: String) -> anyhow::Result<()> {
    let ch = Channel::from_shared(url)?.connect().await?;
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    eprintln!("proxy: listening on 127.0.0.1:{}", listener.local_addr()?.port());
    loop {
        let (s, _) = listener.accept().await?;
        let (ch, node, service) = (ch.clone(), node.clone(), service.clone());
        tokio::spawn(async move {
            let svc = hyper::service::service_fn(move |req| {
                let (ch, node, service) = (ch.clone(), node.clone(), service.clone());
                async move {
                    if req.uri().path() == "/__sse.html" {
                        return Ok(Response::builder()
                            .header("content-type", "text/html")
                            .body(full(SSE_PAGE.to_string()))
                            .unwrap());
                    }
                    Ok::<_, Infallible>(match relay(ch, node, service, req).await {
                        Ok(r) => r,
                        Err(e) => Response::builder()
                            .status(502)
                            .body(full(format!("relay: {e:#}\n")))
                            .unwrap(),
                    })
                }
            });
            let _ = hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(s), svc)
                .await;
        });
    }
}

/// The WebView test page: logs when each change event of the log file arrives, by the
/// device's clock, to the console (which the spike app copies to logcat).
const SSE_PAGE: &str = r#"<!doctype html><meta charset=utf-8><pre id=o>waiting</pre><script>
const o = document.getElementById('o');
const es = new EventSource('/api/events?watch=data/log.txt');
es.addEventListener('change', e => {
  const d = JSON.parse(e.data);
  const line = `SSE ${Date.now()} size=${d.size}`;
  console.log(line); o.textContent = line + '\n' + o.textContent;
});
es.onerror = () => console.log('SSE error ' + Date.now());
</script>"#;

fn full(s: String) -> Body {
    Full::new(Bytes::from(s)).map_err(|n: Infallible| match n {}).boxed_unsync()
}

async fn relay(
    ch: Channel,
    node: String,
    service: String,
    req: Request<Incoming>,
) -> anyhow::Result<Response<Body>> {
    let (parts, body) = req.into_parts();
    let body = body.collect().await?.to_bytes();
    let head = Head {
        node,
        service,
        method: parts.method.to_string(),
        path: parts.uri.path_and_query().map(|p| p.to_string()).unwrap_or_else(|| "/".into()),
        headers: headers(&parts.headers),
        status: 0,
    };
    let mut pieces = vec![Piece { head: Some(head), ..Default::default() }];
    pieces.extend(body.chunks(PIECE).map(|c| Piece { body: c.to_vec(), ..Default::default() }));
    let mut inbound = control_client::ControlClient::new(ch)
        .service_request(tokio_stream::iter(pieces))
        .await?
        .into_inner();
    let head = inbound.message().await?.context("no answer")?.head.context("no head")?;
    let mut rb = Response::builder().status(head.status as u16);
    for h in head.headers {
        rb = rb.header(h.name, h.value);
    }
    let frames = inbound.map(|r| match r {
        Ok(p) => Ok(Frame::data(Bytes::from(p.body))),
        Err(e) => Err(std::io::Error::other(e)),
    });
    Ok(rb.body(StreamBody::new(frames).boxed_unsync())?)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let a: Vec<String> = std::env::args().skip(1).collect();
    match a.first().map(String::as_str) {
        Some("hub") => hub(&a[1]).await,
        Some("node") => {
            let services = a[3..]
                .iter()
                .filter_map(|s| s.split_once('=').map(|(k, v)| (k.to_string(), v.to_string())))
                .collect();
            node(a[1].clone(), a[2].clone(), services).await
        }
        Some("proxy") => proxy(a[1].clone(), a[2].parse()?, a[3].clone(), a[4].clone()).await,
        _ => anyhow::bail!("usage: hub <addr> | node <url> <name> <svc>=<addr>... | proxy <url> <port> <node> <svc>"),
    }
}
