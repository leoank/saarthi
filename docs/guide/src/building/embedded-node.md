# Embedding a node

An app can run a node of its own, in its own process, with the `saarthi-node` library
(ADR-0080). The node links to the hub with a node token like any machine, shows up in the
app's node list, and runs the sessions clients start on it. It needs no `saarthi node
serve` on the machine, and runs beside one if there is.

```toml
[dependencies]
saarthi-node = { git = "https://github.com/leoank/saarthi", rev = "<the hub's commit>" }
tokio = { version = "1", features = ["macros", "rt-multi-thread", "signal"] }
```

```rust
use saarthi_node::node::{Hub, Node, Options};

#[tokio::main] // multi-threaded: the node's health sampler blocks in place
async fn main() -> anyhow::Result<()> {
    let token = std::fs::read_to_string("/run/secrets/reports-node-token")?.trim().to_string();
    let mut opts = Options::new("reports", "/var/lib/reports/node".into());
    opts.hub = Some(Hub::direct("http://hub:7700", token));
    opts.spawn_roots = vec!["/srv/reports".into()];
    opts.claude_files = vec!["CLAUDE.md".into()]; // linked from ~/.claude, if you want them
    let node = Node::start(opts).await?;
    tokio::signal::ctrl_c().await?;
    node.stop().await;
    Ok(())
}
```

Add a line for it to the hub's node-token file (`reports-node 51ab…`): the hub knows it by
that name.

## What differs from the machine's daemon

- **It sees only the sessions it starts.** It does not watch `~/.claude`, take `saarthi
  run` sessions, offer a terminal or poll the account's usage: those stay the daemon's.
- **Its sessions run in a Claude config directory of its own**, `<state>/claude`, with its
  hooks in that directory's `settings.json`. They do not see your `CLAUDE.md`, plugins or
  MCP servers unless you list them in `claude_files`, and your daemon does not see them.
- **They run only under the hub's Claude accounts.** That directory has no login, so a
  spawn on the node names one of the hub's Claude tokens, or the node needs a default
  account set at the hub (`saarthi ctl default-token`); one without is refused.
- **Everything it writes is under its state directory**: sockets, the Claude config,
  photos sent to sessions, its lock. Two nodes may not share one, and the path must be
  short (a socket's path has at most 103 bytes on macOS). The hook endpoint takes a free
  port unless `hook_addr` names one.
- **Stopping it ends the sessions it started.** `Node::stop` closes their input and kills
  any still running five seconds later; their transcripts stay in the state directory, so
  a node started again on it can resume them.
- **Nothing in it is the process's.** Several nodes, for several hubs or tailnets, can run
  in one process; it installs no signal handler or log subscriber of its own (its log
  lines are `tracing` events, under a span with the node's name if you add one).

A node can reach the hub through a tailnet identity of its own instead of the host's
network: `HubAddr::via_tailnet` in `Hub::addr`, with `Hub::tailnet` giving the auth key; its
state goes in `<state>/tailnet`.
