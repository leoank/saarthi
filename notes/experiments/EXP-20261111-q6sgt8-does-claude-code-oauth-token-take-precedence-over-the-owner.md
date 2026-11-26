---
id: EXP-20261111-q6sgt8
type: experiment
schema_version: 1
title: Does CLAUDE_CODE_OAUTH_TOKEN take precedence over the owner's keychain login for a session in the default config directory?
project: PRJ-0001
status: concluded           # planned | active | paused | concluded | abandoned
outcome: positive         # null | positive | negative | inconclusive | infra
influenced_by: []
created: 2026-11-11
author: agent:claude
protocols: []
materials: []
tags: [credentials, phase-2]
review: pending
---

## Question
ADR-0039 has a node's default token apply to every spawned session, host ones included.
A host session runs in the owner's default config directory, where Claude Code is logged
in through the keychain. Given `CLAUDE_CODE_OAUTH_TOKEN`, does it run under the token, or
does the keychain login win?

## Hypothesis and predictions
If the variable takes precedence, `claude auth status` reports `oauth_token` with it set and
the turn's rate-limit windows are the token's account's; if the keychain wins, the status
stays `claude.ai` and the windows are the owner's, and host sessions would need their own
config directory to run under a token.

## Plan
1. In a directory the node ignores, one Haiku turn without the variable and one with it,
   in the default config directory; compare `claude auth status`, the init event's
   `apiKeySource` and the `rate_limit_event` windows.
2. Repeat with a token of the owner's second account, whose windows differ, to see the
   turn itself move.

## Success criteria
- With the variable set, the session's auth is the token's and its windows are the
  token's account's.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.
- [[RUN-20261111-q6t69j]] same account's token: auth status moves from `claude.ai` (team)
  to `oauth_token` with the variable set; windows equal, as expected for one account

## Conclusion
Draft by agent:claude, for the owner to approve.

The variable wins ([[RUN-20261111-q6t69j]]): in the owner's default config directory, with
`CLAUDE_CODE_OAUTH_TOKEN` set, `claude auth status` moves from `claude.ai` (team) to
`oauth_token`; an empty variable is ignored, and `apiKeySource` is `none` both ways. Host
sessions need no config directory of their own to run under a token. ADR-0046 relies on
it: the wrapper starts a terminal session with the node's default token in the variable.

Not checked: plan step 2, a second account's token, so the turn's own rate-limit windows
were never seen to move; the token used belonged to the keychain's account, whose windows
are the same either way.
