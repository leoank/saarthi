---
id: AGS-20261114-rapbh2
type: agent_session
schema_version: 1
title: "Record the design for yajna on the saarthi app: a service relay, notebooks in the app, a node terminal and voice"
model: "claude-opus-5-5"
harness: "claude-code"
started: 2026-11-14T22:08+00:00
finished: 2026-11-14T22:12+00:00
created_entities: [PHS-20261114-rbv2fs, EXP-20261114-raqjjy, EXP-20261114-raqjf0, EXP-20261114-raqk9b, EXP-20261114-raqkym]
edited_entities: [PRJ-0001]
outcome: done
created: 2026-11-14
author: agent:claude
---

## Task
The owner asked whether a yajna app should be its own, in the yajna repo, or built on the
saarthi app, for new projects, files, reviews, a terminal on a node, bench work and voice;
then whether yajna should run as a daemon on every machine. They took the suggestions
(2026-11-14): record them, no code yet.

## What happened
- Four ADRs: ADR-0052 (the hub relays a client's requests to a node's local services, the
  node dialling back a stream per request), ADR-0053 (notebooks in the saarthi app, a web
  view first, native for new projects, reviews and the bench), ADR-0054 (a terminal through
  the node's PTY, off unless `--terminal`), ADR-0055 (push to talk and spoken replies).
- `docs/notebooks.md` and a section in `docs/architecture.md`; phase
  [[PHS-20261114-rbv2fs]] with eleven items; planned experiments [[EXP-20261114-raqjjy]]
  (the relay and a WebView), [[EXP-20261114-raqjf0]] (libghostty on Android),
  [[EXP-20261114-raqk9b]] (the recognizer), [[EXP-20261114-raqkym]] (bench timers and
  offline ticks).
- yajna's side in its repo: ADR-0036 and phase 3 items ([[yajna/AGS-20261114-rapbdr]]).
- Decided without asking: the terminal is opt-in per node; services are unix sockets
  only, no loopback ports; the node dials back a stream per request rather than
  multiplexing on `Link`; iOS is out of scope, as it was not answered.
