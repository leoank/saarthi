---
id: EXP-20261110-g5mht5
type: experiment
schema_version: 1
title: Does GET /api/oauth/usage, the endpoint behind --claude-usage, accept a claude setup-token token, and if not what other source gives an idle account's usage
project: PRJ-0001
status: concluded           # planned | active | paused | concluded | abandoned
outcome: negative         # null | positive | negative | inconclusive | infra
influenced_by: []
created: 2026-11-10
author: agent:claude
protocols: []
materials: []
tags: [credentials, phase-2]
review: pending
---


## Question
`--claude-usage` (ADR-0030) reads the owner's OAuth access token and calls
`GET /api/oauth/usage` for the 5-hour, 7-day and Fable-week figures. Under ADR-0039 the
hub holds `setup-token` tokens and may want the usage of an account no session is using.
Does the endpoint accept such a token? If not, what else gives an idle account's usage?

## Hypothesis and predictions
If the token's scope covers the profile endpoints, the call returns the same JSON shape as
with the owner's login, and the hub can poll accounts itself. If the token is limited to
inference (as its purpose suggests), the call is refused with 401 or 403; then usage comes
only from the `rate_limit_event`s of running sessions, and the next thing to try is a
minimal Haiku turn (or `claude -p "/usage"` with the token) as a probe.

## Plan
1. With the token from its file in the environment only, call the endpoint once with the
   same headers `--claude-usage` sends (`crates/node`, the usage sampler), recording the
   status and the response's keys, never the token or the account's details.
2. If refused: run `claude -p "/usage"` with the token and an empty config directory,
   and record whether it prints usage and what it costs (a model turn or none).
3. Compare the figures with the owner's own `--claude-usage` reading at the same time.

## Success criteria
- A clear answer: the endpoint accepts the token (with the same figures as the owner's
  login), or it refuses it and a named alternative gives the figures, or none does.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.
- [[RUN-20261111-q60gf1]] the endpoint with a setup-token: 403, "OAuth token does not meet
  scope requirement user:profile"; the hub cannot poll an account through it
- [[RUN-20261111-q6afjv]] the alternatives: `/usage` gives only its own session's totals;
  `count_tokens` carries no windows; any inference request carries every window in its
  headers, which Claude Code streams as `rate_limit_event` (the design uses those, ADR-0042)

## Conclusion
Draft by agent:claude, for the owner to approve.

The endpoint refuses a setup-token ([[RUN-20261111-q60gf1]]): 403, "OAuth token does not
meet scope requirement user:profile"; the token is scoped to inference, so the hub cannot
poll an idle account through it. Of the alternatives ([[RUN-20261111-q6afjv]]),
`claude -p "/usage"` gives only its own session's totals and `count_tokens` carries no
windows; every inference request carries the account's 5-hour and 7-day windows in its
headers, which Claude Code streams as `rate_limit_event`. The figures matched the owner's
login on the same account ([[RUN-20261111-q6t69j]], 0.04 and 0.36).

ADR-0042 followed: a token's usage comes from the sessions under it. An account no session
uses has no fresh figures; a minimal turn of Claude Code itself is the probe left, not
built.
