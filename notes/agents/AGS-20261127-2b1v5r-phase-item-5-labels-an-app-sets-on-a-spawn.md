---
id: AGS-20261127-2b1v5r
type: agent_session
schema_version: 1
title: "Phase item 5: labels an app sets on a spawn"
model: claude-opus-5-5
harness: claude-code
started: 2026-11-27T02:08+00:00
finished: 2026-11-27T02:11+00:00
created_entities: []
edited_entities: [PHS-20261126-f3e51h]
outcome: done             # done | failed | aborted | pending
created: 2026-11-27
author: agent:claude
---

## Task
The owner asked for the next item of [[PHS-20261126-f3e51h]]: item 5, labels an app sets on a
spawn, returned with the session and searchable.

## What happened
- `SpawnRequest.labels` (9), `Session.labels` (20), `ListSessionsRequest.labels` (3). Stored as
  a JSON object on the session, recorded with the launch before the node is asked (as
  `launched_by`), so the first insert takes them; migration 0014.
- Decided without asking: at most 16 labels, keys 1 to 64 of `[A-Za-z0-9_.-/]`, values up
  to 256 characters with no control characters (`INVALID_ARGUMENT` otherwise); a list
  matches every label given (`json_each`); a resume keeps them and a fork takes them.
  "Searchable" was read as the list filter; full-text search, watch and history filters
  are item 6.
- `ctl spawn --label k=v` and `ctl sessions --label k=v`, in the guide's ctl page.
- Tests: a db test of storing and matching, labels in the embedded-node end-to-end test,
  and a fork carrying them in the spawn test.
