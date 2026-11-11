# ADR-0043: Usage probes keep every account's usage fresh, half-hourly

Status: accepted
Date: 2026-11-11

## Context
ADR-0042 takes an account's 5-hour and 7-day windows from the sessions that run under its
token, so an account nobody has used lately shows old figures, and its rate-limit alerts
wait for the next session. It left a scheduled probe turn as a possibility. The owner asked
for it, every half hour with some randomness, in throwaway sessions
([[AGS-20261111-q5z55t]]). One Haiku turn of Claude Code in an empty config directory
costs about 18.7k input tokens, nearly all cached, and 4 output tokens: about $0.0015 at
list price, a second of time, and no visible change to the 5-hour window.

## Decision
- The hub's `--usage-probe-node <name>` names the node that runs probes;
  `--usage-probe-minutes` (default 30) sets the interval. Without the flag there are none.
- Per token, the hub waits a time drawn between 85% and 115% of the interval (25 to 35
  minutes by default), then sends the node a `Probe` with the token, unless a session under
  that token reported its windows within 85% of the interval. The first round after a start
  is spread over the first five minutes. A probe's windows are recorded and alerted on as a
  session's are (ADR-0042).
- The node runs one `claude -p` turn on Haiku, through the login shell, in a fresh
  temporary directory with an empty config directory there: no settings, no hooks, nothing
  the node lists or tails. It returns the turn's `rate_limit_info` and removes the
  directory. It runs on a task of its own so the link keeps flowing. A node without spawn
  roots refuses, as it refuses spawns.
- The randomness spreads probes out; they are ordinary Claude Code runs and do not try to
  look like anything else.

Settled by the agent, for the owner to revisit: Haiku and the one-line prompt; the 15%
spread; skipping an account a session reported on recently; probes on one named node.

## Alternatives considered
- A direct API request per token: cheaper, but a subscription token used outside Claude
  Code (ADR-0042).
- Probes on every node, or the hub choosing one: the owner's always-on host is the natural
  place, and naming it keeps a laptop from running turns unasked.
- A fixed interval: the owner asked for randomness.

## Consequences
- Each account's usage is at most about half an hour old, and its alerts come within that
  time even when nothing runs under it.
- Each probe is a small turn on the account, about 48 a day per account at the default.
- A probe that fails (a revoked or expired token, the node offline) is logged and tried
  again next round; it shows as the account's figures going stale.
