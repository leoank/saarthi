---
id: AGS-20261126-m5cnjc
type: agent_session
schema_version: 1
title: "Phase item 15: the Node builder of ADR-0080, saarthi node serve rewritten onto it"
model: claude-opus-5-5
harness: claude-code
started: 2026-11-26T18:21+00:00
finished: 2026-11-27T01:09+00:00
created_entities: []
edited_entities: [PHS-20261126-f3e51h]
outcome: done             # done | failed | aborted | pending
created: 2026-11-26
author: agent:claude
---

## Task
Phase item 15 of [[PHS-20261126-f3e51h]]: build the `Node` of ADR-0080 that an app starts and
stops, and rewrite `saarthi node serve` onto it.

## What happened
- `crates/node/src/node.rs`: `Options` (embedded defaults from `Options::new`), `Machine` for
  the daemon's parts (registry scan, wrapper socket, terminal, usage), `Hub` and `Tailnet`,
  `Node::start`, `finished`, `stop`. Every path derives from the state directory; the
  builder takes `node.lock` there and refuses a directory whose longest socket path is over
  the system's limit. `awake` and `usage` moved from the binary into the library.
- An embedded node's sessions run with `CLAUDE_CONFIG_DIR=<state>/claude`, its hooks written
  to that dir's `settings.json` as well as passed by `--settings` (one block, asked once,
  EXP-20261106-qt94y4), its API proxies' sockets in `<state>/api`, photos in
  `<state>/uploads`. A spawn there with no hub token is refused before anything starts.
- `Spawner::end_all` ends the sessions a node started; `Node::stop` calls it, then aborts
  the node's tasks. The daemon now does the same on Ctrl-C.
- `crates/hub/tests/embedded.rs`: two embedded nodes in one process against a test hub,
  with a stand-in claude. The test hub names nodes by their token, not the name registered
  (decided without asking: the test keys on the token names). It got a second node token.
- Not done: the hub's test helpers still assemble a node from its parts (an ADR-0080
  consequence), and a real `claude` in an embedded node's dir is item 17.
