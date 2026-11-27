---
id: AGS-20261127-hme6sw
type: agent_session
schema_version: 1
title: "Phase items 8 and 9: policies from a file and the hub's database, and nodes under the same engine"
model: claude-opus-5-5
harness: claude-code
started: 2026-11-27T16:03+00:00
finished: 2026-11-27T17:11+00:00
created_entities: []
edited_entities: [PHS-20261126-f3e51h]
outcome: done             # done | failed | aborted | pending
created: 2026-11-27
author: agent:claude
---

## Task
The owner asked for items 8 and 9 of [[PHS-20261126-f3e51h]]: policies from a file and the
hub's database, validated and applied without a restart; and nodes as principals of the
same engine.

## What happened
- Item 8: `--policies <file>` (NixOS `policiesFile`) as the bootstrap set in place of the
  built-in one, read again when its content changes (checked every 5 s); the `policies`
  table (migration 0015); `ListPolicies`, `SetPolicy`, `DeletePolicy` under the admin's
  `ManagePolicies`; `ctl policies [--full]`, `ctl set-policy <id> <file|->`,
  `ctl delete-policy <id>`. Each change is validated alone (so the error names the
  policy) and with the rest, then swapped in; a database write that fails undoes it.
- Decided without asking: a stored policy that no longer fits keeps the hub from
  starting (the error says how to remove it), rather than starting without it, since a
  `forbid` dropped would allow what it forbade; ids are 1 to 64 of `[A-Za-z0-9_-]`.
- Item 9: `check_node` with the node's hub id as principal on `ClaudeToken`
  (`GetClaudeToken`), the API relay (`UseClaudeApi`) and `SessionTranscript` (a new
  action, `GetTranscript`, its own sessions only). A node not yet registered is refused
  these with "the node has not registered"; the node links before anything asks.
- Tests: unit tests for both; `tests/policies.rs` (stored policies apply, are refused when
  they do not fit, outlive a restart; the bootstrap file read again; a node refused an
  account a policy forbids, by token and through the relay). Checked by hand against the
  scratch hub with `ctl`. Items 8 and 9 share one commit: their code is interleaved in
  `authz/mod.rs` and the test file.
