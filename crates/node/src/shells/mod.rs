//! The shell holder (ADR-0078): `saarthi shells serve`, a per-user process apart from the
//! node that owns every shell a client opened, each a login shell under a PTY with its screen
//! kept in libghostty-vt, until the shell exits or is killed. Clients attach and detach as
//! they come and go: one attaching gets the screen as it is, then the live output; several
//! may be attached at once, each seeing the same and any typing; the shell takes the size of
//! whoever attached or resized last. The node reaches it over a unix socket, starting one
//! when none answers, so a node restart or upgrade leaves the shells running.

pub mod vt;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use portable_pty::{ChildKiller, CommandBuilder, MasterPty, PtySize, native_pty_system};
use saarthi_proto::shells::v1::shells_client::ShellsClient;
use saarthi_proto::shells::v1::shells_server::{Shells, ShellsServer};
use saarthi_proto::shells::v1::{
    AttachIn, AttachOut, Attached, KillRequest, KillResponse, ListRequest, ListResponse, Open, Resize, Shell as ShellInfo,
    attach_in, attach_out,
};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::{Request, Response, Status, Streaming};

/// Scrollback each shell keeps, in lines (ADR-0078).
pub const SCROLLBACK: usize = 10_000;

/// Output is read from a PTY in pieces of at most this.
const PIECE: usize = 64 * 1024;

/// Messages queued for one attached client. A client that falls this far behind is dropped
/// and attaches again, getting the screen as it is then.
const BACKLOG: usize = 1024;

/// Where the holder listens unless told: in the runtime directory where there is one (Linux),
/// else beside the node's other state.
pub fn default_socket() -> PathBuf {
    if let Some(run) = std::env::var_os("XDG_RUNTIME_DIR").filter(|d| Path::new(d).is_dir()) {
        return PathBuf::from(run).join("saarthi").join("shells.sock");
    }
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"));
    home.join(".local/state/saarthi/shells.sock")
}

fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// One shell: its PTY and process, and its screen with the clients attached to it.
struct Shell {
    id: String,
    started_ms: i64,
    state: Mutex<State>,
    writer: Mutex<Box<dyn Write + Send>>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
}

struct State {
    screen: vt::Screen,
    clients: Vec<(u64, mpsc::Sender<Result<AttachOut, Status>>)>,
    next_client: u64,
    cols: u16,
    rows: u16,
}

fn out(body: attach_out::Body) -> Result<AttachOut, Status> {
    Ok(AttachOut { body: Some(body) })
}

impl Shell {
    /// Resizes the PTY and the screen, the shell then redrawing for the new size.
    fn resize(&self, size: &Resize) {
        let (Ok(cols), Ok(rows)) = (u16::try_from(size.cols), u16::try_from(size.rows)) else { return };
        if cols == 0 || rows == 0 {
            return;
        }
        let px = |v: u32| u16::try_from(v).unwrap_or(0);
        let _ = self.master.lock().unwrap().resize(PtySize {
            rows,
            cols,
            pixel_width: px(size.width_px),
            pixel_height: px(size.height_px),
        });
        let mut st = self.state.lock().unwrap();
        st.screen.resize(cols, rows, size.width_px, size.height_px);
        st.cols = cols;
        st.rows = rows;
    }

    fn input(&self, bytes: &[u8]) -> bool {
        let mut w = self.writer.lock().unwrap();
        w.write_all(bytes).and_then(|()| w.flush()).is_ok()
    }

    /// Adds a client: `Attached`, the screen as it is, then whatever the shell writes next,
    /// with nothing lost or repeated between the two since both happen under the lock.
    fn attach(&self, tx: mpsc::Sender<Result<AttachOut, Status>>, started: bool) -> u64 {
        let mut st = self.state.lock().unwrap();
        let _ = tx.try_send(out(attach_out::Body::Attached(Attached { id: self.id.clone(), started })));
        let screen = st.screen.snapshot();
        let _ = tx.try_send(out(attach_out::Body::Output(screen)));
        let n = st.next_client;
        st.next_client += 1;
        st.clients.push((n, tx));
        n
    }

    fn detach(&self, client: u64) {
        self.state.lock().unwrap().clients.retain(|(n, _)| *n != client);
    }

    fn info(&self) -> ShellInfo {
        let mut st = self.state.lock().unwrap();
        ShellInfo {
            id: self.id.clone(),
            started_ms: self.started_ms,
            clients: st.clients.len() as u32,
            cols: u32::from(st.cols),
            rows: u32::from(st.rows),
            title: st.screen.title(),
        }
    }
}

/// The shells held, by id.
#[derive(Default)]
pub struct Holder {
    shells: Mutex<HashMap<String, Arc<Shell>>>,
}

