---
id: AGS-20261126-fjh5zd
type: agent_session
schema_version: 1
title: "Run EXP-20261126-f3e5a9: Cedar in the hub"
model: "claude-opus-5-5"
harness: "claude-code"
started: 2026-11-26T14:10+00:00
finished: 2026-11-26T14:26+00:00
created_entities: [RUN-20261126-fjh58z, RUN-20261126-fjh6wk, RUN-20261126-fjh6d5]
edited_entities: [EXP-20261126-f3e5a9, PRJ-0001]
outcome: done
created: 2026-11-26
author: agent:claude
---

## Task
Run [[EXP-20261126-f3e5a9]]: can the hub authorize every control call with Cedar.

## What happened
- A spike under `spikes/16-cedar` with cedar-policy 4.15.0: a schema of people, device
  clients, apps with grants, nodes (shared, embedded), profiles, accounts, sessions with
  labels as tags, shells and the hub, an action per control call in five groups; six
  policy files; a world and a table of 59 cases.
- All three runs passed: the table on the first try, the benchmark after adding a
  measure of upserting one session into the live set (the per-call slice from JSON was
  slow, so how the hub keeps entities mattered), the builds on three targets.
- Decided without asking: grants as attributes on the app rather than Cedar templates;
  the admin role inherits nothing for apps (an app is not in its person); a `keep`
  label as the example of a forbid on tags.
- Drafted the conclusion (positive, review pending). The runs need a commit holding their
  scripts before `yajna run record`.
