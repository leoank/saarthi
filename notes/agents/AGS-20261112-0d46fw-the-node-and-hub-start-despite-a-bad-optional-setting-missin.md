---
id: AGS-20261112-0d46fw
type: agent_session
schema_version: 1
title: "The node and hub start despite a bad optional setting: missing spawn roots, a bad profiles or Claude tokens file"
model: claude-opus-5-5
harness: claude-code
started: 2026-11-12T00:22+00:00
finished: 2026-11-12T00:25+00:00
created_entities: []
edited_entities: []
outcome: done             # done | failed | aborted | pending
created: 2026-11-12
author: agent:claude
---

## Task
The owner found that ghost001's node would crash-loop: neusis set `~/workspace` as its
spawn root, the directory did not exist there, and the node exited at start when a spawn
root was missing. They fixed the deployment in neusis themselves and asked that the hub and
node not crash, fixed here, with no changes in neusis. Before that, in the same
conversation: saarthi 7f24656 and 1b5f919 pushed, and neusis committed (a785fcc, e60400b)
with the input bumped and `usageProbeNode = "ghost001"`, not pushed.

## What happened
- Looked for every start-up failure that comes from an optional setting: a missing spawn
  root and a `--profiles` file that does not parse stop the node, a `--claude-tokens` file
  that does not parse stops the hub. Recorded the rule as ADR-0044: such a setting is left
  out with an error in the log; what the process cannot run without (peer token files, the
  database, its ports, the node's hub token) stays fatal.
- Decided without asking: a missing spawn root is created rather than only skipped, since
  the owner declared it and probes on ghost001 need it; `Spawner::new` no longer returns a
  `Result`.
- Tests run the real binaries: `crates/node/tests/start.rs` (a missing root, a root under a
  regular file, a profiles file that does not parse) and `crates/hub/tests/start.rs` (a
  Claude tokens line without a token). Both failed with the old code ("the node exited",
  "the hub exited") and pass with the new.
