//! A real hub and a real node on loopback, shared by the hub's end-to-end tests.
#![allow(dead_code)]

use std::sync::Arc;
use std::time::Duration;

use saarthi_hub::claude_tokens::ClaudeTokens;
use saarthi_hub::db::Db;
use saarthi_hub::live::Live;
use saarthi_hub::node_service::Hub;
use saarthi_hub::tokens::Tokens;
use saarthi_node::link;
use saarthi_node::outbox::Outbox;
use saarthi_node::pending::Pending;
use saarthi_node::sessions::Sessions;
use tokio::net::TcpListener;
use tokio::task::JoinHandle;

pub const TOKENS: &str = "test-node t-node\n";
pub const CLIENT_TOKENS: &str = "test-client t-client\n";

/// A hub on its own runtime in its own thread, so stopping it drops every connection and
/// task it had, as a process exit would.
pub struct HubProc {
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl HubProc {
    pub fn start(db_path: &std::path::Path, addr: &str) -> (Self, String) {
        Self::start_with(db_path, addr, None)
    }

    /// The same, publishing notifications to an ntfy topic URL.
    pub fn start_with(
        db_path: &std::path::Path,
        addr: &str,
        ntfy: Option<String>,
    ) -> (Self, String) {
        Self::start_full(db_path, addr, ntfy, "")
    }

    /// The same, holding Claude tokens, with usage probes run on `node` about every `every`,
    /// the first at once (ADR-0043).
    pub fn start_probing(
        db_path: &std::path::Path,
        addr: &str,
        claude_tokens: &str,
        node: &str,
        every: Duration,
    ) -> (Self, String) {
        Self::start_inner(
            db_path,
            addr,
            None,
            claude_tokens,
            Some((node.to_string(), every)),
        )
    }

    /// The same, holding the named Claude tokens of a `--claude-tokens` file's text.
    pub fn start_full(
        db_path: &std::path::Path,
        addr: &str,
        ntfy: Option<String>,
        claude_tokens: &str,
    ) -> (Self, String) {
        Self::start_inner(db_path, addr, ntfy, claude_tokens, None)
    }

    fn start_inner(
        db_path: &std::path::Path,
        addr: &str,
        ntfy: Option<String>,
        claude_tokens: &str,
        probes: Option<(String, Duration)>,
    ) -> (Self, String) {
        let claude_tokens = claude_tokens.to_string();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let (bound_tx, bound_rx) = std::sync::mpsc::channel();
        let (db_path, addr) = (db_path.to_path_buf(), addr.to_string());
        let thread = std::thread::spawn(move || {
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async move {
                let db = Db::open(&db_path).await.unwrap();
                let claude_tokens = ClaudeTokens::parse(&claude_tokens).unwrap();
                let fps: Vec<(String, String)> = claude_tokens
                    .fingerprints()
                    .map(|(n, f)| (n.to_string(), f))
                    .collect();
                db.sync_claude_tokens(&fps, saarthi_hub::node_service::now_ms())
                    .await
                    .unwrap();
                let listener = TcpListener::bind(&addr).await.unwrap();
                bound_tx
                    .send(listener.local_addr().unwrap().to_string())
                    .unwrap();
                let hub = Hub {
                    notifier: Arc::new(saarthi_hub::notify::Notifier::new(ntfy, db.clone(), Vec::new())),
                    db,
                    node_tokens: Arc::new(Tokens::parse(TOKENS).unwrap()),
                    client_tokens: Arc::new(Tokens::parse(CLIENT_TOKENS).unwrap()),
                    claude_tokens: Arc::new(claude_tokens),
                    live: Live::new(),
                };
                if let Some((node, every)) = probes {
                    tokio::spawn(saarthi_hub::run_usage_probes(
                        hub.clone(),
                        node,
                        every,
                        Duration::from_millis(1),
                    ));
                }
                tokio::select! {
                    r = saarthi_hub::serve(listener, hub) => r.unwrap(),
                    _ = stopped => {}
                }
            });
            rt.shutdown_timeout(Duration::ZERO);
        });
        let addr = bound_rx.recv().unwrap();
        (
            Self {
                stop: Some(stop),
                thread: Some(thread),
            },
            addr,
        )
    }

    pub fn stop(mut self) {
        let _ = self.stop.take().unwrap().send(());
        self.thread.take().unwrap().join().unwrap();
    }
}

/// A node as `saarthi node serve --hub` runs it: its outbox draining into a link to the hub,
/// and its hook endpoint on a free port holding PermissionRequest for `hold`.
pub struct NodeProc {
    pub outbox: Outbox,
    pub sessions: Arc<Sessions>,
    pub pending: Arc<Pending>,
    pub hooks: std::net::SocketAddr,
    pub task: JoinHandle<()>,
}

pub async fn start_node(
    hub_addr: &str,
    token: &str,
    claude_dir: &std::path::Path,
    hold: Duration,
) -> NodeProc {
    start_spawning_node(hub_addr, token, claude_dir, hold, "claude", &[], Default::default()).await
}

/// The same, allowed to spawn sessions under `roots` with `claude` as the program.
pub async fn start_spawning_node(
    hub_addr: &str,
    token: &str,
    claude_dir: &std::path::Path,
    hold: Duration,
    claude: &str,
    roots: &[std::path::PathBuf],
    contained: saarthi_node::spawner::Contained,
) -> NodeProc {
    let (outbox, rx) = Outbox::new(1024);
    let sessions = Arc::new(Sessions::new(
        claude_dir.to_path_buf(),
        outbox.clone(),
        Duration::from_millis(20),
            Vec::new(),
        ));
    let pending = Pending::new(outbox.clone());
    let cfg = link::Config {
        hub: format!("http://{hub_addr}"),
        token: token.to_string(),
        name: "test-node".to_string(),
        buffer_cap: 10_000,
    };
    let wrappers =
        saarthi_node::wrapper::Wrappers::new(sessions.clone(), pending.clone(), outbox.clone());
    let spawner =
        saarthi_node::spawner::Spawner::new(claude.into(), "/bin/sh".into(), roots, None, contained, sessions.clone());
    let parts = link::Parts {
        sessions: sessions.clone(),
        pending: pending.clone(),
        wrappers,
        spawner,
        keep_awake: Default::default(),
    };
    let task = tokio::spawn(link::run(cfg, rx, parts));
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let hooks = listener.local_addr().unwrap();
    let hold = saarthi_node::hooks::Hold {
        pending: pending.clone(),
        duration: hold,
        wrapped_duration: hold,
    };
    let app = saarthi_node::hooks::router(outbox.clone(), sessions.clone(), Some(hold));
    tokio::spawn(async move { axum_serve(listener, app).await });
    NodeProc {
        outbox,
        sessions,
        pending,
        hooks,
        task,
    }
}

async fn axum_serve(listener: TcpListener, app: axum::Router) {
    let _ = axum::serve(listener, app).await;
}

/// Posts a hook input the way Claude Code does and returns the response body.
pub async fn post_hook(addr: std::net::SocketAddr, body: String) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let request = format!(
        "POST /hook HTTP/1.1\r\nHost: {addr}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut reply = String::new();
    stream.read_to_string(&mut reply).await.unwrap();
    reply
        .split("\r\n\r\n")
        .nth(1)
        .unwrap_or_default()
        .to_string()
}
