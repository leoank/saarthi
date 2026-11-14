---
id: AGS-20261114-d827jm
type: agent_session
schema_version: 1
title: "Run the experiment for the hub holding the Claude tokens: interactive claude over the socket, a hub restart mid-turn, bytes per turn"
model: "claude-opus-5-5"
harness: "claude-code"
started: 2026-11-14T12:03+00:00
finished: 2026-11-14T17:38+00:00
created_entities: [EXP-20261114-d82rcz, RUN-20261114-gd7jeb, RUN-20261114-gd816v, RUN-20261114-gd8fn2]
edited_entities: [PRJ-0001, PHS-20261106-3702q3]
outcome: done             # done | failed | aborted | pending
created: 2026-11-14
author: agent:claude
---

## Task
The owner asked whether sessions under a hub token should reach the Claude API through the
hub, which adds the token, so it never leaves the hub and cutting a node off is revoking
its node token. They chose: all hub tokens (host, sandboxed, terminal), the node's own
login direct, a hub restart may cut running turns. Run the experiment first.

## What happened
- [[EXP-20261114-d82rcz]] with a two-hop rig in `spikes/13-hub-proxy/` (node hop on a unix
  socket, hub hop on TCP adding the token). Positive on all three questions; findings on
  the page.
- Settled without asking: the owner's sandbox setup-token for the test turns (their
  approval for test turns of 2026-11-13); a fresh config directory for the interactive
  runs rather than the owner's own; a baseline with the real token in the environment, as
  ADR-0039 delivers it now, to tell the dummy's effects apart.
- The first restart attempt broke on the rig, not the design: it resolved the token's
  agenix link once and a rebuild replaced the target. Kept as `restart-accidental`: it
  measured the outage that ends a turn (about 145 s).
- The transcript estimate reads only usage counts of this machine's transcripts.
- Phase item 2.6 added, open; the ADR and the build wait for the owner.
- The owner's answers to the findings: auto mode by default for spawned and terminal
  sessions; the hub records usage from the headers it relays; compression between node and
  hub later (ghost001 has 10 TB a month); commit and push the experiment.
