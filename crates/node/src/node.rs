//! A node built and run by its owner (ADR-0080): `saarthi node serve`, which is the machine's
//! node, or an app that embeds one. Everything a node binds or writes derives from its
//! state directory unless given, and nothing it holds is the process's, so several can run
//! in one process beside the machine's daemon.
//!
//! ```no_run
//! # async fn f() -> anyhow::Result<()> {
//! use saarthi_node::node::{Hub, Node, Options};
//! let mut opts = Options::new("my-app", "/tmp/my-app-node".into());
//! opts.hub = Some(Hub::direct("http://hub:7700", "node token"));
//! opts.spawn_roots = vec!["/srv/work".into()];
//! let node = Node::start(opts).await?;
//! // ... the app's clients spawn sessions on it through the hub ...
//! node.stop().await;
//! # Ok(()) }
//! ```
//!
//! An embedded node sees only the sessions it starts. They run in a Claude config directory
//! of its own, `<state>/claude`, under the hub's Claude tokens through the node's proxy
//! (ADR-0051): the directory has no login. The machine's node ([`Machine`]) alone watches
//! the user's Claude directory, serves `saarthi run` and the terminal, and reads the usage.

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use tokio::task::JoinHandle;

use crate::containment::Profiles;
use crate::link::{self, HubAddr};
use crate::outbox::Outbox;
use crate::pending::Pending;
use crate::services::Services;
use crate::sessions::Sessions;
use crate::spawner::{self, Contained, Spawner};
use crate::wrapper::Wrappers;
use crate::{awake, health, hooks, registry, sink, usage};

/// Seconds Claude Code waits for a hook of the node's before giving up on it.
pub const HOOK_TIMEOUT_SECS: u32 = 600;

/// How often joining the hub's tailnet is tried again after it failed.
const TAILNET_RETRY: Duration = Duration::from_secs(30);

/// The most bytes a unix socket's path may have, its NUL not counted: 104 bytes of
/// `sun_path` on macOS, 108 on Linux.
const SOCKET_PATH_MAX: usize = if cfg!(target_os = "macos") { 103 } else { 107 };

/// The hub a node links to.
pub struct Hub {
    pub addr: HubAddr,
    /// The node's token at the hub, its identity there.
    pub token: String,
    /// How to join the tailnet when `addr` is reached through one (`HubAddr::via_tailnet`).
    pub tailnet: Tailnet,
}

impl Hub {
    /// A hub on the host's network.
    pub fn direct(url: impl Into<String>, token: impl Into<String>) -> Self {
        Self {
            addr: HubAddr::direct(url.into()),
            token: token.into(),
            tailnet: Tailnet::default(),
        }
    }
}

/// A tailnet identity of the node's own (ADR-0048).
#[derive(Default)]
pub struct Tailnet {
    /// The auth key for the first join, and again if the tailnet logs the node out.
    pub authkey: Option<saarthi_tailnet::AuthKey>,
    /// The machine name on the tailnet; `saarthi-<name>` when `None`.
    pub hostname: Option<String>,
    /// Where the identity keeps its state; `<state>/tailnet` when `None`.
    pub state: Option<PathBuf>,
}

/// What only the machine's node does (ADR-0080): one per user per machine.
pub struct Machine {
    /// The user's home, for the health sample's disk.
    pub home: PathBuf,
    /// The user's Claude config directory, whose session registry it scans.
    pub claude_dir: PathBuf,
    /// Claude Code's global state file, whose cached usage goes with health when the usage
    /// endpoint is not polled.
    pub claude_json: PathBuf,
    pub registry_every: Duration,
    /// The socket `saarthi run` wrappers connect to.
    pub wrapper_socket: PathBuf,
    /// Whether clients may open a shell here (ADR-0054), kept by the holder on `shells`.
    pub terminal: bool,
    pub shells_socket: PathBuf,
    /// Poll the account's usage endpoint (ADR-0030).
    pub usage: Option<Usage>,
}

pub struct Usage {
    pub store: usage::Store,
    pub url: String,
    pub every: Duration,
}

