//! The node's local services (ADR-0052, ADR-0057): HTTP servers on unix sockets that the
//! owner's clients reach through the hub, yajna's notebook server the first. A services file
//! names each one; the node registers the names, and for each `OpenService` from the hub
//! sends the request to the socket and streams the answer back over a `ServiceStream` it
//! dials, starting the service first if it has a command and nothing answers.
//!
//! ```toml
//! [services.yajna]
//! socket = "~/.local/state/yajna/serve.sock"
//! start = ["yajna", "serve", "--socket", "~/.local/state/yajna/serve.sock"]
//! idle = "10m"
//! ```

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper_util::rt::TokioIo;
use saarthi_proto::node::v1::{HttpHeader, HttpStatus, OpenService, ServiceUp};
use serde::Deserialize;
use tokio::process::{Child, Command};
use tokio::sync::{Mutex, mpsc};
use tokio_stream::wrappers::ReceiverStream;

use crate::claude_api::HubApi;

/// A body goes up in pieces of at most this.
const PIECE: usize = 1 << 20;
/// A request body larger than this is refused rather than held in memory.
const MAX_BODY: usize = 64 << 20;
/// How long a started service has to answer on its socket.
const START_TIMEOUT: Duration = Duration::from_secs(15);
/// How often a started service is checked for idleness.
const IDLE_CHECK: Duration = Duration::from_secs(30);

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    services: BTreeMap<String, Entry>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Entry {
    socket: PathBuf,
    #[serde(default)]
    start: Vec<String>,
    idle: Option<String>,
}

/// One service.
pub struct Service {
    pub name: String,
    pub socket: PathBuf,
    /// The command that starts it when nothing answers on the socket; empty: never started.
    pub start: Vec<String>,
    /// How long it may go without requests before a process the node started is stopped;
    /// None: never stopped.
    pub idle: Option<Duration>,
    /// The process the node started, while it runs.
    child: Mutex<Option<Child>>,
    /// Requests in flight, and when the last one ended.
    active: AtomicUsize,
    last: std::sync::Mutex<tokio::time::Instant>,
}

#[derive(Default)]
pub struct Services {
    by_name: BTreeMap<String, Arc<Service>>,
}

/// `90s`, `10m`, `1h`, or plain seconds.
fn duration(s: &str) -> Result<Duration> {
    let s = s.trim();
    let (n, unit) = match s.find(|c: char| !c.is_ascii_digit()) {
        Some(i) => s.split_at(i),
        None => (s, "s"),
    };
    let n: u64 = n.parse().with_context(|| format!("not a duration: {s}"))?;
    let secs = match unit {
        "s" => n,
        "m" => n * 60,
        "h" => n * 3600,
        _ => bail!("not a duration: {s} (use s, m or h)"),
    };
    Ok(Duration::from_secs(secs))
}

fn expand(p: &Path, home: &Path) -> PathBuf {
    match p.strip_prefix("~") {
        Ok(rest) => home.join(rest),
        Err(_) => p.to_path_buf(),
    }
}

impl Services {
    /// Reads a services file; `~` in a socket path or a start command's arguments is the
    /// home directory.
    pub fn load(path: &Path, home: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        Self::parse(&text, home)
    }

    pub fn parse(text: &str, home: &Path) -> Result<Self> {
        let file: File = toml::from_str(text)?;
        let mut by_name = BTreeMap::new();
        for (name, e) in file.services {
            if !e.socket.is_absolute() && !e.socket.starts_with("~") {
                bail!("service {name}: the socket must be an absolute path or start with ~");
            }
            let idle = e.idle.as_deref().map(duration).transpose().with_context(|| format!("service {name}"))?;
            let start = e
                .start
                .iter()
                .map(|a| match a.strip_prefix("~/") {
                    Some(rest) => home.join(rest).to_string_lossy().into_owned(),
                    None => a.clone(),
                })
                .collect();
            by_name.insert(
                name.clone(),
                Arc::new(Service {
                    name,
                    socket: expand(&e.socket, home),
                    start,
                    idle,
                    child: Mutex::new(None),
                    active: AtomicUsize::new(0),
                    last: std::sync::Mutex::new(tokio::time::Instant::now()),
                }),
            );
        }
        Ok(Self { by_name })
    }

    /// Adds a service the node serves itself on `socket`, such as `changes` (ADR-0067),
    /// unless the services file already names one so; false then.
    pub fn add_builtin(&mut self, name: &str, socket: PathBuf) -> bool {
        if self.by_name.contains_key(name) {
            return false;
        }
        self.by_name.insert(
            name.to_string(),
            Arc::new(Service {
                name: name.to_string(),
                socket,
                start: Vec::new(),
                idle: None,
                child: Mutex::new(None),
                active: AtomicUsize::new(0),
                last: std::sync::Mutex::new(tokio::time::Instant::now()),
            }),
        );
        true
    }

