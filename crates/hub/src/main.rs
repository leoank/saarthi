//! `saarthi-hub`: records what every node observes.

use std::io::IsTerminal;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::Parser;
use saarthi_hub::claude_tokens::ClaudeTokens;
use saarthi_hub::db::Db;
use saarthi_hub::live::Live;
use saarthi_hub::node_service::Hub;
use saarthi_hub::notify::Notifier;
use saarthi_hub::tokens::Tokens;

#[derive(Parser)]
#[command(
    name = "saarthi-hub",
    version,
    about = "saarthi hub: records every node's sessions, events and health, and relays decisions"
)]
struct Cli {
    /// Where nodes connect.
    #[arg(long, default_value = "127.0.0.1:7700")]
    listen: SocketAddr,
    /// The SQLite database; created on first start.
    #[arg(long)]
    db: PathBuf,
    /// A file of `<node name> <token>` lines.
    #[arg(long)]
    node_tokens: PathBuf,
    /// A file of `<client name> <token>` lines, for the phone and `saarthi ctl`.
    #[arg(long)]
    client_tokens: PathBuf,
    /// A file of `<name> <token>` lines: `claude setup-token` tokens that spawned sessions
    /// may run under (ADR-0039). Read at start; clients see the names, never the tokens.
    #[arg(long)]
    claude_tokens: Option<PathBuf>,
    /// The node that runs usage probes: one throwaway Haiku turn under each Claude token
    /// about every `--usage-probe-minutes`, so every account's usage stays fresh (ADR-0043).
    /// It must allow headless sessions (`--spawn-root`). Without it, an account's usage
    /// comes only from the sessions that run under its token.
    #[arg(long)]
    usage_probe_node: Option<String>,
    /// Minutes between an account's usage probes, each wait drawn within 15% of it.
    #[arg(long, default_value_t = 30)]
    usage_probe_minutes: u64,
    /// An ntfy topic URL, `http(s)://host/topic`, for notifications when a session waits on
    /// a person or finishes a turn. Without it nothing is pushed.
    #[arg(long)]
    ntfy: Option<String>,
    /// A notification kind not to send: `permission`, `question`, `turn`, `ended`, `usage`
    /// or `token` (a Claude token about to expire). Repeatable (ADR-0029, ADR-0032).
    #[arg(long = "no-notify")]
    no_notify: Vec<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .init();
    let cli = Cli::parse();
    let db = Db::open(&cli.db).await?;
    let claude_tokens = match &cli.claude_tokens {
        Some(path) => ClaudeTokens::load(path)?,
        None => ClaudeTokens::default(),
    };
    let fingerprints: Vec<(String, String)> = claude_tokens
        .fingerprints()
        .map(|(n, f)| (n.to_string(), f))
        .collect();
    db.sync_claude_tokens(&fingerprints, saarthi_hub::node_service::now_ms())
        .await?;
    tracing::info!(names = ?claude_tokens, "Claude tokens");
    let hub = Hub {
        notifier: Arc::new(Notifier::new(cli.ntfy.clone(), db.clone(), cli.no_notify.clone())),
        db,
        node_tokens: Arc::new(Tokens::load(&cli.node_tokens)?),
        client_tokens: Arc::new(Tokens::load(&cli.client_tokens)?),
        claude_tokens: Arc::new(claude_tokens),
        live: Live::new(),
    };
    let listener = tokio::net::TcpListener::bind(cli.listen)
        .await
        .with_context(|| format!("binding {}", cli.listen))?;
    tracing::info!(listen = %cli.listen, db = %cli.db.display(), "hub serving");
    if let Some(node) = cli.usage_probe_node.clone() {
        tracing::info!(%node, minutes = cli.usage_probe_minutes, "usage probes");
        let every = std::time::Duration::from_secs(cli.usage_probe_minutes.max(1) * 60);
        // The first round after a start comes within five minutes, spread out.
        let first = every.min(std::time::Duration::from_secs(5 * 60));
        tokio::spawn(saarthi_hub::run_usage_probes(
            hub.clone(),
            node,
            every,
            first,
        ));
    }
    let reminders = hub.clone();
    tokio::spawn(async move {
        let mut hourly = tokio::time::interval(std::time::Duration::from_secs(3600));
        loop {
            hourly.tick().await;
            if let Err(e) = saarthi_hub::remind_expiring(&reminders).await {
                tracing::warn!(error = %format!("{e:#}"), "checking Claude token expiry failed");
            }
        }
    });
    tokio::select! {
        r = saarthi_hub::serve(listener, hub) => r,
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("stopping");
            Ok(())
        }
    }
}