/// What a node is built from. [`Options::new`] gives an embedded node's defaults.
pub struct Options {
    /// The hub to link to; `None` writes every event to stdout as JSON lines.
    pub hub: Option<Hub>,
    /// The name it registers under; the hub keys nodes by token, not by name.
    pub name: String,
    /// Where everything the node binds or writes lives unless given below. Two nodes may
    /// not share one.
    pub state_dir: PathBuf,
    /// The hook endpoint; port 0 takes a free one.
    pub hook_addr: SocketAddr,
    /// The claude binary, resolved by the login shell `shell`.
    pub claude: String,
    pub shell: PathBuf,
    /// Directories clients may spawn sessions under (ADR-0034); none turns spawning off.
    pub spawn_roots: Vec<PathBuf>,
    pub profiles: Profiles,
    pub services: Services,
    /// The user's Claude config directory, which `claude_files` and the profiles' Claude
    /// files are linked from.
    pub user_claude: PathBuf,
    /// Entries of `user_claude` linked into an embedded node's own config directory, such
    /// as `CLAUDE.md` or `skills` (as a profile's, ADR-0038).
    pub claude_files: Vec<String>,
    /// Working directories whose sessions are not tracked (ADR-0028).
    pub ignore_cwd: Vec<glob::Pattern>,
    pub transcript_poll: Duration,
    pub health_every: Duration,
    /// How long a permission dialog waits for the hub, for a bare session and one under
    /// `saarthi run`.
    pub hold: Duration,
    pub wrapped_hold: Duration,
    /// Unacked events kept while the hub is unreachable.
    pub buffer_events: usize,
    /// Keep the machine awake while a session is busy, at start (ADR-0031).
    pub keep_awake: bool,
    /// `<state>/changes.sock` when `None`.
    pub changes_socket: Option<PathBuf>,
    /// The hooks of sandboxed sessions; `<state>/hook.sock` when `None`.
    pub hook_socket: Option<PathBuf>,
    /// The daemon's parts; `None` for an embedded node.
    pub machine: Option<Machine>,
}

impl Options {
    /// An embedded node with nothing to spawn under yet. `user_claude` is `~/.claude` when
    /// `$HOME` is set.
    pub fn new(name: impl Into<String>, state_dir: PathBuf) -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
        Self {
            hub: None,
            name: name.into(),
            state_dir,
            hook_addr: SocketAddr::from(([127, 0, 0, 1], 0)),
            claude: "claude".into(),
            shell: spawner::login_shell(),
            spawn_roots: Vec::new(),
            profiles: Profiles::default(),
            services: Services::default(),
            user_claude: home.join(".claude"),
            claude_files: Vec::new(),
            ignore_cwd: Vec::new(),
            transcript_poll: Duration::from_millis(500),
            health_every: Duration::from_secs(30),
            hold: Duration::from_secs(120),
            wrapped_hold: Duration::from_secs(540),
            buffer_events: 200_000,
            keep_awake: false,
            changes_socket: None,
            hook_socket: None,
            machine: None,
        }
    }
}

/// A running node. Dropping it leaves its tasks running; [`Node::stop`] ends them.
pub struct Node {
    hook_addr: SocketAddr,
    claude_dir: PathBuf,
    spawner: Arc<Spawner>,
    link: JoinHandle<()>,
    tasks: Vec<JoinHandle<()>>,
    _lock: std::fs::File,
}

