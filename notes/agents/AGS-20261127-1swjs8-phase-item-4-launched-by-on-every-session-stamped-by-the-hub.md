---
id: AGS-20261127-1swjs8
type: agent_session
schema_version: 1
title: "Phase item 4: launched_by on every session, stamped by the hub"
model: claude-opus-5-5
harness: claude-code
started: 2026-11-27T01:38+00:00
finished: 2026-11-27T01:43+00:00
created_entities: []
edited_entities: [PHS-20261126-f3e51h]
outcome: done             # done | failed | aborted | pending
created: 2026-11-27
author: agent:claude
---

## Task
The owner accepted ADR-0080, ADR-0081 and ADR-0082 and asked for the next item of
[[PHS-20261126-f3e51h]]: item 4, `launched_by` on every session.

## What happened
- Accepted ADR-0080, 0081 and 0082 at the owner's word; ADR-0021 marked superseded in part
  by ADR-0081 in its status line and the index, as ADR-0012 was.
- `Session.launched_by` (field 19): `client:<name>` for a session a client spawned, or a fork
  a client's resume made; `node:<name>` for one its node started. Names are the hub's, from
  the token files, until ADR-0082's principals land; apps will be `app:<name>`.
- The node's report of a new session can reach the hub before the node's reply to the
  spawn, so the hub records the launcher in a `launches` table before asking the node, and
  a session's first insert reads it (migration 0013). A later report never changes it; a
  fork reported before the reply is corrected from `node:` to its resumer.
- Sessions from before the migration record their node, as ADR-0082 says.
- Not done here: the core crate and the app do not carry it yet (item 6), nor `ctl`.
