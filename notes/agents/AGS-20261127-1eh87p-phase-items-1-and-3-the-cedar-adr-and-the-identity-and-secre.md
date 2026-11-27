---
id: AGS-20261127-1eh87p
type: agent_session
schema_version: 1
title: "Phase items 1 and 3: the Cedar ADR and the identity and secrets ADR"
model: claude-opus-5-5
harness: claude-code
started: 2026-11-27T01:19+00:00
finished: 2026-11-27T01:24+00:00
created_entities: [EXP-20261127-1gz93p]
edited_entities: [PHS-20261126-f3e51h]
outcome: done             # done | failed | aborted | pending
created: 2026-11-27
author: agent:claude
---

## Task
The owner asked to move on to the next items of [[PHS-20261126-f3e51h]] after item 15: item
1's ADR (authorization with Cedar) and item 3's (identity and secrets).

## What happened
- ADR-0081, authorization with Cedar, written as proposed on the draft conclusion of
  [[EXP-20261126-f3e5a9]]: the spike's schema, a live entity set, lists narrowed by the hub's
  query first, grants as attributes, a bootstrap policy file plus stored policies, and a
  default set that keeps today's behaviour. Item 1 ticked.
- Asked the owner how a person signs in to the admin page: passkeys. ADR-0082, identity
  and secrets, written as proposed. Decided without asking, for the owner to review in the
  ADR: `sc_`/`sa_`/`sn_` token prefixes, SHA-256 (not Argon2) for random tokens,
  XChaCha20-Poly1305 under `--secrets-key-file` for Claude tokens, `--owner` naming the
  bootstrap admin, 24-hour enrolment links and 12-hour admin sessions.
- Passkeys need HTTPS, which the hub does not serve: [[EXP-20261127-1gz93p]] planned (not
  run) to check the tailnet's certificate or one from a file before item 18. Item 3 ticked.
