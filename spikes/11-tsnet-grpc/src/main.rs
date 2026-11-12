//! EXP-20261112-rq8kgc: tonic over libtailscale. Two tsnet identities in one process, a
//! "hub" serving a small gRPC service and a "node" calling it by MagicDNS name, then the
//! checks the experiment asks for, printed as one `===RESULT===` line.
//!
//! tsnet-grpc --state DIR [--authkey-file PATH] [--prefix NAME] [--stream N] [--idle-secs S]

mod ts;

use std::collections::BTreeSet;
use std::os::fd::{AsRawFd, IntoRawFd, OwnedFd};
use std::pin::Pin;
use std::sync::Mutex;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use hyper_util::rt::TokioIo;
use serde_json::{Value, json};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::server::Connected;
use tonic::transport::{Endpoint, Server};
use tonic::{Request, Response, Status};

pub mod pb {
    tonic::include_proto!("spike.v1");
}
use pb::echo_client::EchoClient;
use pb::echo_server::{Echo, EchoServer};
use pb::{CountRequest, Msg};

/// Every descriptor libtailscale handed out, to look for in a spawned child.
static HANDED: Mutex<Vec<(&'static str, i32, bool)>> = Mutex::new(Vec::new());

fn handed(kind: &'static str, fd: i32) {
    HANDED.lock().unwrap().push((kind, fd, ts::cloexec(fd)));
}

fn is_socket(fd: &OwnedFd) -> bool {
    let mut st: libc::stat = unsafe { std::mem::zeroed() };
    unsafe { libc::fstat(fd.as_raw_fd(), &mut st) == 0 && (st.st_mode & libc::S_IFMT) == libc::S_IFSOCK }
}

fn stream_of(fd: OwnedFd) -> Result<tokio::net::UnixStream> {
    let s = std::os::unix::net::UnixStream::from(fd);
    s.set_nonblocking(true)?;
    Ok(tokio::net::UnixStream::from_std(s)?)
}

/// An accepted tailnet connection, carrying the dialler's address into the request.
struct Conn {
    io: tokio::net::UnixStream,
    remote: String,
}

#[derive(Clone)]
struct Remote(String);

impl Connected for Conn {
    type ConnectInfo = Remote;
    fn connect_info(&self) -> Remote {
        Remote(self.remote.clone())
    }
}

impl AsyncRead for Conn {
    fn poll_read(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &mut ReadBuf<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.io).poll_read(cx, buf)
    }
}

impl AsyncWrite for Conn {
    fn poll_write(mut self: Pin<&mut Self>, cx: &mut Context<'_>, buf: &[u8]) -> Poll<std::io::Result<usize>> {
        Pin::new(&mut self.io).poll_write(cx, buf)
    }
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.io).poll_flush(cx)
    }
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.io).poll_shutdown(cx)
    }
}

