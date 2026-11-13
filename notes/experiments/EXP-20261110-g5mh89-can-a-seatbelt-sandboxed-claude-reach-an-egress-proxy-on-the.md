---
id: EXP-20261110-g5mh89
type: experiment
schema_version: 1
title: Can a seatbelt-sandboxed claude reach an egress proxy on the node that adds a setup-token as its credential, with a dummy token inside and no TLS interception
project: PRJ-0001
status: active            # planned | active | paused | concluded | abandoned
outcome: null             # null | positive | negative | inconclusive | infra
influenced_by: [EXP-20261107-413q82]
created: 2026-11-10
author: agent:claude
protocols: []
materials: []
tags: [credentials, phase-2]
review: pending
---


## Question
Under ADR-0039 an egress proxy on the node should hold the token and add it to requests
to the Anthropic API, so a sandboxed session holds only a dummy. On seatbelt the sandbox
has no loopback TCP (`hostLoopback: deny`, ADR-0038), and `HTTPS_PROXY` cannot add a
header to a TLS tunnel without intercepting it. Can a sandboxed `claude` reach a proxy
that adds the token, with no TLS interception, and does Claude Code accept a dummy token
and a different base URL?

## Hypothesis and predictions
If Claude Code honours `ANTHROPIC_BASE_URL` with plain HTTP and sends the OAuth token as
a header the proxy can replace, then a proxy that rewrites that header and forwards over
TLS to api.anthropic.com runs a headless turn with the dummy inside. The transport is the
open part: loopback TCP is refused in the sandbox, so we expect to need either a unix
socket (if the base URL or a proxy setting accepts one), an exception for one loopback
port in the seatbelt profile, or a non-loopback address of the node. If none works, the
proxy has to intercept TLS with its own CA trusted inside the sandbox, or the token stays
in the sandbox.

## Plan
1. On the host, no sandbox: a small forwarding proxy that replaces the auth header with
   the real token. Run `claude -p` with a dummy `CLAUDE_CODE_OAUTH_TOKEN` and
   `ANTHROPIC_BASE_URL=http://127.0.0.1:<port>`; record whether the turn runs, which
   header carries the token, and any other host it calls (telemetry, the usage endpoint).
2. The same inside the seatbelt sandbox: first as is (expect loopback refused), then with
   each candidate transport: a unix-socket base URL, one allowed loopback port, the node's
   tailnet address.
3. With egress denied except the proxy, check that the session still works and that
   nothing inside can read the real token.

## Success criteria
- A turn and a resume run inside the sandbox with only a dummy token, through a named
  transport, with egress limited to the proxy; or a clear account of why not and which
  fallback is left.

## Findings
H1 held through the unix socket ([[RUN-20261113-2s2fpy]], [[RUN-20261113-2s4d8h]]).

- Claude Code's `ANTHROPIC_UNIX_SOCKET` (made for `claude ssh`) is a transport: alone it
  carries TLS; with `ANTHROPIC_BASE_URL=http://api.anthropic.com` it carries plain
  HTTP/1.1, the token in one `Authorization: Bearer` header. A proxy that swaps it and
  forwards over TLS runs a turn and a resume with only a dummy inside. No TLS interception,
  no loopback, no change to the sandbox's egress.
- Inside mxc's seatbelt sandbox under the node's policy the socket in the state directory
  connects while loopback TCP is refused, the token file is unreadable and only the dummy
  is in the environment.
- Only `/v1/messages` uses the base URL. Remote settings, policy limits and an account
  check go straight to `api.anthropic.com` and fail with the dummy; the session runs on, the
  model is unchanged, but an organisation's policy limits would not apply.
- mxc's own proxy support opens a loopback port only when egress is denied by default.

The node's proxy built on this (ADR-0050) ran a real sandboxed session spawned through a
hub end to end ([[AGS-20261113-28sfr0]]).

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.

- [[RUN-20261113-2s2fpy]] on the host: plain HTTP over the socket with an http:// base URL, a turn and a resume with a dummy, what still goes direct; positive
- [[RUN-20261113-2s4d8h]] inside seatbelt under the node's policy: loopback refused, token unreadable, a turn and a resume through the socket; positive
- [[RUN-20261113-2s2fpy]] compute run, not yet described
- [[RUN-20261113-2s4d8h]] compute run, not yet described

## Conclusion
Written once, by a human, when status becomes `concluded`.
