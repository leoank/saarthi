---
id: AGS-20261126-j986es
type: agent_session
schema_version: 1
title: fix the two-node collisions EXP-20261126-f3e5v3 found and write the ADR on the node as a library
model: claude-opus-5-5
harness: claude-code
started: 2026-11-26T16:38+00:00
finished: 2026-11-26T16:46+00:00
created_entities: []
edited_entities: [PHS-20261126-f3e51h]
outcome: done             # done | failed | aborted | pending
created: 2026-11-26
author: agent:claude
---

## Task
The owner asked to commit the two-nodes experiment, make the fixes it suggested, and write
the ADR on the node as a library (phase PHS-20261126-f3e51h item 2).

## What happened
- Committed the spike (8637e4a), recorded its three runs at it, committed the pages
  (e05a811).
- Fix 1, c3e3fb0: `link::HubAddr`, the hub's URL and, for a tailnet, a slot for the
  identity, carried by the link config, the wrappers and the API relay, replacing the
  statics `VIA_TAILNET` and `TAILNET`. The hub's test helper gained `start_node_at` for the
  tailnet test. A unit test shows two nodes' ways to the hub are independent.
- Fix 2, 5282158: `Wrappers::bind` refuses a socket another node answers on, as
  `changes::serve` already did; a stale one is still replaced. Unit test added.
- Workspace tests pass (`cargo test --workspace` in the dev shell); clippy's warnings are
  older and in other files.
- Took as mine to settle: which fixes were "the ones suggested" (these two; the state
  directory and the embedded mode are the ADR's, to build in item 15), and the ADR's
  status, `proposed`, for the owner to accept. The spike `spikes/17-two-nodes` calls the
  removed `dial_via_tailnet` and no longer builds at HEAD; it is reproducible at its run
  commit, 8637e4a, and was left as it was.
- ADR-0080 written, with a row in the index and a paragraph in the architecture.