impl Node {
    /// Builds the node and starts it on the current Tokio runtime, which must be
    /// multi-threaded (the health sampler blocks in place). Fails when the state directory
    /// is held by another node or its sockets' paths are too long, or a socket or the hook
    /// port cannot be bound.
    pub async fn start(opts: Options) -> Result<Node> {
        let state = &opts.state_dir;
        std::fs::create_dir_all(state).with_context(|| format!("creating {}", state.display()))?;
        let lock = lock(state)?;
        let own = opts.machine.is_none();
        let changes_socket = opts.changes_socket.clone().unwrap_or_else(|| state.join("changes.sock"));
        let hook_socket = opts.hook_socket.clone().unwrap_or_else(|| state.join("hook.sock"));
        let api_dir = own.then(|| state.join("api"));
        let longest = [changes_socket.clone(), hook_socket.clone()]
            .into_iter()
            .chain(api_dir.iter().map(|d| d.join(crate::claude_api::socket_name())))
            .max_by_key(|p| p.as_os_str().len())
            .expect("not empty");
        check_socket_path(&longest)?;

        let claude_dir = match &opts.machine {
            Some(m) => m.claude_dir.clone(),
            None => state.join("claude"),
        };
        // `~/.local/state/saarthi/uploads` for the daemon on its default state directory.
        let uploads = state.join("uploads");

        let (outbox, rx) = Outbox::new(1024);
        let sessions = Arc::new(Sessions::new(
            claude_dir.clone(),
            outbox.clone(),
            opts.transcript_poll,
            opts.ignore_cwd.clone(),
        ));
        let pending = Pending::new(outbox.clone());
        let wrappers = Wrappers::new(sessions.clone(), pending.clone(), outbox.clone());
        let mut tasks = Vec::new();
        if let Some(m) = &opts.machine {
            let listener = Wrappers::bind(&m.wrapper_socket)?;
            tracing::info!(socket = %m.wrapper_socket.display(), "listening for wrappers");
            tasks.push(tokio::spawn(wrappers.clone().serve(listener)));
        }

        // A session's changes, served by the node itself (ADR-0067).
        let mut services = opts.services;
        if services.add_builtin(crate::changes::NAME, changes_socket.clone()) {
            let sessions = sessions.clone();
            tasks.push(tokio::spawn(async move {
                if let Err(e) = crate::changes::serve(changes_socket, sessions).await {
                    tracing::error!(error = %format!("{e:#}"), "a session's changes cannot be served");
                }
            }));
        } else {
            tracing::warn!("the services file names a service changes; it is served instead of the sessions' changes");
        }
        let services = Arc::new(services);
        for name in services.names() {
            tracing::info!(service = %name, "local service offered through the hub");
        }
        tasks.push(tokio::spawn(services.clone().reap_idle()));

        // The hook endpoint is bound before the spawner is made, which gives its port to
        // the sessions it starts.
        let listener = tokio::net::TcpListener::bind(opts.hook_addr)
            .await
            .with_context(|| format!("binding the hook endpoint on {}", opts.hook_addr))?;
        let hook_addr = listener.local_addr()?;
        let hooks_block = hooks::settings(hook_addr, HOOK_TIMEOUT_SECS);
        if own {
            prepare_claude_dir(&claude_dir, &opts.user_claude, &opts.claude_files, &hooks_block)?;
        } else if let Err(e) = crate::claude_api::make_host_socket_dir() {
            tracing::warn!(error = %e, "the Claude API sockets' directory could not be made");
        }
        let api_deny = api_dir.clone().unwrap_or_else(crate::claude_api::host_socket_dir);
        let contained = Contained {
            hooks: Some(hooks::socket_settings(&hook_socket, HOOK_TIMEOUT_SECS)),
            user_claude: opts.user_claude.clone(),
            // The wrappers' socket would let a sandbox pose as `saarthi run`, the proxies of
            // host and terminal sessions spend the hub's tokens (ADR-0051), and the local
            // services answer the owner (ADR-0052).
            deny: opts
                .machine
                .iter()
                .map(|m| m.wrapper_socket.clone())
                .chain([api_deny])
                .chain(services.sockets())
                .collect(),
            profiles: opts.profiles,
            own_claude: own.then(|| claude_dir.clone()),
            api_dir,
        };
        // The hooks block `saarthi node hooks` prints by default, so it matches the user's own
        // and Claude Code asks each hook once (EXP-20261106-qt94y4).
        let spawner = Spawner::new(
            opts.claude.clone(),
            opts.shell.clone(),
            &opts.spawn_roots,
            Some(hooks_block),
            contained,
            sessions.clone(),
        );
        let sandboxed = spawner.profiles().any_sandboxed();
        let profile_dirs = spawner.claude_dirs();
        for p in spawner.profiles().all() {
            tracing::info!(profile = %p.name, backend = ?p.backend, "launch profile");
        }
        if !opts.spawn_roots.is_empty() {
            tracing::info!(roots = ?spawner.roots(), "spawning allowed");
        }
        let keep_awake = Arc::new(AtomicBool::new(opts.keep_awake));

        // The sessions running now, read before the hub link starts: the hub ends any
        // session it has as live that the registration does not report.
        let registry_dir = claude_dir.join("sessions");
        if opts.machine.is_some() {
            sessions.apply_registry(&scan_registry(&registry_dir, &profile_dirs).await).await;
        }
        let mut hold = None;
        let mut asking = None;
        let link = match opts.hub {
            Some(hub) => {
                wrappers.set_hub(hub.addr.clone(), hub.token.clone());
                if hub.addr.is_via_tailnet() {
                    let t = hub.tailnet;
                    let opts = saarthi_tailnet::Options {
                        name: "hub".to_string(),
                        dir: t.state.unwrap_or_else(|| state.join("tailnet")),
                        hostname: t.hostname.unwrap_or_else(|| format!("saarthi-{}", opts.name)),
                        authkey: t.authkey,
                        ephemeral: false,
                        give_up_after: None,
                    };
                    tasks.push(tokio::spawn(join_tailnet(hub.addr.clone(), opts)));
                }
                let cfg = link::Config {
                    hub: hub.addr,
                    token: hub.token,
                    name: opts.name.clone(),
                    buffer_cap: opts.buffer_events,
                    terminal: opts.machine.as_ref().and_then(|m| m.terminal.then(String::new)),
                    shells: match &opts.machine {
                        Some(m) => m.shells_socket.clone(),
                        None => state.join("shells.sock"),
                    },
                };
                hold = Some(hooks::Hold {
                    pending: pending.clone(),
                    duration: opts.hold,
                    wrapped_duration: opts.wrapped_hold,
                    upload_roots: std::iter::once(uploads.clone())
                        .chain(spawner.profiles().all().filter_map(|p| p.state_dir.as_deref().map(crate::uploads::profile_root)))
                        .collect(),
                });
                // Spawned sessions ask their questions through the hook port (ADR-0061).
                let questions = spawner.questions();
                questions.set_url(format!("http://{hook_addr}/mcp"));
                asking = Some((questions, pending.clone()));
                let parts = link::Parts {
                    sessions: sessions.clone(),
                    pending,
                    wrappers,
                    spawner: spawner.clone(),
                    keep_awake: keep_awake.clone(),
                    services: services.clone(),
                    queue: crate::queue::Queue::new(sessions.clone()),
                    uploads,
                };
                tasks.push(link::run_queue(&parts));
                tokio::spawn(link::run(cfg, rx, parts))
            }
            None => tokio::spawn(sink::stdout(rx)),
        };

        tracing::info!(name = %opts.name, addr = %hook_addr, claude_dir = %claude_dir.display(), "node serving");
        let mut app = hooks::router(outbox.clone(), sessions.clone(), hold.clone());
        if let Some((questions, pending)) = asking {
            app = app.merge(crate::questions::router(questions, pending));
        }
        tasks.push(tokio::spawn(async move {
            let _ = axum::serve(listener, app).await;
        }));
        if sandboxed {
            if let Some(dir) = hook_socket.parent() {
                std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
            }
            let _ = std::fs::remove_file(&hook_socket);
            let listener = tokio::net::UnixListener::bind(&hook_socket)
                .with_context(|| format!("binding the hook socket {}", hook_socket.display()))?;
            tracing::info!(socket = %hook_socket.display(), "hooks of sandboxed sessions");
            let app = hooks::router(outbox.clone(), sessions.clone(), hold);
            tasks.push(tokio::spawn(async move {
                let _ = axum::serve(listener, app).await;
            }));
        }

        let (home, mut cached_usage, awake_every) = match &opts.machine {
            Some(m) => (m.home.clone(), None, m.registry_every),
            None => (state.clone(), None, Duration::from_secs(2)),
        };
        if let Some(m) = opts.machine {
            let scan = sessions.clone();
            let every = m.registry_every;
            tasks.push(tokio::spawn(async move {
                let mut tick = tokio::time::interval(every);
                // The first scan ran before the link started.
                tick.tick().await;
                loop {
                    tick.tick().await;
                    scan.apply_registry(&scan_registry(&registry_dir, &profile_dirs).await).await;
                }
            }));
            match m.usage {
                Some(u) => tasks.push(tokio::spawn(poll_usage(u, outbox.clone()))),
                // With the usage polled the poller reports the same metrics live; Claude
                // Code's cached value, written only by /usage, would interleave hours-old
                // numbers with them.
                None => cached_usage = Some(m.claude_json),
            }
        }
        let awake = keep_awake_parts(&sessions, &keep_awake, awake_every, &outbox, &mut tasks).await;
        tasks.push(tokio::spawn(sample_health(home, cached_usage, opts.health_every, awake, outbox.clone())));

        Ok(Node {
            hook_addr,
            claude_dir,
            spawner,
            link,
            tasks,
            _lock: lock,
        })
    }

