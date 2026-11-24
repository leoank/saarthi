---
id: EXP-20261124-1pg2cq
type: experiment
schema_version: 1
title: Under the node proxy (ADR-0051), where does Claude Code send its account and settings calls, and would a hub token answer them?
project: PRJ-0001
status: active            # planned | active | paused | concluded | abandoned
outcome: negative           # null | positive | negative | inconclusive | infra
influenced_by: []
created: 2026-11-24
author: agent:claude
protocols: []
materials: []
tags: []
review: pending
---

## Question
A session under a hub token (ADR-0051) gets `CLAUDE_CODE_OAUTH_TOKEN=saarthi-proxied`,
`ANTHROPIC_UNIX_SOCKET` and `ANTHROPIC_BASE_URL=http://api.anthropic.com`; the node's proxy
on that socket forwards `/v1/` only, the hub too. Claude Code then shows the machine's own
Keychain account and warns at start. Another agent proposed letting read-only `/api/`
paths (`/api/oauth/profile`, `/api/remote_managed_settings`, `/api/policy_limits`) through
the proxy and the hub. That rests on two things nobody has checked: (a) that Claude Code
sends those calls through the proxy at all, and (b) that a `claude setup-token` token, which
the hub holds, is answered on them.

## Hypothesis and predictions
H1: Claude Code sends its account and settings calls to the base URL, through the proxy.
- If H1 holds: the proxy's socket sees `/api/...` requests besides `/v1/messages`, and no
  direct connection goes out.
- If it is false: the socket sees `/v1/` only, and direct connections go to
  `api.anthropic.com` (or another host) for the rest, with the dummy token: then widening
  the proxy changes nothing, and the fix lies elsewhere.

H2 (run 2, on the hub, with a hub token): `/api/oauth/profile` and the settings paths
answer 200 to a setup-token token. If they answer 401 or 403 (an inference-only scope),
forwarding them would not help either.

The node's own log on rogue already leans against H1: sessions under a hub token there
logged `/v1/messages` relays and not one refused `/api/` path.

## Plan
1. Run 1, here, nothing leaving the machine: Claude Code 2.1.330's bare binary, with a fresh
   `CLAUDE_CONFIG_DIR` (no settings, no hooks), the node's three variables, a logging unix
   socket in place of the proxy that answers 401 to everything, and `HTTPS_PROXY` at a
   logging proxy that records each `CONNECT` and refuses it. Once in `-p` mode (a spawned
   session) and once at a terminal (a `saarthi run` session), each for about 20 s.
2. Run 2, only if run 1 shows calls going through the proxy: on the hub, each hub token's
   status on those paths, printing statuses only, never a token.

## Success criteria
The socket and proxy logs name every path and host Claude Code tried, in both modes. H1 is
decided by whether any `/api/` path reached the socket.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.

- [[RUN-20261124-1qtntf]] the node's environment, both modes: only `POST /v1/messages` reached the proxy's socket; 21 (`-p`) and 21 (terminal) other connections went straight to Anthropic, so H1 is false and widening the proxy would change nothing
- [[RUN-20261124-1sae23]] the same with `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`: 14 and 16 direct connections, still none through the proxy
- [[RUN-20261124-1qtntf]] compute run, not yet described
- [[RUN-20261124-1sae23]] compute run, not yet described

## Conclusion
Written once, by a human, when status becomes `concluded`.