/// Who is behind a tailnet address, from the serving identity's own status.
fn whois(status: &Value, remote: &str) -> Value {
    let ip = remote.rsplit_once(':').map_or(remote, |(ip, _)| ip).trim_matches(['[', ']']);
    let peers = status["Peer"].as_object().cloned().unwrap_or_default();
    for p in peers.values() {
        let ips: Vec<&str> = p["TailscaleIPs"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
        if ips.contains(&ip) {
            let user = &status["User"][p["UserID"].to_string()];
            return json!({
                "host": p["HostName"], "dns": p["DNSName"], "id": p["ID"],
                "login": user["LoginName"], "tags": p["Tags"],
            });
        }
    }
    json!({ "unknown": ip })
}

struct Svc {
    hub: ts::Server,
}

#[tonic::async_trait]
impl Echo for Svc {
    async fn say(&self, req: Request<Msg>) -> Result<Response<Msg>, Status> {
        let remote = req.extensions().get::<Remote>().map(|r| r.0.clone()).unwrap_or_default();
        let hub = self.hub;
        let status = tokio::task::spawn_blocking(move || hub.status())
            .await
            .map_err(|e| Status::internal(e.to_string()))?
            .map_err(|e| Status::internal(e.to_string()))?;
        let who = json!({ "remote": remote, "peer": whois(&status, &remote) });
        Ok(Response::new(Msg { text: who.to_string(), n: req.into_inner().n }))
    }

    type CountStream = ReceiverStream<Result<Msg, Status>>;

    async fn count(&self, req: Request<CountRequest>) -> Result<Response<Self::CountStream>, Status> {
        let CountRequest { n, idle_secs } = req.into_inner();
        let (tx, rx) = mpsc::channel(64);
        tokio::spawn(async move {
            for i in 0..n {
                if tx.send(Ok(Msg { text: String::new(), n: i })).await.is_err() {
                    return;
                }
            }
            if idle_secs > 0 {
                tokio::time::sleep(Duration::from_secs(idle_secs)).await;
                let _ = tx.send(Ok(Msg { text: "after idle".into(), n })).await;
            }
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }
}

fn arg(name: &str) -> Option<String> {
    let mut args = std::env::args();
    while let Some(a) = args.next() {
        if a == name {
            return args.next();
        }
    }
    None
}

#[tokio::main]
async fn main() -> Result<()> {
    let state = arg("--state").context("--state DIR")?;
    let prefix = arg("--prefix").unwrap_or_else(|| "saarthi-spike".into());
    let n: u64 = arg("--stream").map_or(Ok(1000), |s| s.parse())?;
    let idle: u64 = arg("--idle-secs").map_or(Ok(60), |s| s.parse())?;
    // The key is read and handed to libtailscale; it is never printed.
    let key = match arg("--authkey-file") {
        Some(p) => Some(std::fs::read_to_string(&p).with_context(|| format!("reading {p}"))?.trim().to_string()),
        None => None,
    };

    let mut ids = Vec::new();
    for role in ["hub", "node"] {
        let dir = format!("{state}/{role}");
        std::fs::create_dir_all(&dir)?;
        let log = std::fs::OpenOptions::new().create(true).append(true).open(format!("{dir}/tsnet.log"))?;
        let s = ts::Server::new(&dir, &format!("{prefix}-{role}"), key.as_deref(), log.into_raw_fd())?;
        ids.push(s);
    }
    let (hub, node) = (ids[0], ids[1]);

    // Both identities come up at once, each on a blocking thread.
    let up = |s: ts::Server| {
        tokio::task::spawn_blocking(move || {
            let t = Instant::now();
            s.up().map(|_| t.elapsed().as_millis() as u64)
        })
    };
    let (hub_up, node_up) = tokio::join!(up(hub), up(node));
    let (hub_up_ms, node_up_ms) = (hub_up??, node_up??);
    eprintln!("both up: hub {hub_up_ms} ms, node {node_up_ms} ms");

    let listener = hub.listen(":7700")?;
    handed("listener", listener);
    let (conn_tx, conn_rx) = mpsc::channel::<std::io::Result<Conn>>(16);
    let socket_kind = std::sync::Arc::new(Mutex::new(None::<bool>));
    let kind = socket_kind.clone();
    let rt = tokio::runtime::Handle::current();
    std::thread::spawn(move || {
        let _rt = rt.enter();
        loop {
            let (fd, remote) = match ts::accept(listener) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("accept: {e:#}");
                    return;
                }
            };
            handed("accepted", fd.as_raw_fd());
            kind.lock().unwrap().get_or_insert(is_socket(&fd));
            let conn = stream_of(fd).map(|io| Conn { io, remote }).map_err(std::io::Error::other);
            if conn_tx.blocking_send(conn).is_err() {
                return;
            }
        }
    });
    tokio::spawn(
        Server::builder()
            .http2_keepalive_interval(Some(Duration::from_secs(15)))
            .http2_keepalive_timeout(Some(Duration::from_secs(10)))
            .add_service(EchoServer::new(Svc { hub }))
            .serve_with_incoming(ReceiverStream::new(conn_rx)),
    );

    let target = format!("{prefix}-hub:7700");
    let dials = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let counted = dials.clone();
    let connector = tower::service_fn(move |_uri: tonic::transport::Uri| {
        let target = target.clone();
        let counted = counted.clone();
        async move {
            // After a start from saved state, `up` returns before the peers are known, so the
            // name may not resolve for a moment: retry, as the node's link does.
            let mut tries = 0;
            let fd = loop {
                tries += 1;
                let t = target.clone();
                match tokio::task::spawn_blocking(move || node.dial(&t)).await? {
                    Ok(fd) => break fd,
                    Err(e) if tries < 60 => {
                        eprintln!("dial try {tries}: {e:#}");
                        tokio::time::sleep(Duration::from_millis(500)).await;
                    }
                    Err(e) => return Err(e),
                }
            };
            counted.fetch_add(tries, std::sync::atomic::Ordering::Relaxed);
            handed("dialled", fd.as_raw_fd());
            anyhow::Ok(TokioIo::new(stream_of(fd)?))
        }
    });
    let t = Instant::now();
    let channel = Endpoint::from_shared(format!("http://{prefix}-hub:7700"))?
        .connect_timeout(Duration::from_secs(30))
        .http2_keep_alive_interval(Duration::from_secs(15))
        .keep_alive_timeout(Duration::from_secs(10))
        .keep_alive_while_idle(true)
        .connect_with_connector(connector)
        .await?;
    let connect_ms = t.elapsed().as_millis() as u64;
    let mut client = EchoClient::new(channel);

    let t = Instant::now();
    let said = client.say(Msg { text: "hello".into(), n: 7 }).await?.into_inner();
    let say_ms = t.elapsed().as_millis() as u64;
    let seen: Value = serde_json::from_str(&said.text)?;

    let t = Instant::now();
    let mut s = client.count(CountRequest { n, idle_secs: 0 }).await?.into_inner();
    let (mut got, mut in_order) = (0u64, true);
    while let Some(m) = s.message().await? {
        in_order &= m.n == got;
        got += 1;
    }
    let stream_ms = t.elapsed().as_millis() as u64;

    // A child spawned while connections are open: which descriptors does it hold?
    let out = tokio::process::Command::new("/bin/sh").args(["-c", "ls /dev/fd"]).output().await?;
    let child: BTreeSet<i32> = String::from_utf8_lossy(&out.stdout).split_whitespace().filter_map(|s| s.parse().ok()).collect();
    let handed_out = HANDED.lock().unwrap().clone();
    let inherited: Vec<Value> = handed_out
        .iter()
        .filter(|(_, fd, _)| child.contains(fd))
        .map(|(k, fd, _)| json!({ "kind": k, "fd": fd }))
        .collect();

    let (mut idle_ok, mut idle_ms) = (None, 0);
    if idle > 0 {
        let t = Instant::now();
        let mut s = client.count(CountRequest { n: 1, idle_secs: idle }).await?.into_inner();
        let first = s.message().await?;
        let after = s.message().await?;
        idle_ms = t.elapsed().as_millis() as u64;
        idle_ok = Some(first.is_some() && after.is_some_and(|m| m.text == "after idle"));
    }

    let self_of = |s: ts::Server| -> Result<Value> {
        let st = s.status()?;
        Ok(json!({ "id": st["Self"]["ID"], "host": st["Self"]["HostName"], "ips": s.ips()? }))
    };
    let result = json!({
        "target": format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
        "with_key": key.is_some(),
        "hub": self_of(hub)?, "node": self_of(node)?,
        "up_ms": { "hub": hub_up_ms, "node": node_up_ms },
        "connect_ms": connect_ms, "dials": dials.load(std::sync::atomic::Ordering::Relaxed),
        "say": { "ok": said.n == 7, "ms": say_ms, "seen": seen },
        "stream": { "n": got, "want": n, "in_order": in_order, "ms": stream_ms },
        "idle": { "ok": idle_ok, "secs": idle, "ms": idle_ms },
        "conn_is_socket": *socket_kind.lock().unwrap(),
        "cloexec": handed_out.iter().map(|(k, fd, c)| json!({ "kind": k, "fd": fd, "cloexec": c })).collect::<Vec<_>>(),
        "child_inherited": inherited,
    });
    println!("===RESULT=== {result}");
    hub.close();
    node.close();
    Ok(())
}
