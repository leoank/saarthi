# ADR-0042: A Claude token's usage comes from the sessions that run under it

Status: accepted
Date: 2026-11-11

## Context
ADR-0039 has the hub hold named `claude setup-token` tokens. The owner now has two Claude
accounts and wants each account's 5-hour and 7-day usage in the app, with the rate-limit
alerts of ADR-0032 per account, if the usage endpoint allows it
([[AGS-20261111-q5z55t]]). It does not: `GET /api/oauth/usage` refuses a setup-token with
403, "OAuth token does not meet scope requirement user:profile"
([[RUN-20261111-q60gf1]]). `claude -p "/usage"` reports only its own session's totals.
Every inference request under the token does carry the account's windows in its response
headers, and Claude Code passes them to a stream-json session as `rate_limit_event`
(`unifiedWindows`, utilisation and reset per window) on every turn
([[RUN-20261111-q6afjv]], [[EXP-20261016-rq1402]]).

## Decision
- The node forwards each `rate_limit_event` of a session it spawned as a daemon event
  `rate_limit` with its `rate_limit_info`. The hub, storing it, looks up the Claude token
  the session runs under and records the windows on that token (newest report wins), then
  runs the ADR-0032 alert rules on them, titled with the token's name and keyed by it, so
  each account alerts once whichever node reports it. Sessions under a node's own login
  change nothing here; their account's figures still come from the node's health.
- Clients see, per token, the latest windows and when they were reported. An account no
  session has used since the hub started shows none.
- The hub records the token a session ran under itself, when the node accepts the spawn or
  resume, and keeps it when a restarted node reports the session without one; a resume
  without a pick runs under it again.
- The protocol: a spawn's token is optional, absent for the node's default and empty for
  the node's own login; a resume's absent token is the session's last. A token the hub
  does not hold, picked or by default, is refused before the node is asked. A sandboxed
  profile's `token_file` becomes optional, a fallback while nodes have no default.
- The expiry reminder is a notification kind of its own, `token`, checked hourly.

Settled by the agent, for the owner to revisit: the alert title "<name> account: ...";
the reminder's wording; no probe turn to refresh an idle account's figures.

## Alternatives considered
- Calling the API directly with the token for a one-token request, which returns every
  window (probe 3 of [[RUN-20261111-q6afjv]]): cheapest, but it uses a subscription token
  outside Claude Code, which the owner should not have to risk.
- A scheduled minimal Claude Code turn per token from a node: figures for idle accounts at
  the cost of a turn each time; possible later, on demand from the app, if stale figures
  turn out to matter.
- No usage per token: the owner asked for it.

## Consequences
- Usage per account is as fresh as the last turn under its token: live while sessions
  run, stale after; the app shows how old it is.
- Interactive sessions started by hand never feed it; they run under the node's login.
- The usage endpoint's refusal also means `--claude-usage` can never move to tokens; it
  stays on the node's keychain login.
