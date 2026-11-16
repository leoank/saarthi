---
id: AGS-20261116-jw3ytk
type: agent_session
schema_version: 1
title: "rogue's node flapping on the app and no battery: the link broke resending a large buffer"
model: "claude-opus-5-5"
harness: "claude-code"
started: 2026-11-16T16:56+00:00
finished: 2026-11-16T17:11+00:00
created_entities: []
edited_entities: []
outcome: done          # done | failed | aborted | pending
created: 2026-11-16
author: agent:claude
---

## Task
The owner saw no battery for rogue in the app, and rogue connecting and disconnecting,
while ghost001 (0.8.0) was stable.

## What happened
- rogue's node (0.10.0) registered, then lost the link about nine seconds later, every
  time, with 26,348 events buffered that never drained; nothing new reached the hub, the
  battery samples included. Its log shows the same since 2026-11-15: every link that
  resent 17,229 events or more died within half a minute, one with 10,970 held for hours.
- Reproduced on this machine over loopback with a node reading this Mac's own transcripts
  while the hub was down (26,000 to 39,000 events): the link dropped about two seconds
  after registering, each attempt storing about 10,900 events.
- Cause: the node resent its whole buffer before reading any of the hub's acks, one per
  event, so they piled up until the link broke. Fixed in the node: the buffer goes out
  from a queue while the acks are read, in order, live messages behind it. The same
  reproduction then drained 39,507 events on one link. Regression test
  `crates/hub/tests/backlog.rs` (30,000 events, no disconnect), which fails without the
  fix; passes on macOS and aarch64-linux (curiosity).
- Version 0.11.1. Only the node changed; the hub is unchanged, so rogue's node at 0.11.1
  fixes this against ghost001's hub as it is. Committed as the fix and then the version.
