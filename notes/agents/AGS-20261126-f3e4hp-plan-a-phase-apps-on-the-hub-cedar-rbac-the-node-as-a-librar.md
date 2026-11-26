---
id: AGS-20261126-f3e4hp
type: agent_session
schema_version: 1
title: "Plan a phase: apps on the hub, Cedar RBAC, the node as a library, a hub admin UI"
model: "claude-opus-5-5"
harness: "claude-code"
started: 2026-11-26T13:45+00:00
finished: 2026-11-26T14:03+00:00
created_entities: [PHS-20261126-f3e51h, EXP-20261126-f3e5a9, EXP-20261126-f3e5v3]
edited_entities: [PRJ-0001, PHS-20261106-3701r1, PHS-20261106-3701nh, PHS-20261106-3702ke, PHS-20261106-3702q3, PHS-20261114-rbv2fs, PHS-20261117-f22kxe, PHS-20261118-kbka9b]
outcome: done
created: 2026-11-26
author: agent:claude
---

## Task
Plan a phase for apps the owner writes that use the hub and may embed nodes: a
`launched_by` on sessions, RBAC with Cedar, no assumption about where an app runs, the
node as a library, and a hub admin UI to watch clients and add and revoke secrets, with
the agenix files kept to bootstrap the hub.

## What happened
- Read the control API: every call is already tied to a client's name from
  `--client-tokens`, so `launched_by` is stamped by the hub, never sent by the app; any
  client token can do everything today.
- Created phase [[PHS-20261126-f3e51h]] (22 items in five sections) and two planned
  experiments it rests on: [[EXP-20261126-f3e5a9]] (Cedar in the hub) and
  [[EXP-20261126-f3e5v3]] (two nodes on one machine and in one process).
- Decided without asking: three ADRs to come (Cedar, the node as a library, identity and
  secrets), written after the experiments; hashes for client and node tokens the hub
  keeps, encryption under an agenix key for Claude tokens; where the admin UI lives is
  left to the identity ADR, an open choice for the owner.
- The owner's answers (2026-11-26): the hub serves the admin page; the Rust library first;
  no gRPC-web; people as principals so others can be onboarded. Written into the phase and
  [[EXP-20261126-f3e5a9]].
- The owner asked to tick what is done and draft missing conclusions. Ticked from evidence:
  phase 1's done criteria, phase 2 items 1 and 3.5, phase 3 items 1, 1.5 and 2.3 (neusis
  884b9c3 and 3b7ffbf); phases 1, 1.5, f22kxe and kbka9b marked done at the commit that
  last touched each page. Voice items in [[PHS-20261114-rbv2fs]] annotated, not ticked
  (spoken replies are not built). Three subagents drafted conclusions for 25
  experiments; [[EXP-20261110-g5mh7g]] (second machine) and [[EXP-20261121-3k3cxv]] (the
  phone's day) stay open. Removed duplicate "not yet described" run lines and two duplicate
  experiment lines on the project page.
