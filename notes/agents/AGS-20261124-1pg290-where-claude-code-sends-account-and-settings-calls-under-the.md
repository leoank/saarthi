---
id: AGS-20261124-1pg290
type: agent_session
schema_version: 1
title: Where Claude Code sends account and settings calls under the node proxy, and whether a hub token answers them
model: claude-opus-5-5
harness: claude-code
started: 2026-11-24T01:33+00:00
finished: 2026-11-24T01:39+00:00
created_entities: [EXP-20261124-1pg2cq, RUN-20261124-1qtntf, RUN-20261124-1sae23]
edited_entities: []
outcome: done             # done | failed | aborted | pending
created: 2026-11-24
author: agent:claude
---

## Task
Another agent proposed letting read-only `/api/` paths through the node's and hub's Claude
API proxy (ADR-0051), so a session under a hub token shows that token's account instead of
the machine's Keychain login. The owner asked for the experiment first.

## What happened
- The real node's log on rogue had hub-token sessions relaying `/v1/messages` and not one
  refused `/api/` path, against the proposal's premise.
- [[EXP-20261124-1pg2cq]], runs [[RUN-20261124-1qtntf]] and [[RUN-20261124-1sae23]]: with a
  stand-in proxy and a refusing `HTTPS_PROXY`, Claude Code 2.1.330 sent only
  `POST /v1/messages` through the socket; every other call went to Anthropic directly, at a
  terminal its start-up check too; `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1` cut their
  number, not their route. The hub-token half (run 2 in the plan) was dropped: moot.
- Proposed conclusion: H1 is false; the proxy never sees those calls, so options 1 and 2
  cannot work. Showing the token's account would need all of Claude Code's HTTPS to
  `api.anthropic.com` through the node, an intercepting proxy with a CA Claude Code trusts,
  which I do not recommend for a cosmetic label.