    pub fn names(&self) -> Vec<String> {
        self.by_name.keys().cloned().collect()
    }

    pub fn sockets(&self) -> Vec<PathBuf> {
        self.by_name.values().map(|s| s.socket.clone()).collect()
    }

    pub fn get(&self, name: &str) -> Option<Arc<Service>> {
        self.by_name.get(name).cloned()
    }

    /// Stops the services the node started once they have been idle long enough; runs for
    /// the node's life.
    pub async fn reap_idle(self: Arc<Self>) {
        loop {
            tokio::time::sleep(IDLE_CHECK).await;
            self.reap_once().await;
        }
    }

    /// One look at every service: those the node started and that have been idle long
    /// enough are stopped.
    pub async fn reap_once(&self) {
        for s in self.by_name.values() {
            s.stop_if_idle().await;
        }
    }

    /// Whether the node has a process of `name` running that it started.
    pub async fn started(&self, name: &str) -> bool {
        match self.by_name.get(name) {
            Some(s) => s.child.lock().await.is_some(),
            None => false,
        }
    }
}

impl Service {
    async fn stop_if_idle(&self) {
        let Some(idle) = self.idle else { return };
        let mut child = self.child.lock().await;
        if child.is_none() || self.active.load(Ordering::SeqCst) > 0 {
            return;
        }
        if self.last.lock().expect("not poisoned").elapsed() < idle {
            return;
        }
        if let Some(mut c) = child.take() {
            tracing::info!(service = %self.name, "stopping an idle service");
            let _ = c.kill().await;
        }
    }

