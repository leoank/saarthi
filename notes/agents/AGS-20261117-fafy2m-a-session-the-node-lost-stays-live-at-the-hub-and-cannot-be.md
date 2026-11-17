---
id: AGS-20261117-fafy2m
type: agent_session
schema_version: 1
title: "a session the node lost stays live at the hub and cannot be resumed (curiosity after a reboot)"
model: "claude-opus-5-5"
harness: "claude-code"
started: 2026-11-17T13:50+00:00
finished: 2026-11-17T13:57+00:00
created_entities: []
edited_entities: []
outcome: done          # done | failed | aborted | pending
created: 2026-11-17
author: agent:claude
---

## Task
The owner tried to continue a curiosity session from the app; the node answered that it
had no such session. They asked how a restore works there, then for the fix.

## What happened
- The hub had session 12b7b93a as live (pid 17569) though curiosity, a live disk, had
  rebooted: the old node never sent its end, and a registration did not end the sessions
  it no longer reported. A prompt went to the node as to a running session, and a resume
  was refused as the session still running.
- Fixed (0.12.1): at registration the hub ends, quietly, every session it has as live for
  the node that the node does not report, and tells clients; the node reads its session
  registry once before the link starts, so a node that only restarted reports its running
  sessions. Test `crates/hub/tests/gone.rs`, failing without the fix.
- Given to the owner meanwhile: a one-row update of the hub's database on ghost001 that
  ends that session, so it resumes from the hub's copy (ADR-0059).