impl Holder {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    fn get(&self, id: &str) -> Option<Arc<Shell>> {
        self.shells.lock().unwrap().get(id).cloned()
    }

    /// Starts a shell: `program`, or the owner's login shell when empty, in their home
    /// directory, at the client's size.
    fn start(self: &Arc<Self>, program: &str, size: &Resize) -> Result<Arc<Shell>, String> {
        let cols = u16::try_from(size.cols).ok().filter(|c| *c > 0).unwrap_or(80);
        let rows = u16::try_from(size.rows).ok().filter(|r| *r > 0).unwrap_or(24);
        let pty = native_pty_system()
            .openpty(PtySize { rows, cols, pixel_width: 0, pixel_height: 0 })
            .map_err(|e| format!("could not open a PTY: {e}"))?;
        let mut cmd = if program.is_empty() { CommandBuilder::new_default_prog() } else { CommandBuilder::new(program) };
        // portable-pty would otherwise start it in `$HOME` itself, and fail there when that
        // is not a directory (unset, or a build sandbox's `/homeless-shelter`).
        match std::env::var_os("HOME").filter(|h| Path::new(h).is_dir()) {
            Some(home) => cmd.cwd(home),
            None => cmd.cwd(std::env::current_dir().unwrap_or_else(|_| "/".into())),
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        // Nothing of a Claude session the holder was started from leaks into the shell.
        for (name, _) in std::env::vars_os() {
            let name = name.to_string_lossy();
            if name.starts_with("CLAUDE") || name == "AI_AGENT" {
                cmd.env_remove(name.as_ref());
            }
        }
        let mut child = pty.slave.spawn_command(cmd).map_err(|e| format!("could not start the shell: {e}"))?;
        drop(pty.slave);
        let reader = pty.master.try_clone_reader().map_err(|e| format!("could not read the PTY: {e}"))?;
        let writer = pty.master.take_writer().map_err(|e| format!("could not write the PTY: {e}"))?;
        let screen = vt::Screen::new(cols, rows, SCROLLBACK).ok_or("could not make the shell's screen")?;
        let id = uuid::Uuid::new_v4().simple().to_string()[..12].to_string();
        let shell = Arc::new(Shell {
            id: id.clone(),
            started_ms: now_ms(),
            state: Mutex::new(State { screen, clients: Vec::new(), next_client: 0, cols, rows }),
            writer: Mutex::new(writer),
            master: Mutex::new(pty.master),
            killer: Mutex::new(child.clone_killer()),
        });
        self.shells.lock().unwrap().insert(id.clone(), shell.clone());
        tracing::info!(%id, cols, rows, "a shell started");

        // The shell's output: into its screen, and to every client attached, in one step.
        let s = shell.clone();
        std::thread::spawn(move || read_output(s, reader));
        // Its end: every client told why, and the shell dropped.
        let (holder, s) = (self.clone(), shell.clone());
        std::thread::spawn(move || {
            let status = child.wait().ok();
            let why = match status {
                Some(s) if s.success() => "the shell exited".to_string(),
                Some(s) => format!("the shell exited with {}", s.exit_code()),
                None => "the shell ended".to_string(),
            };
            // Whatever it wrote last reaches the clients first.
            std::thread::sleep(Duration::from_millis(50));
            holder.shells.lock().unwrap().remove(&s.id);
            let clients = std::mem::take(&mut s.state.lock().unwrap().clients);
            for (_, tx) in clients {
                let _ = tx.try_send(out(attach_out::Body::Ended(why.clone())));
            }
            tracing::info!(id = %s.id, %why, "a shell ended");
        });
        Ok(shell)
    }
}

fn read_output(shell: Arc<Shell>, mut reader: Box<dyn Read + Send>) {
    let mut buf = vec![0u8; PIECE];
    loop {
        let n = match reader.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => n,
        };
        let mut st = shell.state.lock().unwrap();
        st.screen.write(&buf[..n]);
        // A client too far behind is let go; it attaches again and gets the screen as it is.
        st.clients.retain(|(_, tx)| tx.try_send(out(attach_out::Body::Output(buf[..n].to_vec()))).is_ok());
    }
}

/// The holder as the gRPC service.
#[derive(Clone)]
pub struct Service(Arc<Holder>);

#[tonic::async_trait]
impl Shells for Service {
    async fn list(&self, _: Request<ListRequest>) -> Result<Response<ListResponse>, Status> {
        let shells: Vec<Arc<Shell>> = self.0.shells.lock().unwrap().values().cloned().collect();
        let mut shells: Vec<ShellInfo> = shells.iter().map(|s| s.info()).collect();
        shells.sort_by_key(|s| s.started_ms);
        Ok(Response::new(ListResponse { shells }))
    }

    type AttachStream = ReceiverStream<Result<AttachOut, Status>>;

