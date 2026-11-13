---
id: AGS-20261113-28sfr0
type: agent_session
schema_version: 1
title: "Phase 3 item 2.4: the egress proxy on the node that adds the Claude token, so a sandbox holds a dummy"
model: claude-opus-5-5
harness: claude-code
started: 2026-11-13T02:04+00:00
finished: 2026-11-13T02:34+00:00
created_entities: [RUN-20261113-2s2fpy, RUN-20261113-2s4d8h]
edited_entities: [EXP-20261110-g5mh89, PHS-20261106-3702q3]
outcome: done             # done | failed | aborted | pending
created: 2026-11-13
author: agent:claude
---

## Task
After the phone's tailnet identity, the owner asked for phase 3 item 2.4: the proxy that
adds the Claude token so a sandboxed session holds only a dummy (ADR-0039,
[[EXP-20261110-g5mh89]]). Their choices: egress stays open, only sandboxed sessions get
the proxy, and the experiment may run a few turns on the sandbox profile's token.

## What happened
- Found that mxc opens a loopback proxy port only when egress is denied by default, and
  that Claude Code has `ANTHROPIC_UNIX_SOCKET` (for `claude ssh`). Probed it with a stub
  that holds no credential: TLS over the socket alone; plain HTTP/1.1 with an `http://`
  base URL, the token in one header.
- Ran the experiment ([[RUN-20261113-2s2fpy]] on the host, [[RUN-20261113-2s4d8h]] in
  seatbelt under the node's policy): a turn and a resume with only a dummy inside, the
  token file unreadable, loopback refused. Remote settings, policy limits and an account
  check still go direct and fail with the dummy; the session runs on.
- Built it into the node: `claude_api` (axum on a unix socket, reqwest with rustls out,
  `/v1/` only, the response streamed), one proxy per sandboxed spawn or resume in
  `<state>/tmp`, owned by the sandboxed process's handle; the profile's environment gets
  the dummy, the socket and the base URL; a token not valid in a header refuses the spawn.
  Tests: the token swap against a fake API, `/v1/` only, a too-long socket path, and the
  live seatbelt test now checks the dummy inside, the proxy's 403 from inside and the
  socket gone after the session.
- End to end on rogue: a dev node with a seatbelt profile linked to the scratch hub; a
  session spawned with `ctl spawn --profile box` answered through the node's proxy, no
  file in the state directory held the token, and its socket was gone two seconds after
  `ctl kill`.
- Decided without asking: one socket per session (sessions of a profile may run under
  different tokens), `/v1/` only, the dummy `saarthi-proxied`, request bodies up to 64 MB
  buffered, the profile's `env` may override the three variables.
- ADR-0050, the architecture, the node guide; versions to 0.6.0 (app versionCode 6).