    /// The hook endpoint, with the port it was given.
    pub fn hook_addr(&self) -> SocketAddr {
        self.hook_addr
    }

    /// The Claude config directory its sessions run in.
    pub fn claude_dir(&self) -> &Path {
        &self.claude_dir
    }

    /// Resolves when the link to the hub (or the stdout sink) ends, which happens only when
    /// the node fails.
    pub async fn finished(&mut self) {
        let _ = (&mut self.link).await;
    }

    /// Ends the sessions it spawned (their input closes; those still running after the grace
    /// period are killed), then its tasks. Their transcripts stay in the state directory, so
    /// a node started again on it can resume them.
    pub async fn stop(self) {
        self.spawner.end_all().await;
        // The sessions' ends reach the hub before the link goes, when it is up.
        tokio::time::sleep(Duration::from_millis(200)).await;
        self.link.abort();
        for t in self.tasks {
            t.abort();
        }
    }
}

/// Takes the state directory's lock, held for as long as the returned file is open.
fn lock(state: &Path) -> Result<std::fs::File> {
    use std::os::fd::AsRawFd;
    let path = state.join("node.lock");
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .with_context(|| format!("opening {}", path.display()))?;
    // SAFETY: flock only reads the descriptor, which the file keeps open.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        bail!("{}: another node runs on this state directory; give this one its own", state.display());
    }
    Ok(file)
}