    async fn attach(&self, request: Request<Streaming<AttachIn>>) -> Result<Response<Self::AttachStream>, Status> {
        let mut inbound = request.into_inner();
        let Some(AttachIn { body: Some(attach_in::Body::Open(Open { id, program, size })) }) = inbound.message().await? else {
            return Err(Status::invalid_argument("an attach starts with Open"));
        };
        let size = size.unwrap_or_default();
        let (shell, started) = if id.is_empty() {
            (self.0.start(&program, &size).map_err(Status::internal)?, true)
        } else {
            (self.0.get(&id).ok_or_else(|| Status::not_found(format!("no shell {id}: it ended or was killed")))?, false)
        };
        // The shell takes the size of whoever attached last, before its screen is sent.
        if !started {
            shell.resize(&size);
        }
        let (tx, rx) = mpsc::channel(BACKLOG);
        let client = shell.attach(tx, started);
        tracing::info!(id = %shell.id, started, "a client attached");
        tokio::spawn(async move {
            while let Ok(Some(m)) = inbound.message().await {
                match m.body {
                    Some(attach_in::Body::Input(bytes)) => {
                        if !shell.input(&bytes) {
                            break;
                        }
                    }
                    Some(attach_in::Body::Resize(size)) => shell.resize(&size),
                    _ => {}
                }
            }
            shell.detach(client);
            tracing::info!(id = %shell.id, "a client detached");
        });
        Ok(Response::new(ReceiverStream::new(rx)))
    }

    async fn kill(&self, request: Request<KillRequest>) -> Result<Response<KillResponse>, Status> {
        let id = request.into_inner().id;
        let killed = match self.0.get(&id) {
            Some(shell) => shell.killer.lock().unwrap().kill().is_ok(),
            None => false,
        };
        Ok(Response::new(KillResponse { killed }))
    }
}

/// Serves the holder on `socket` until the process ends: its directory made private, a stale
/// socket from an earlier holder replaced, a live one left alone (it is an error to start
/// a second).
pub async fn serve(socket: &Path) -> anyhow::Result<()> {
    if let Some(dir) = socket.parent() {
        std::fs::create_dir_all(dir)?;
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(dir, std::fs::Permissions::from_mode(0o700))?;
    }
    if socket.exists() {
        if connect(socket).await.is_ok() {
            anyhow::bail!("a shell holder already listens on {}", socket.display());
        }
        std::fs::remove_file(socket)?;
    }
    let listener = tokio::net::UnixListener::bind(socket)?;
    tracing::info!(socket = %socket.display(), "holding shells");
    tonic::transport::Server::builder()
        .add_service(ShellsServer::new(Service(Holder::new())))
        .serve_with_incoming(tokio_stream::wrappers::UnixListenerStream::new(listener))
        .await?;
    Ok(())
}

/// A client of the holder on `socket`.
pub async fn connect(socket: &Path) -> anyhow::Result<ShellsClient<tonic::transport::Channel>> {
    let path = socket.to_path_buf();
    let channel = tonic::transport::Endpoint::from_static("http://shells.saarthi")
        .connect_with_connector(tower::service_fn(move |_: tonic::transport::Uri| {
            let path = path.clone();
            async move { Ok::<_, std::io::Error>(hyper_util::rt::TokioIo::new(tokio::net::UnixStream::connect(path).await?)) }
        }))
        .await?;
    Ok(ShellsClient::new(channel))
}

/// The holder on `socket`, started when none answers: this program as `saarthi shells
/// serve`, in a session of its own so it outlives the node, its output in the log beside the
/// socket. Under the node module a holder runs as a service of its own (ADR-0078); this is
/// for a node run by hand.
pub async fn reach(socket: &Path) -> anyhow::Result<ShellsClient<tonic::transport::Channel>> {
    if let Ok(c) = connect(socket).await {
        return Ok(c);
    }
    let exe = std::env::current_exe()?;
    let log = socket.with_extension("log");
    if let Some(dir) = socket.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let file = std::fs::OpenOptions::new().create(true).append(true).open(&log)?;
    let mut cmd = std::process::Command::new(exe);
    cmd.args(["shells", "serve", "--socket"]).arg(socket);
    cmd.stdin(std::process::Stdio::null()).stdout(file.try_clone()?).stderr(file);
    use std::os::unix::process::CommandExt;
    // Its own session: the node's ending, or a signal to its group, does not reach it.
    unsafe {
        cmd.pre_exec(|| {
            libc::setsid();
            Ok(())
        });
    }
    let child = cmd.spawn()?;
    tracing::info!(pid = child.id(), socket = %socket.display(), "started a shell holder");
    for _ in 0..50 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        if let Ok(c) = connect(socket).await {
            return Ok(c);
        }
    }
    anyhow::bail!("the shell holder did not answer on {}", socket.display())
}
