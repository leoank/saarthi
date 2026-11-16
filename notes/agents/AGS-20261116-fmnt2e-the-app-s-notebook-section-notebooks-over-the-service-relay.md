---
id: AGS-20261116-fmnt2e
type: agent_session
schema_version: 1
title: "the app's notebook section: notebooks over the service relay in a web view with the event-stream bridge"
model: "claude-opus-5-5"
harness: "claude-code"
started: 2026-11-16T14:14+00:00
finished: 2026-11-16T14:31+00:00
created_entities: []
edited_entities: [PHS-20261114-rbv2fs]
outcome: done          # done | failed | aborted | pending
created: 2026-11-16
author: agent:claude
---

## Task
The owner asked to continue with the app's notebook section: item 3 of
[[PHS-20261114-rbv2fs]], after the service relay (0.8.0) and `yajna serve --socket` were
deployed and checked from rogue through ghost001 earlier the same day.

## What happened
- The core gained `Hub::service_request`, which returns a `ServiceResponse` with its status
  and headers and a body read piece by piece (`read`, `cancel`); `Hub::notebooks`, which
  reads yajna's `/api/notebooks`; and `NodeInfo.services`. Tests: a unit test of the list's
  parsing, and `crates/core/tests/services.rs` against a real hub and node with a stand-in
  yajna on a unix socket (a list, a POST body, a held event stream and its cancel).
- The app gained `Notebooks.kt`: the list per node, and a notebook's pages in a web view
  whose requests are answered by `shouldInterceptRequest` through the core, with the
  ADR-0056 bridge put at the top of each HTML page. Entry: a book icon on the sessions
  screen, shown when a node offers `yajna`.
- Checked on the rc2s2 emulator against a hub and node on this machine, the node starting
  `yajna serve --socket` for a scratch notebook: the list, the site, the files viewer, a
  file followed as it grew, review POSTs through the bridge answered as yajna answers them
  directly, and the live stream recovering after `yajna serve` was killed.
- Found on the way, none of them in the spike: the web view inside Compose had
  `wrap_content` height, so the files viewer (laid out at 100% of the viewport) was blank;
  the web view applies a request's `Range` to an intercepted answer again and checks it
  against `available()`, so a followed file lost the start of each new line, then failed;
  interception never sees a POST's body. Fixed, and the last two recorded as ADR-0058.
- Decided without asking (in ADR-0058, for the owner to look at): the bridge carries a
  same-origin `fetch` with a string body; range answers are placed at their offset behind a
  virtual lead rather than fetching whole files; the bridge's `EventSource` reconnects
  after three seconds; each node's pages get their own origin.
- Versions bumped to 0.9.0 (app versionCode 10); guide page `using/notebooks.md`.
- Not done: the phone, through the deployed hub; it needs hub, node and app at 0.9.0
  (the hub and node are unchanged in behaviour, the app reads `services` from the
  existing `ListNodes`). Nothing committed.
