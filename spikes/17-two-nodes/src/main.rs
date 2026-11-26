//! EXP-20261126-f3e5v3 H4: two saarthi nodes in one process, assembled from the
//! `saarthi_node` library as the hub's end-to-end tests assemble one, each with its own
//! Claude dir, sockets and hook port, and no registry scan (the rule run 2 suggests for an
//! embedded node).
//!
//! `two-nodes <hub> <dir> <claude> <name>=<token-file>... [--tailnet <name>]`
//!
//! `--tailnet <name>` does for that node what `saarthi node serve --tailnet` does at start:
//! `link::dial_via_tailnet()`, with no tailnet ever joined (none is needed to see which
//! nodes it applies to).

use std::{path::PathBuf, sync::Arc, time::Duration};

use tracing::Instrument;

use saarthi_node::{
    hooks, link, outbox::Outbox, pending::Pending, queue::Queue, sessions::Sessions,
    spawner::Spawner, wrapper::Wrappers,
};

async fn node(hub: &str, dir: &PathBuf, claude: &str, name: &str, token: &str) -> anyhow::Result<()> {
    let own = dir.join(name);
    let claude_dir = own.join("claude");
    std::fs::create_dir_all(&claude_dir)?;
    let (outbox, rx) = Outbox::new(1024);
    let sessions = Arc::new(Sessions::new(claude_dir.clone(), outbox.clone(), Duration::from_millis(200), Vec::new()));
    let pending = Pending::new(outbox.clone());
    let wrappers = Wrappers::new(sessions.clone(), pending.clone(), outbox.clone());
    tokio::spawn(wrappers.clone().serve(Wrappers::bind(&own.join("wrap.sock"))?));
    // Its own hook port, given to the sessions it spawns.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let addr = listener.local_addr()?;
    let spawner = Spawner::new(
        claude.into(),
        "/bin/sh".into(),
        &[dir.join("work")],
        Some(hooks::settings(addr, 600)),
        Default::default(),
        sessions.clone(),
    );
    let hold = hooks::Hold {
        pending: pending.clone(),
        duration: Duration::from_secs(120),
        wrapped_duration: Duration::from_secs(540),
        upload_roots: Vec::new(),
    };
    let app = hooks::router(outbox.clone(), sessions.clone(), Some(hold));
    tokio::spawn(async move { axum::serve(listener, app).await });
    let cfg = link::Config {
        hub: hub.into(),
        token: token.into(),
        name: name.into(),
        buffer_cap: 10_000,
        terminal: None,
        shells: own.join("shells.sock"),
    };
    let parts = link::Parts {
        sessions: sessions.clone(),
        pending,
        wrappers,
        spawner,
        keep_awake: Default::default(),
        services: Default::default(),
        queue: Queue::new(sessions),
    };
    link::run_queue(&parts);
    // Tagged with the node's name: the library's own log lines do not say which node.
    tokio::spawn(link::run(cfg, rx, parts).instrument(tracing::info_span!("node", name)));
    eprintln!("node {name}: hooks on {addr}, Claude dir {}", claude_dir.display());
    Ok(())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_writer(std::io::stderr).with_ansi(false).init();
    let mut args: Vec<String> = std::env::args().skip(1).collect();
    let tailnet = args.iter().position(|a| a == "--tailnet").map(|i| {
        args.remove(i);
        args.remove(i)
    });
    let (hub, dir, claude) = (args[0].clone(), PathBuf::from(&args[1]), args[2].clone());
    for spec in &args[3..] {
        let (name, file) = spec.split_once('=').expect("name=token-file");
        let token = std::fs::read_to_string(file)?.trim().to_string();
        if tailnet.as_deref() == Some(name) {
            link::dial_via_tailnet();
            eprintln!("node {name}: dials the hub through its tailnet identity");
        }
        node(&hub, &dir, &claude, name, &token).await?;
    }
    tokio::signal::ctrl_c().await?;
    Ok(())
}
