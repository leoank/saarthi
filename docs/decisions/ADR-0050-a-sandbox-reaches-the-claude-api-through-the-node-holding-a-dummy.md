# ADR-0050: A sandbox reaches the Claude API through the node, holding a dummy token

Status: accepted
Date: 2026-11-13

## Context
ADR-0038 gives a sandboxed session its Claude token as `CLAUDE_CODE_OAUTH_TOKEN`, so the
sandbox can read a credential that lasts a year; ADR-0039 planned a proxy on the node that
adds the token while the sandbox holds a dummy. On seatbelt the sandbox has no loopback TCP
(`hostLoopback: deny`), and mxc emits its loopback proxy rule only when egress is denied by
default, while the owner chose open egress; and `HTTPS_PROXY` cannot add a header to a TLS
tunnel without intercepting it.

[[EXP-20261110-g5mh89]] found Claude Code's `ANTHROPIC_UNIX_SOCKET`, made for `claude ssh`:
it sends its API requests over a unix socket, and with `ANTHROPIC_BASE_URL` set to an
`http://` URL they are plain HTTP/1.1 there, the token in one `Authorization: Bearer`
header. A proxy that replaced it ran a turn and its resume, on the host and inside mxc's
seatbelt sandbox under the node's own policy, with only a dummy inside, the token file
unreadable and loopback refused. Claude Code's other calls to Anthropic (remote settings,
policy limits, an account lookup) go direct, fail with the dummy, and the session runs on.
The owner chose on 2026-11-13 ([[AGS-20261113-28sfr0]]): egress stays open, and only
sandboxed sessions get the proxy.

## Decision
- For each sandboxed spawn or resume the node starts a proxy on a unix socket in the
  profile's state directory, `tmp/claude-api-<8 random hex>.sock`, holding the session's
  token in memory: the one the hub sent, else the profile's `token_file`. The proxy is
  owned by the sandboxed process's handle and stops, removing its socket, when the session
  has exited.
- Inside, the environment has `CLAUDE_CODE_OAUTH_TOKEN=saarthi-proxied`,
  `ANTHROPIC_UNIX_SOCKET=<the socket>` and `ANTHROPIC_BASE_URL=http://api.anthropic.com`.
  The profile's `env` can still override them.
- The proxy (`claude_api` in the node: axum on the socket, reqwest with rustls out) forwards
  only paths under `/v1/` to `https://api.anthropic.com`, with `Authorization: Bearer
  <token>` in place of whatever came in, `x-api-key`, `Host` and hop-by-hop headers
  dropped, and the answer streamed back as it arrives. Anything else gets a 403, logged.
- A token that is empty or not valid in a header refuses the spawn before anything starts.
- Host sessions are unchanged: they run with the owner's privileges and could read the
  token anyway.

## Alternatives considered
- `HTTPS_PROXY` with TLS interception and a CA trusted inside: works for every host, but
  the node would mint and guard a CA, and every TLS connection of the sandbox would be
  decrypted.
- Opening one loopback port in the seatbelt profile: mxc does this only with egress denied
  by default; with open egress it would mean overriding mxc's whole profile.
- Egress denied except the proxy, with an allowlist: tighter, but nix substituters, git over
  SSH and every other tool would need routes or exceptions; the owner kept open egress.
- One socket per profile, shared by its sessions: fewer sockets, but sessions of one profile
  may run under different tokens.

## Consequences
- The sandbox can spend the token through its socket, on the inference API only, but
  cannot read it; a leaked dummy is worth nothing.
- Under open egress seatbelt lets a sandbox connect to any unix socket not under a denied
  or read-only path, so a sandbox that knew another session's socket path could use that
  session's token. The names are random and a sandbox cannot list another profile's state
  directory; sessions of one profile share `tmp/` and can, as they can already read each
  other's transcripts.
- Without its own token, Claude Code inside cannot fetch remote settings or policy limits:
  an organisation's policy restrictions would not reach a sandboxed session. For a
  personal account there are none.
- A sandbox's API traffic now passes through the node: if the node stops, its sandboxed
  sessions lose the API with it, as they already lose their hooks.
