# ADR-0051: The hub adds its Claude tokens and never hands them out

Status: accepted
Date: 2026-11-14
Supersedes: the delivery in ADR-0039 and ADR-0046, and the token holder of ADR-0050 for
the hub's tokens

## Context
ADR-0039 has the hub send a named token's value to the node with every spawn, resume and
usage probe, and ADR-0046 to a terminal session's wrapper; ADR-0050 then keeps it out of a
sandbox by a proxy on the node. Every node that ever ran a session under a token has held
it, so cutting a node off means renewing the token (a `claude setup-token`, a year long)
everywhere. The owner asked on 2026-11-14 that the token never leave the hub
([[AGS-20261114-d827jm]]).

[[EXP-20261114-d82rcz]] checked what that rests on, through a two-hop rig: the interactive
TUI runs over `ANTHROPIC_UNIX_SOCKET` with a dummy, as `-p` did, with a warning that remote
settings failed and starting in manual permission mode where the account's default is now
auto (auto itself works); a hub gone mid-stream for up to 90 s costs one retried request
and the turn finishes whole, while about 145 s ends it with an API error; a request is the
whole context every call, 0.6 MB at the median and 2.4 MB at most on the owner's machine,
about 11 GB a month; and every answer carries the account's unified rate-limit headers.

The owner chose on 2026-11-14: every session under a hub token goes through the hub (host,
sandboxed, terminal, probes); the node's own login stays direct; a hub restart may cut
running turns, zero-downtime restarts a later goal; such sessions start in auto mode by
default; the hub records usage from the headers it relays; compressing node-to-hub traffic
is a later goal (ghost001 has 10 TB a month).

## Decision
- **No value leaves the hub.** Spawn, Resume, Probe and the `ClaudeToken` call carry the
  token's name only; `ClaudeToken.token` stays empty.
- **A new node-service call**, `ClaudeApi`, a bidirectional stream on the node's existing
  connection to the hub (TCP or its tailnet, authenticated by its node token): the request
  head (the token's name, method, path, headers) and the body in pieces of at most 1 MB,
  then the response head and the body as it arrives. The hub refuses an unknown node token
  or token name and any path outside `/v1/`, drops `Authorization`, `x-api-key`, `Host` and
  hop-by-hop headers, sets `Authorization: Bearer <token>`, sends the request to
  `https://api.anthropic.com` and streams the answer back. A failure before the answer is a
  502 to claude, which retries it; an unknown name a 403.
- **The node relays.** For every session under a hub token it runs a proxy on a unix socket
  (as in ADR-0050) whose upstream is `ClaudeApi` under the token's name, and starts the
  session with `CLAUDE_CODE_OAUTH_TOKEN=saarthi-proxied`, `ANTHROPIC_UNIX_SOCKET` and
  `ANTHROPIC_BASE_URL=http://api.anthropic.com`:
  - a sandboxed spawn or resume: the socket in the profile's `tmp/`, as now;
  - a host spawn or resume, and a usage probe: a socket in a per-user directory under the
    temporary directory, which every sandbox is denied;
  - a terminal session: the daemon answers the wrapper with the token's name and a socket
    path in that directory; the wrapper starts its program with the dummy and the socket and
    names both in its hello; the daemon serves the socket while the hello's connection
    lasts, and again when the wrapper reconnects after a daemon restart.
  The proxy's socket and connection go when the session ends.
- **Auto by default.** A spawn or resume under a hub token with no permission mode gets
  `--permission-mode auto`; `saarthi run` adds it before the program's arguments when the
  program is `claude` and they set no permission mode of their own.
- **Usage at the hub.** From each answer's `anthropic-ratelimit-unified-5h-*` and
  `-7d-*` utilization and reset headers the hub records the token's windows and alerts as
  for a session's report (ADR-0042), at most once a minute per token unless they change.
  Sessions' reports and probes stay.
- **What stays.** The node's own login is untouched and direct. A sandboxed profile's own
  `token_file` (ADR-0038) is a node-held token: its proxy still adds it on the node, as in
  ADR-0050.

## Alternatives considered
- Keep the token on the node (ADR-0039, ADR-0050): no hub in the path, but a node keeps
  what it was given, and cutting one off means renewing the token everywhere.
- An HTTP endpoint on the hub beside gRPC: plain HTTP is simpler to relay, but it would need
  a listener and client of its own on every transport the node uses, the tailnet included,
  and its own authentication.
- Only sandboxed sessions through the hub: the TUI's differences avoided, but host and
  terminal sessions would still carry the token.
- Compression between node and hub now: the owner's bandwidth makes it unnecessary for now.

## Consequences
- Revoking a node's hub token (its line in `--node-tokens` and a hub restart) cuts it off at
  once, running turns included; no Claude token needs renewing. A node can still spend a
  token while it is trusted, and sees the requests and answers it relays.
- The hub is in the path of every turn under its tokens: a restart costs a retried request,
  an outage longer than about two minutes ends turns with an API error, and the session
  resumes. All of that traffic crosses the hub's link twice, and the hub sees it in plain
  text.
- Sessions under a hub token lose Claude Code's own calls to Anthropic with the dummy:
  remote managed settings and policy limits (a warning at the TUI's start), and the
  account's default permission mode, hence auto by default.
- A node and a hub must be upgraded together: an older node given a name without a value
  would start claude with an empty token.
- Usage now arrives with every answer, for every session under a hub token, whether or not
  its stream reports it.
