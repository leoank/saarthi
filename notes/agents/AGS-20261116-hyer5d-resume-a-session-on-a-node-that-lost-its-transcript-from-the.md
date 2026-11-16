---
id: AGS-20261116-hyer5d
type: agent_session
schema_version: 1
title: "resume a session on a node that lost its transcript, from the hub's copy; why claude on curiosity asks to sign in"
model: "claude-opus-5-5"
harness: "claude-code"
started: 2026-11-16T16:10+00:00
finished: 2026-11-16T16:27+00:00
created_entities: []
edited_entities: []
outcome: done          # done | failed | aborted | pending
created: 2026-11-16
author: agent:claude
---

## Task
Two asks: resume a session on a node that was wiped and no longer has its transcript,
from what the hub keeps; and find why `claude` on curiosity asks to sign in with a
subscription account when the hub's Claude token should serve it.

## What happened
- Resume needs the transcript on the node. The hub kept only `user`, `assistant` and
  `system` lines, but Claude Code chains a conversation through `attachment` lines too: in
  one of the owner's transcripts the chain back from the last line crossed 264 lines the
  hub did not have. Attachments add about 15% (76 MB against 499 MB over the owner's
  transcripts).
- Built (ADR-0059): the node sends attachment lines; a new `SessionTranscript` call streams
  a node's own session's lines from the hub; on a resume without a local transcript the
  node rebuilds the main conversation (subagents and repeats left out, gaps from before
  attachments were kept chained to the line before) and writes it into the launch profile
  the hub names in `Resume`, then resumes as before. Tested end to end on loopback (a
  session, its transcript deleted, resumed from the hub), on macOS and on aarch64-linux
  (curiosity). Version 0.11.0.
- Decided without asking: store attachments rather than only bridge the gaps; restore into
  the session's launch profile and refuse where the node lacks it; leave subagent
  transcripts out; require the session's directory to exist.
- curiosity: it boots a live disk whose image predates saarthi 0.3.0. Its node is
  `saarthi 0.1.0` (no hub tokens, no terminal-session token, no API relay), its unit has
  no tailnet flags and none of today's auto-update units, and `claude` there is the plain
  package, not under `saarthi run`; the live home has no Claude login, so it asks for one.
  neusis today evaluates curiosity's node at 0.10.0 with `claude` wrapped, so a new
  image (`nix build .#curiosity-live`, written to the stick) fixes it. Not built: the
  owner's call.
- Nothing committed.