fn check_socket_path(path: &Path) -> Result<()> {
    let len = path.as_os_str().len();
    if len > SOCKET_PATH_MAX {
        bail!(
            "{}: a socket's path would be {len} bytes, and {SOCKET_PATH_MAX} is the most this system takes; choose a shorter state directory",
            path.display()
        );
    }
    Ok(())
}

/// Makes an embedded node's Claude config directory: the user's files it was told to link,
/// and its hooks in `settings.json`, so a session started there reaches this node and no
/// other. A `settings.json` linked from the user's is left alone.
fn prepare_claude_dir(dir: &Path, user_claude: &Path, files: &[String], hooks: &serde_json::Value) -> Result<()> {
    std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    for f in files {
        let (from, to) = (user_claude.join(f), dir.join(f));
        if from.exists() && std::fs::symlink_metadata(&to).is_err() {
            std::os::unix::fs::symlink(&from, &to).with_context(|| format!("linking {}", to.display()))?;
        }
    }
    let settings = dir.join("settings.json");
    if std::fs::symlink_metadata(&settings).is_ok_and(|m| m.file_type().is_symlink()) {
        tracing::warn!(path = %settings.display(), "settings.json is linked from the user's; the node's hooks reach its sessions by --settings only");
        return Ok(());
    }
    std::fs::write(&settings, serde_json::to_string_pretty(hooks)?)
        .with_context(|| format!("writing {}", settings.display()))
}

/// Joins the hub's tailnet, again every 30 s until it works: the auth key may arrive after
/// the node starts (on macOS nothing orders the node's launchd agent after agenix's). Until
/// then the link retries; a tailnet that cannot be joined costs the link, not the node
/// (ADR-0044).
async fn join_tailnet(hub: HubAddr, opts: saarthi_tailnet::Options) {
    let mut last = None::<String>;
    loop {
        match saarthi_tailnet::Tailnet::start(opts.clone()).await {
            Ok(t) => {
                hub.tailnet_up(t);
                return;
            }
            Err(e) => {
                let e = format!("{e:#}");
                if last.as_ref() != Some(&e) {
                    tracing::error!(error = %e, "the hub's tailnet could not be joined; trying again every 30 s");
                    last = Some(e);
                }
                tokio::time::sleep(TAILNET_RETRY).await;
            }
        }
    }
}

/// The live sessions in Claude Code's registry, the host's and each launch profile's.
async fn scan_registry(dir: &Path, profile_dirs: &[(String, PathBuf)]) -> Vec<registry::Record> {
    let (dir, others) = (dir.to_path_buf(), profile_dirs.to_vec());
    tokio::task::spawn_blocking(move || {
        let mut live = registry::live(&dir);
        for (profile, claude_dir) in others {
            live.extend(registry::live(&claude_dir.join("sessions")).into_iter().map(|mut r| {
                r.claude_dir = Some(claude_dir.clone());
                r.profile = profile.clone();
                r
            }));
        }
        live
    })
    .await
    .unwrap_or_default()
}

