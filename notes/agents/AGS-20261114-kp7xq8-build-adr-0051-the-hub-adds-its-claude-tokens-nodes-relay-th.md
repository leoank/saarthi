---
id: AGS-20261114-kp7xq8
type: agent_session
schema_version: 1
title: "Build ADR-0051: the hub adds its Claude tokens, nodes relay through ClaudeApi, auto mode by default, usage from the relayed headers"
model: "claude-opus-5-5"
harness: "claude-code"
started: 2026-11-14T17:55+00:00
finished: 2026-11-14T17:55+00:00
created_entities: []
edited_entities: [PHS-20261106-3702q3]
outcome: done             # done | failed | aborted | pending
created: 2026-11-14
author: agent:claude
---

## Task
Build the hub holding every hub token, as the owner chose on [[EXP-20261114-d82rcz]]'s
findings ([[AGS-20261114-d827jm]]): sessions under a hub token reach the API through the
hub, the node's own login direct, auto mode by default, usage recorded at the hub,
compression later.

## What happened
- ADR-0051, superseding the delivery of ADR-0039 and ADR-0046 and the token holder of
  ADR-0050 for the hub's tokens; architecture and guide updated; 0.7.0 across hub, node and
  app.
- `ClaudeApi`, a bidirectional stream on the node service: the transport and the node's
  authentication come with the existing connection, the tailnet included.
- Settled without asking: a gRPC stream rather than an HTTP endpoint on the hub; host and
  terminal sessions' sockets in a per-user directory beside the wrappers' one, denied to
  every sandbox; the wrapper's socket served while its hello's connection lasts, so it
  comes back after a daemon restart; `--permission-mode auto` added before the program's
  arguments only for `claude` with no mode of its own (a leading flag works with
  subcommands, checked on 2.1.322); usage recorded at most once a minute per token unless
  it changes; a refusal by the hub is a 403 to claude, anything else a 502 it retries.
- Checks: `cargo test --workspace` passes (the token tests rewritten end to end through a
  fake API: dummy, auto, the real token at the API, /v1 only, usage from the headers, the
  wrapper's socket served and removed); clippy shows only two older warnings in files not
  touched. Live on rogue against a scratch hub reading the sandbox token from a pipe: a
  host spawn, a sandboxed spawn whose profile has no token file and `saarthi run` each
  answered `pong` through the hub, the host session's environment held the dummy, the
  socket and auto mode, the hub recorded the account's usage, no file of the node held
  the token, and no socket was left after the kills.
- Left to the owner: neusis bump and deploy of hub and node together (an older node given a
  name without a value would start claude with an empty token).
