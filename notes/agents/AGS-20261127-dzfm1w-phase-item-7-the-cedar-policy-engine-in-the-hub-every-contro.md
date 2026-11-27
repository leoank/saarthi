---
id: AGS-20261127-dzfm1w
type: agent_session
schema_version: 1
title: "Phase item 7: the Cedar policy engine in the hub, every control call checked"
model: claude-opus-5-5
harness: claude-code
started: 2026-11-27T12:43+00:00
finished: 2026-11-27T12:48+00:00
created_entities: []
edited_entities: [PHS-20261126-f3e51h]
outcome: done             # done | failed | aborted | pending
created: 2026-11-27
author: agent:claude
---

## Task
The owner asked to bring the docs up to date for what this phase built, to keep doing so
for every feature, and to build the next item of [[PHS-20261126-f3e51h]]: item 7, the
Cedar policy engine in the hub (ADR-0081).

## What happened
- Docs pass first: the guide's node page (state directory, lock, stop ends spawned
  sessions, the node named by its token not `--name`), troubleshooting (a second node
  refused), the home screen as it is now, two new pages for app authors (apps; embedding a
  node; both examples compiled in a scratch crate), the architecture and the README. It
  found a regression from item 15: the daemon's state directory was fixed, so a second
  daemon with every path apart was refused; fixed with `--state-dir` (b656bd8), checked
  with two daemons on one machine.
- `crates/hub/src/authz/`: the spike's schema (plus `RegisterPush` and `Notifications`,
  and an optional account for `SetDefaultClaudeToken`) and its policies as the default set,
  less the example `keep` label forbid, which would change behaviour. All 29 control calls
  checked; lists, search, history and the watch filter row by row.
- Surprises: (1) a policy on an action group (`read`) matched nothing until the entity set
  started from the schema's action entities: `Entities::empty()` lacks them, so every
  member and monitor policy was silently dead; a unit test now covers a member and a
  monitor. (2) cedar-policy turns on serde_json's `preserve_order` for the whole
  workspace, so the queue test's stand-in claude, which matched `slow` before
  `"type":"user"`, broke; the stand-in no longer depends on key order.
- Decided without asking: refusals say `not allowed: <Action> on this <kind>`
  (`PERMISSION_DENIED`) and are logged; lists filter per row for now (narrowing by query
  needs principals other than the owner); every session's owner is the owner until
  ADR-0082's principals land.
