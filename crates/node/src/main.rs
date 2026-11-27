//! `saarthi`: the node side of saarthi. `saarthi node serve` runs the daemon; `saarthi node hooks`
//! prints the settings block that points Claude Code's hooks at it.

use std::io::IsTerminal;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};

use saarthi_node::containment::Profiles;
use saarthi_node::link::HubAddr;
use saarthi_node::node::{self, HOOK_TIMEOUT_SECS};
use saarthi_node::{hooks, spawner, usage, wrapper};

mod ctl;
mod tui;

#[derive(Parser)]
#[command(
    name = "saarthi",
    version,
    about = "saarthi: the node daemon that observes and gates agent sessions on this machine, and a client of the hub"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    #[command(subcommand)]
    Node(Node),
    /// Talk to the hub's control service as a client.
    Ctl(CtlArgs),
    /// Run Claude Code under a PTY the node daemon can interrupt and kill.
    Run(RunArgs),
    /// A dashboard of the hub in the terminal: sessions by node, what waits for you, the
    /// Claude accounts; allow, deny, answer, prompt, interrupt, end and start sessions.
    Tui(TuiArgs),
    /// The shell holder (ADR-0078).
    #[command(subcommand)]
    Shells(Shells),
}

#[derive(Subcommand)]
enum Shells {
    /// Keep the shells clients open on this machine, apart from the node, so they outlive
    /// its restarts: each until it exits or is killed. The node starts one when none answers;
    /// the node module runs it as a service of its own.
    Serve {
        /// Where to listen. Defaults to `$XDG_RUNTIME_DIR/saarthi/shells.sock`, else
        /// `~/.local/state/saarthi/shells.sock`.
        #[arg(long)]
        socket: Option<PathBuf>,
    },
}

#[derive(Args)]
struct TuiArgs {
    /// The hub, `http://host:port`. Defaults to `hub` in `~/.config/saarthi/ctl.toml`.
    #[arg(long, env = "SAARTHI_HUB")]
    hub: Option<String>,
    /// A file holding this client's hub token. Defaults to `token_file` in
    /// `~/.config/saarthi/ctl.toml`.
    #[arg(long, env = "SAARTHI_TOKEN_FILE")]
    token_file: Option<PathBuf>,
}

#[derive(Args)]
struct RunArgs {
    /// The program to run.
    #[arg(long, default_value = "claude")]
    command: String,
    /// The node daemon's wrapper socket; the daemon's default when omitted.
    #[arg(long)]
    wrapper_socket: Option<PathBuf>,
    /// Run under this one of the hub's Claude tokens (ADR-0046). Without it the program
    /// runs under the node's default account, as set from the app, or the machine's own
    /// login when the node has none.
    #[arg(long, conflicts_with = "own_login")]
    claude_token: Option<String>,
    /// Run under the machine's own Claude login, whatever the node's default.
    #[arg(long)]
    own_login: bool,
    /// Arguments for the program, after `--`.
    #[arg(last = true)]
    args: Vec<String>,
}

#[derive(Args)]
struct CtlArgs {
    /// The hub, `http://host:port`. Defaults to `hub` in `~/.config/saarthi/ctl.toml`.
    #[arg(long, env = "SAARTHI_HUB")]
    hub: Option<String>,
    /// A file holding this client's hub token. Defaults to `token_file` in
    /// `~/.config/saarthi/ctl.toml`.
    #[arg(long, env = "SAARTHI_TOKEN_FILE")]
    token_file: Option<PathBuf>,
    #[command(subcommand)]
    cmd: ctl::Ctl,
}

#[derive(Subcommand)]
enum Node {
    /// Run the daemon. Until the hub link exists, everything it observes goes to stdout as
    /// JSON lines.
    Serve(Box<Serve>),
    /// Print the `hooks` block for ~/.claude/settings.json.
    Hooks {
        #[arg(long, default_value = "127.0.0.1:7777")]
        hook_addr: SocketAddr,
        /// Seconds Claude waits for a hook before giving up on it.
        #[arg(long, default_value_t = HOOK_TIMEOUT_SECS)]
        timeout: u32,
    },
}

