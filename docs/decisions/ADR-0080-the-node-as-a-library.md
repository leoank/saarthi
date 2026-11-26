# ADR-0080: The node as a library

Status: proposed
Date: 2026-11-26
Amends: ADR-0048

## Context
Phase PHS-20261126-f3e51h lets the owner's apps launch agent sessions through the hub. An app
may run where the owner's daemon already runs, on a machine with none, or several to a
process. The owner asked for the node to be usable as a library, Rust first, and for no
assumption about where an app runs.

`saarthi_node` is already a library in name. The hub's tests assemble a node from its parts.
But `saarthi node serve` wires those parts in 360 lines of `main.rs`, and the parts assume
they are the machine's only node. EXP-20261126-f3e5v3 checked what collides when there are
two:

- **The way to the hub was process-wide.** Two statics in `link.rs` made one node's tailnet
  every node's. This is now fixed: c3e3fb0 put a `HubAddr` in each node's link config.
- **The default paths are the user's.** The wrapper socket, the changes socket, the hook
  port and the tailnet state all default to the same per-user places. A second node on the
  defaults took the first's wrapper socket without a word. 5282158 makes it refuse instead.
  With the paths apart, nothing collided.
- **A Claude dir is watched by every node pointed at it.** Two nodes on the user's
  `~/.claude` both listed all its sessions and uploaded the same transcripts, about 114,700
  events each. A session one node spawned was also reported by the other. Hooks in
  `~/.claude/settings.json` name one node, the machine's daemon. A session started in that
  dir reads them whoever started it.
- **Nothing else is process-wide.** The library writes no environment. Only the binary
  installs signal handlers and the log subscriber.

## Decision
- **Two kinds of node, one builder.** `saarthi_node::Node` is built from a hub (`HubAddr`
  and node token), a name and a state directory, then started and stopped by its owner:
  - **The machine's node** is today's daemon: one per user per machine. It alone watches
    the user's Claude dir. It scans the session registry, takes hooks from
    `settings.json`, serves `saarthi run`'s socket and the terminal, and reads the usage
    from the user's credentials.
  - **An embedded node** sees only the sessions it starts. It does none of the above.

  `saarthi node serve` becomes a caller of the builder with those machine parts switched
  on. Its flags and `$HOME` defaults stay in the binary, so there is one code path and the
  daemon tests it.
- **Everything a node binds or writes derives from its state directory**, unless given:
  - the sessions' Claude config dir, `<state>/claude`;
  - the changes socket, the API proxies' sockets, photo uploads and the tailnet state;
  - the hook port, which is loopback port 0 unless one is named.

  The builder checks the longest socket path against the 104-byte limit on macOS and
  refuses a state directory that is too long. Two nodes must not share a state directory:
  the builder takes a lock file in it and refuses to start if another node holds the lock.
- **An embedded node's sessions run in its own Claude config dir, under the hub's
  accounts.** This is what a sandbox profile does (ADR-0038):
  - The node writes its hooks into `<state>/claude/settings.json`, so its sessions'
    SessionStart reaches it too.
  - It links only the user files it is told to (`claude_files`, as a profile does).
  - It runs every session under a named hub Claude token, through its API proxy
    (ADR-0051). The app's grant names which tokens it may use (EXP-20261126-f3e5a9). A
    config dir of its own has no login, and the machine's login lives in the user's dir.

  Launch profiles, a sandbox among them, work as on the daemon. A profile's own state
  directory keeps it apart in the same way.
- **The library holds nothing process-wide.** No statics, no signal handlers, no log
  subscriber, no environment writes. It runs on the app's Tokio runtime, which must be
  multi-threaded because the health sampler blocks in place. Its log lines carry the
  node's name in a span.
- **Stopping a node ends the sessions it spawned.** Their input closes and those still
  running after the grace period are killed, as on the daemon today. The hub marks them
  ended. The state directory keeps their transcripts, so a node started again on it can
  resume them.
- **On the hub an embedded node is a node like any other**, with a node token of its own.
  How the hub records that it belongs to an app, and what it may ask for, is phase items 3,
  9 and 16. The library assumes nothing about where the app runs: beside the owner's daemon
  (as checked in EXP-20261126-f3e5v3), on a machine with none, or over the app's own
  tailnet identity (`HubAddr::via_tailnet` with its state in the node's directory).
- **Apps depend on the crate by git revision.** It is versioned with the hub, node and app
  (0.23.0 now) and is not published until phase item 14 publishes the client crate beside
  it.

## Alternatives considered
- **An embedded node in the user's Claude dir under the machine's login.** No hub token is
  needed and the user's CLAUDE.md, plugins and MCP servers come along. But the run showed
  every session listed twice, every transcript uploaded twice, and the app's sessions
  reported by the owner's daemon as bare ones. Only one node can watch a Claude dir.
- **One node per process, the statics kept.** It is simpler, but an app that runs work for
  two hubs, or for two accounts on separate tailnets, would need two processes, and the
  rule would live only in documentation. The fix was small.
- **The app runs `saarthi node serve` as a child process.** That works today with every
  path given, and suits apps in other languages. But it still needs the same embedded mode
  and a second process to supervise, and the owner asked for a library. A `serve` flag for
  the embedded mode can come later if an app in another language needs one.
- **Apps only call the hub and spawn on the owner's machines, with no node of their
  own.** That stays possible and needs nothing new. It does not cover an app that must run
  its sessions where its data is, on a machine without the owner's daemon.

## Consequences
- An app's sessions do not see the user's CLAUDE.md, plugins or MCP servers unless they are
  linked in or passed with the spawn. That is phase item 12's spawn options.
- An app spends the hub's Claude accounts, never the machine's login. A machine with only
  a subscription login cannot host an embedded node until the hub holds a token for that
  account.
- What EXP-20261126-f3e5v3 did not run: a real `claude` under an embedded node's own config
  dir, with a hub token through the proxy and hooks from that dir's `settings.json`. Sandbox
  profiles run the same way today. Phase item 17's end-to-end test checks it.
- `main.rs`'s `serve` is rewritten onto the builder. The hub's test helpers move to the
  builder too, and stop assembling parts by hand.
- A state directory has to be short. One under `~/Library/Application Support/<app>/` fits
  the macOS socket limit only if the app's name is brief; the builder's refusal says so.