async fn poll_usage(u: Usage, outbox: Outbox) {
    let client = reqwest::Client::new();
    let mut wait = u.every;
    loop {
        match usage::poll(&client, &u.store, &u.url).await {
            Ok(sample) => {
                outbox.health(sample).await;
                wait = u.every;
            }
            Err(e) => match e.downcast_ref::<usage::TooMany>() {
                Some(usage::TooMany(after)) => {
                    wait = usage::backoff(u.every, wait, *after);
                    tracing::warn!(wait_s = wait.as_secs(), "usage endpoint refused, too many requests; backing off");
                }
                None => tracing::warn!(error = %format!("{e:#}"), "usage poll failed"),
            },
        }
        tokio::time::sleep(wait).await;
    }
}

/// The keep-awake switch and whether the machine is held, when it can be held at all.
type Awake = Option<(Arc<AtomicBool>, awake::Held)>;

async fn keep_awake_parts(
    sessions: &Arc<Sessions>,
    enabled: &Arc<AtomicBool>,
    every: Duration,
    outbox: &Outbox,
    tasks: &mut Vec<JoinHandle<()>>,
) -> Awake {
    let hold = awake::Hold::detect().await?;
    let held: awake::Held = Arc::new(AtomicBool::new(false));
    tasks.push(tokio::spawn(awake::keep(
        hold,
        sessions.clone(),
        enabled.clone(),
        held.clone(),
        every.max(Duration::from_secs(1)),
        Some(outbox.clone()),
    )));
    Some((enabled.clone(), held))
}

async fn sample_health(home: PathBuf, cached: Option<PathBuf>, every: Duration, awake: Awake, outbox: Outbox) {
    let mut sampler = health::Sampler::new(home, cached);
    let mut tick = tokio::time::interval(every);
    // The first tick is immediate; skip it so CPU usage has an interval to measure.
    tick.tick().await;
    loop {
        tick.tick().await;
        let mut sample = tokio::task::block_in_place(|| sampler.sample());
        let flag = |b: &AtomicBool| if b.load(Ordering::Relaxed) { 1.0 } else { 0.0 };
        // Only where the machine can be held up: a client shows the switch when the metric
        // is there, so a node that cannot hold offers none.
        if let Some((enabled, held)) = &awake {
            sample.metrics.push(saarthi_proto::event::v1::Metric {
                name: "node.keep_awake".into(),
                value: flag(held),
            });
            sample.metrics.push(saarthi_proto::event::v1::Metric {
                name: "node.keep_awake_enabled".into(),
                value: flag(enabled),
            });
        }
        outbox.health(sample).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options(state: &Path) -> Options {
        let mut o = Options::new("t", state.to_path_buf());
        o.user_claude = state.join("no-user-claude");
        o
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_state_directory_holds_one_node() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("s");
        let first = Node::start(options(&state)).await.unwrap();
        let second = Node::start(options(&state)).await.err().expect("refused");
        assert!(format!("{second:#}").contains("another node runs on this state directory"), "{second:#}");
        let other = Node::start(options(&dir.path().join("t"))).await.unwrap();
        assert_ne!(first.hook_addr(), other.hook_addr());
        first.stop().await;
        // Free again once the first has stopped.
        Node::start(options(&state)).await.unwrap().stop().await;
        other.stop().await;
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_state_directory_too_long_for_its_sockets_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let state = dir.path().join("x".repeat(90));
        let e = Node::start(options(&state)).await.err().expect("refused");
        assert!(format!("{e:#}").contains("choose a shorter state directory"), "{e:#}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_embedded_node_keeps_its_sessions_in_a_claude_dir_of_its_own() {
        let dir = tempfile::tempdir().unwrap();
        let user = dir.path().join("user-claude");
        std::fs::create_dir_all(&user).unwrap();
        std::fs::write(user.join("CLAUDE.md"), "be kind").unwrap();
        let state = dir.path().join("s");
        let mut o = options(&state);
        o.user_claude = user.clone();
        o.claude_files = vec!["CLAUDE.md".into(), "skills".into()];
        let node = Node::start(o).await.unwrap();
        assert_eq!(node.claude_dir(), state.join("claude"));
        assert_eq!(std::fs::read_link(state.join("claude/CLAUDE.md")).unwrap(), user.join("CLAUDE.md"));
        // Missing in the user's dir, so not linked.
        assert!(std::fs::symlink_metadata(state.join("claude/skills")).is_err());
        let settings = std::fs::read_to_string(state.join("claude/settings.json")).unwrap();
        assert!(settings.contains(&format!("http://{}/hook", node.hook_addr())), "{settings}");
        // The changes service is the node's own.
        assert!(state.join("changes.sock").exists());
        node.stop().await;
    }
}