#[derive(Args)]
struct Serve {
    #[arg(long, default_value = "127.0.0.1:7777")]
    hook_addr: SocketAddr,
    /// Claude Code's config directory: `sessions/` and `projects/` live here.
    #[arg(long, env = "CLAUDE_CONFIG_DIR")]
    claude_dir: Option<PathBuf>,
    /// Claude Code's global state file, which holds the cached usage utilization. Defaults to
    /// `.claude.json` in the home directory, or inside `--claude-dir` when that is given.
    #[arg(long)]
    claude_json: Option<PathBuf>,
    #[arg(long, default_value_t = 30)]
    health_secs: u64,
    #[arg(long, default_value_t = 2)]
    registry_secs: u64,
    #[arg(long, default_value_t = 500)]
    transcript_poll_ms: u64,
    /// The hub's node service, `http://host:port`. Without it everything goes to stdout as
    /// JSON lines.
    #[arg(long, requires = "token_file")]
    hub: Option<String>,
    /// A file holding this node's pre-shared hub token.
    #[arg(long)]
    token_file: Option<PathBuf>,
    /// Reach the hub through a tailnet identity of the node's own instead of the host's
    /// network (ADR-0048); `--hub` then names the hub as on that tailnet,
    /// `http://saarthi-hub:7700`.
    #[arg(long, requires = "hub")]
    tailnet: bool,
    /// The auth key for the first join, and again if the tailnet logs the node out; it
    /// may be deleted between.
    #[arg(long, requires = "tailnet")]
    tailnet_authkey_file: Option<PathBuf>,
    /// The node's machine name on the tailnet. Defaults to `saarthi-<name>`.
    #[arg(long, requires = "tailnet")]
    tailnet_hostname: Option<String>,
    /// Where the identity keeps its state. Defaults to `tailnet` in the state directory.
    #[arg(long, requires = "tailnet")]
    tailnet_state: Option<PathBuf>,
    /// The name this node registers under; the hub keys nodes by token, not by name.
    #[arg(long)]
    name: Option<String>,
    /// Unacked events kept while the hub is unreachable.
    #[arg(long, default_value_t = 200_000)]
    buffer_events: usize,
    /// Seconds a permission dialog of an unwrapped session waits for a decision from the hub
    /// before it falls through to the terminal. Keep it below the hooks' `--timeout`.
    #[arg(long, default_value_t = 120)]
    hold_secs: u64,
    /// The same for a session under `saarthi run`, where a keystroke at its terminal releases
    /// the hold at once. Keep it below the hooks' `--timeout`.
    #[arg(long, default_value_t = 540)]
    wrapped_hold_secs: u64,
    /// The unix socket `saarthi run` wrappers connect to.
    #[arg(long)]
    wrapper_socket: Option<PathBuf>,
    /// Where the node keeps its own files, and the lock that keeps a second node off them
    /// (ADR-0080): the changes and sandbox-hook sockets, photos sent to sessions and the
    /// tailnet identity, unless given apart.
    #[arg(long)]
    state_dir: Option<PathBuf>,
    /// The unix socket the node serves its sessions' changes on, the local service `changes`
    /// (ADR-0067). Defaults to `changes.sock` in the state directory.
    #[arg(long)]
    changes_socket: Option<PathBuf>,
    /// A glob over working directories whose sessions are not tracked at all: not listed,
    /// not tailed, their dialogs not held (ADR-0028). Repeatable.
    #[arg(long = "ignore-cwd")]
    ignore_cwd: Vec<String>,
    /// Report the account's rate-limit utilisation as health, from Anthropic's usage
    /// endpoint with the access token read from Claude Code's credential store, read only
    /// (ADR-0030). Off by default.
    #[arg(long)]
    claude_usage: bool,
    /// How often to poll the usage endpoint; after a 429 the daemon waits longer (it refused
    /// once-a-minute polls for half an hour, EXP-20261025-25f6vm).
    #[arg(long, default_value_t = 300)]
    claude_usage_secs: u64,
    /// The usage endpoint; overridden in tests.
    #[arg(long, default_value = usage::ENDPOINT, hide = true)]
    claude_usage_url: String,
    /// The keychain service name to read instead of Claude Code's own; for tests.
    #[arg(long, hide = true)]
    claude_usage_service: Option<String>,
    /// Keep the machine from idle-sleeping while a session is busy (`caffeinate -i` on
    /// macOS, a `systemd-inhibit` lock on Linux where polkit allows it), reported as the
    /// health metric `node.keep_awake` (ADR-0031, ADR-0045). The state at
    /// start; a client can switch it.
    #[arg(long)]
    keep_awake: bool,
    /// A directory clients may spawn headless sessions under, itself or any directory
    /// below it (ADR-0034). Repeatable; without one, spawning is off.
    #[arg(long = "spawn-root")]
    spawn_roots: Vec<PathBuf>,
    /// The claude binary spawned sessions run, resolved by the login shell (`$SHELL -l`)
    /// they start through.
    #[arg(long, default_value = "claude")]
    claude: String,
    /// A TOML file of launch profiles a spawn may name, such as a seatbelt sandbox
    /// (ADR-0038). Without it every spawned session runs on the host.
    #[arg(long)]
    profiles: Option<PathBuf>,
    /// A TOML file of local HTTP services on unix sockets that the owner's clients reach
    /// through the hub, such as yajna's notebook server (ADR-0052). A sandbox is denied
    /// their sockets.
    #[arg(long)]
    services: Option<PathBuf>,
    /// Let clients open a shell on this machine (ADR-0054): the owner's login shell in their
    /// home directory, kept by the shell holder until it exits or is killed (ADR-0078). Off by
    /// default: a shell is this account's everything, to anyone with a client token.
    #[arg(long)]
    terminal: bool,
    /// The shell holder's socket (`saarthi shells serve`), started when none answers.
    /// Defaults to `$XDG_RUNTIME_DIR/saarthi/shells.sock`, else
    /// `~/.local/state/saarthi/shells.sock`.
    #[arg(long)]
    shells_socket: Option<PathBuf>,
    /// The unix socket sandboxed sessions post their hooks to, since loopback TCP is closed
    /// inside. Defaults to `hook.sock` in the state directory; bound only when a profile is
    /// sandboxed. Keep it short: a socket path has at most 104 bytes on macOS.
    #[arg(long)]
    hook_socket: Option<PathBuf>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();
    match Cli::parse().command {
        Command::Node(Node::Hooks { hook_addr, timeout }) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&hooks::settings(hook_addr, timeout))?
            );
            Ok(())
        }
        Command::Node(Node::Serve(args)) => serve(*args).await,
        Command::Shells(Shells::Serve { socket }) => {
            saarthi_node::shells::serve(&socket.unwrap_or_else(saarthi_node::shells::default_socket)).await
        }
        Command::Run(mut args) => {
            extract_account_args(&mut args);
            let opts = saarthi_node::run::Options {
                command: args.command,
                args: args.args,
                socket: args.wrapper_socket.unwrap_or_else(wrapper::default_socket),
                account: match (args.own_login, args.claude_token) {
                    (true, _) => saarthi_node::run::Account::OwnLogin,
                    (false, Some(name)) => saarthi_node::run::Account::Named(name),
                    (false, None) => saarthi_node::run::Account::NodeDefault,
                },
            };
            let code = tokio::task::spawn_blocking(move || saarthi_node::run::run(opts)).await??;
            std::process::exit(code)
        }
        Command::Tui(args) => {
            let path = ctl::config_path().context("HOME is not set")?;
            let (hub, token_file) = ctl::Config::load(&path)?.resolve(args.hub, args.token_file, &path)?;
            let token = std::fs::read_to_string(&token_file)
                .with_context(|| format!("reading {}", token_file.display()))?;
            tui::run(&hub, token.trim()).await
        }
        Command::Ctl(args) => {
            let path = ctl::config_path().context("HOME is not set")?;
            let (hub, token_file) = ctl::Config::load(&path)?.resolve(args.hub, args.token_file, &path)?;
            let token = std::fs::read_to_string(&token_file)
                .with_context(|| format!("reading {}", token_file.display()))?;
            ctl::run(&hub, token.trim(), args.cmd).await
        }
    }
}