    /// A connection to the service, starting it first if nothing answers and it has a
    /// command.
    async fn connect(&self) -> Result<tokio::net::UnixStream> {
        if let Ok(s) = tokio::net::UnixStream::connect(&self.socket).await {
            return Ok(s);
        }
        if self.start.is_empty() {
            bail!("nothing answers on {}", self.socket.display());
        }
        let mut child = self.child.lock().await;
        // Another request may have started it meanwhile.
        if let Ok(s) = tokio::net::UnixStream::connect(&self.socket).await {
            return Ok(s);
        }
        let running = match child.as_mut() {
            Some(c) => c.try_wait().map(|s| s.is_none()).unwrap_or(false),
            None => false,
        };
        if !running {
            tracing::info!(service = %self.name, command = ?self.start, "starting a service");
            let c = Command::new(&self.start[0])
                .args(&self.start[1..])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .kill_on_drop(true)
                .spawn()
                .with_context(|| format!("starting {}", self.start[0]))?;
            *child = Some(c);
        }
        let deadline = tokio::time::Instant::now() + START_TIMEOUT;
        loop {
            if let Ok(s) = tokio::net::UnixStream::connect(&self.socket).await {
                return Ok(s);
            }
            if let Some(c) = child.as_mut()
                && let Ok(Some(status)) = c.try_wait()
            {
                *child = None;
                bail!("the service exited before it answered ({status})");
            }
            if tokio::time::Instant::now() > deadline {
                bail!("the service did not answer on {} in time", self.socket.display());
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    fn begin(&self) {
        self.active.fetch_add(1, Ordering::SeqCst);
    }

    fn end(&self) {
        *self.last.lock().expect("not poisoned") = tokio::time::Instant::now();
        self.active.fetch_sub(1, Ordering::SeqCst);
    }
}

/// Headers that belong to one hop, or that the node sets itself.
fn passes(name: &str) -> bool {
    !matches!(
        name.to_ascii_lowercase().as_str(),
        "host"
            | "connection"
            | "keep-alive"
            | "proxy-connection"
            | "proxy-authorization"
            | "te"
            | "trailer"
            | "transfer-encoding"
            | "upgrade"
            | "content-length"
    )
}

/// Serves one `OpenService` from the hub: dials `ServiceStream` back naming it and, at the
/// same time, sends the request to the service; the answer goes up the stream.
pub async fn open(services: Arc<Services>, api: Arc<HubApi>, open: OpenService) {
    let (up, up_rx) = mpsc::channel::<ServiceUp>(8);
    let _ = up
        .send(ServiceUp {
            id: open.id.clone(),
            ..Default::default()
        })
        .await;
    let head = open.head.unwrap_or_default();
    // The stream back to the hub; its downward half carries the request's body, if any.
    let (body_tx, body_rx) = mpsc::channel::<Vec<u8>>(8);
    let call = {
        let api = api.clone();
        let id = open.id.clone();
        tokio::spawn(async move {
            let mut down = match api.service_stream(ReceiverStream::new(up_rx)).await {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!(%id, error = %e, "the hub did not take a service stream");
                    return;
                }
            };
            loop {
                match down.message().await {
                    Ok(Some(m)) => {
                        if body_tx.send(m.body).await.is_err() {
                            return;
                        }
                    }
                    _ => return,
                }
            }
        })
    };
    let error = |why: String| ServiceUp {
        error: why,
        ..Default::default()
    };
    let Some(service) = services.get(&head.service) else {
        let _ = up.send(error(format!("the node has no service named {}", head.service))).await;
        return;
    };
    service.begin();
    let outcome = forward(&service, head, body_rx, &up).await;
    service.end();
    if let Err(e) = outcome {
        tracing::info!(service = %service.name, error = %format!("{e:#}"), "a service request failed");
        let _ = up.send(error(format!("{e:#}"))).await;
    }
    drop(up);
    let _ = call.await;
}

async fn forward(
    service: &Service,
    head: saarthi_proto::node::v1::ServiceRequestHead,
    mut body_rx: mpsc::Receiver<Vec<u8>>,
    up: &mpsc::Sender<ServiceUp>,
) -> Result<()> {
    let mut body = Vec::new();
    if head.has_body {
        while let Some(piece) = body_rx.recv().await {
            body.extend_from_slice(&piece);
            if body.len() > MAX_BODY {
                bail!("the request body is too large");
            }
        }
    }
    let io = service.connect().await?;
    let (mut sender, conn) = hyper::client::conn::http1::handshake(TokioIo::new(io)).await?;
    tokio::spawn(async move {
        let _ = conn.await;
    });
    let mut req = hyper::Request::builder()
        .method(head.method.as_str())
        .uri(&head.path);
    for h in &head.headers {
        if passes(&h.name) {
            req = req.header(h.name.as_str(), h.value.as_slice());
        }
    }
    // A loopback name, which a server guarding against DNS rebinding accepts.
    let req = req
        .header(hyper::header::HOST, "localhost")
        .body(Full::new(Bytes::from(body)))?;
    let resp = sender.send_request(req).await?;
    let status = HttpStatus {
        status: resp.status().as_u16() as u32,
        headers: resp
            .headers()
            .iter()
            .filter(|(n, _)| passes(n.as_str()) || n.as_str() == "content-length")
            .map(|(n, v)| HttpHeader {
                name: n.as_str().to_string(),
                value: v.as_bytes().to_vec(),
            })
            .collect(),
    };
    let sent = |r: Result<(), _>| r.map_err(|_| anyhow::anyhow!("the hub closed the stream"));
    sent(
        up.send(ServiceUp {
            head: Some(status),
            ..Default::default()
        })
        .await,
    )?;
    let mut b = resp.into_body();
    while let Some(frame) = b.frame().await {
        if let Ok(data) = frame?.into_data() {
            for piece in data.chunks(PIECE) {
                sent(
                    up.send(ServiceUp {
                        body: piece.to_vec(),
                        ..Default::default()
                    })
                    .await,
                )?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_a_services_file() {
        let home = Path::new("/home/me");
        let s = Services::parse(
            r#"
            [services.yajna]
            socket = "~/.local/state/yajna/serve.sock"
            start = ["yajna", "serve", "--socket", "~/.local/state/yajna/serve.sock"]
            idle = "10m"

            [services.other]
            socket = "/run/other.sock"
            "#,
            home,
        )
        .unwrap();
        assert_eq!(s.names(), ["other", "yajna"]);
        let y = s.get("yajna").unwrap();
        assert_eq!(y.socket, Path::new("/home/me/.local/state/yajna/serve.sock"));
        assert_eq!(y.start[3], "/home/me/.local/state/yajna/serve.sock");
        assert_eq!(y.idle, Some(Duration::from_secs(600)));
        let o = s.get("other").unwrap();
        assert!(o.start.is_empty() && o.idle.is_none());
    }

    #[test]
    fn refuses_a_relative_socket_and_a_bad_duration() {
        let home = Path::new("/h");
        assert!(Services::parse("[services.a]\nsocket = \"a.sock\"\n", home).is_err());
        assert!(Services::parse("[services.a]\nsocket = \"/a.sock\"\nidle = \"5d\"\n", home).is_err());
        assert!(Services::parse("[services.a]\nsocket = \"/a.sock\"\nport = 1\n", home).is_err());
    }

    #[test]
    fn durations() {
        assert_eq!(duration("90").unwrap(), Duration::from_secs(90));
        assert_eq!(duration("90s").unwrap(), Duration::from_secs(90));
        assert_eq!(duration("2h").unwrap(), Duration::from_secs(7200));
    }
}