async fn serve(args: Serve) -> Result<()> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .context("HOME is not set")?;
    let claude_json = match (&args.claude_json, &args.claude_dir) {
        (Some(path), _) => path.clone(),
        (None, Some(dir)) => dir.join(".claude.json"),
        (None, None) => home.join(".claude.json"),
    };
    let default_claude_dir = args.claude_dir.is_none();
    let claude_dir = args.claude_dir.unwrap_or_else(|| home.join(".claude"));
    let ignore = args
        .ignore_cwd
        .iter()
        .map(|g| glob::Pattern::new(g).with_context(|| format!("--ignore-cwd {g}")))
        .collect::<Result<Vec<_>>>()?;
    let profiles = match &args.profiles {
        // A bad profiles file costs the profiles, not the node (ADR-0044).
        Some(path) => Profiles::load(path, &home).unwrap_or_else(|e| {
            tracing::error!(path = %path.display(), error = %e, "launch profiles left out");
            Profiles::default()
        }),
        None => Profiles::default(),
    };
    let services = match &args.services {
        // A bad services file costs the services, not the node (ADR-0044).
        Some(path) => saarthi_node::services::Services::load(path, &home).unwrap_or_else(|e| {
            tracing::error!(path = %path.display(), error = %format!("{e:#}"), "local services left out");
            Default::default()
        }),
        None => Default::default(),
    };
    let hub = match (args.hub, args.token_file) {
        (Some(hub), Some(token_file)) => {
            let token = std::fs::read_to_string(&token_file)
                .with_context(|| format!("reading {}", token_file.display()))?
                .trim()
                .to_string();
            Some(node::Hub {
                addr: if args.tailnet { HubAddr::via_tailnet(hub) } else { HubAddr::direct(hub) },
                token,
                tailnet: node::Tailnet {
                    authkey: args.tailnet_authkey_file.map(saarthi_tailnet::AuthKey::File),
                    hostname: args.tailnet_hostname,
                    state: args.tailnet_state,
                },
            })
        }
        _ => None,
    };
    let usage = args.claude_usage.then(|| node::Usage {
        store: match &args.claude_usage_service {
            Some(service) => usage::Store::Keychain {
                service: service.clone(),
            },
            None => usage::Store::for_dir(&claude_dir, default_claude_dir),
        },
        url: args.claude_usage_url.clone(),
        every: Duration::from_secs(args.claude_usage_secs),
    });
    let opts = node::Options {
        hub,
        name: args
            .name
            .or_else(sysinfo::System::host_name)
            .unwrap_or_else(|| "node".to_string()),
        state_dir: args.state_dir.unwrap_or_else(|| home.join(".local/state/saarthi")),
        hook_addr: args.hook_addr,
        claude: args.claude,
        shell: spawner::login_shell(),
        spawn_roots: args.spawn_roots,
        profiles,
        services,
        user_claude: claude_dir.clone(),
        claude_files: Vec::new(),
        ignore_cwd: ignore,
        transcript_poll: Duration::from_millis(args.transcript_poll_ms),
        health_every: Duration::from_secs(args.health_secs),
        hold: Duration::from_secs(args.hold_secs),
        wrapped_hold: Duration::from_secs(args.wrapped_hold_secs),
        buffer_events: args.buffer_events,
        keep_awake: args.keep_awake,
        changes_socket: args.changes_socket,
        hook_socket: args.hook_socket,
        machine: Some(node::Machine {
            home: home.clone(),
            claude_dir,
            claude_json,
            registry_every: Duration::from_secs(args.registry_secs),
            wrapper_socket: args.wrapper_socket.unwrap_or_else(wrapper::default_socket),
            terminal: args.terminal,
            shells_socket: args.shells_socket.unwrap_or_else(saarthi_node::shells::default_socket),
            usage,
        }),
    };
    let mut node = node::Node::start(opts).await?;
    tokio::select! {
        _ = tokio::signal::ctrl_c() => tracing::info!("stopping"),
        _ = node.finished() => {}
    }
    node.stop().await;
    Ok(())
}

/// Extracts `--claude-token <name>`, `--claude-token=<name>`, or `--own-login` from the
/// arguments after `--` if `saarthi run` was not given an account option itself (e.g. when
/// invoked via a shell wrapper like `claude "$@"`).
fn extract_account_args(args: &mut RunArgs) {
    if args.claude_token.is_some() || args.own_login {
        return;
    }
    let mut i = 0;
    while i < args.args.len() {
        if args.args[i] == "--own-login" {
            args.own_login = true;
            args.args.remove(i);
            return;
        } else if args.args[i] == "--claude-token" && i + 1 < args.args.len() {
            args.args.remove(i);
            args.claude_token = Some(args.args.remove(i));
            return;
        } else if let Some(token) = args.args[i].strip_prefix("--claude-token=") {
            args.claude_token = Some(token.to_string());
            args.args.remove(i);
            return;
        }
        i += 1;
    }
}

#[cfg(test)]
mod run_arg_tests {
    use super::*;

    #[test]
    fn extracts_account_from_wrapped_arguments() {
        let mut args = RunArgs {
            command: "claude".into(),
            wrapper_socket: None,
            claude_token: None,
            own_login: false,
            args: vec!["--claude-token".into(), "work".into(), "--continue".into()],
        };
        extract_account_args(&mut args);
        assert_eq!(args.claude_token.as_deref(), Some("work"));
        assert_eq!(args.args, vec!["--continue"]);

        let mut args = RunArgs {
            command: "claude".into(),
            wrapper_socket: None,
            claude_token: None,
            own_login: false,
            args: vec!["--claude-token=personal".into(), "-p".into(), "hi".into()],
        };
        extract_account_args(&mut args);
        assert_eq!(args.claude_token.as_deref(), Some("personal"));
        assert_eq!(args.args, vec!["-p", "hi"]);

        let mut args = RunArgs {
            command: "claude".into(),
            wrapper_socket: None,
            claude_token: None,
            own_login: false,
            args: vec!["--own-login".into()],
        };
        extract_account_args(&mut args);
        assert!(args.own_login);
        assert!(args.args.is_empty());
    }
}
