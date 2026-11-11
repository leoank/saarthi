---
id: EXP-20261110-g5mh7g
type: experiment
schema_version: 1
title: Does one claude setup-token token, used by several headless sessions at once on two machines, keep working through hours of turns, with no session or the owner's login disturbed
project: PRJ-0001
status: active            # planned | active | paused | concluded | abandoned
outcome: null             # null | positive | negative | inconclusive | infra
influenced_by: []
created: 2026-11-10
author: agent:claude
protocols: []
materials: []
tags: [credentials, phase-2]
review: pending
---


## Question
ADR-0039 lets every session of an account on a node, and several nodes, share
`setup-token` tokens on the grounds that nothing refreshes. [[EXP-20261107-413q6m]] ran
one session at a time. Does one token, used by several headless sessions at once on two
machines, keep working through hours of turns, without disturbing any of them or the
owner's own login?

## Hypothesis and predictions
If the token is a plain bearer credential with no server-side rotation, every turn in
every session succeeds for the whole run, no session's config directory gains a
credential file, and the token is unchanged. If use from two places invalidates it (a
binding to one client, or a rotation we have not seen), a session starts failing with an
auth error, and ADR-0039 falls back to one token per session or to leases.

## Plan
1. The owner makes one token (`claude setup-token`) and puts it on rogue and on a second
   machine (a Linux node or a scratch Mac), mode 600. The agent never prints it.
2. On each machine start three `claude -p` stream-json sessions with an empty config
   directory each and the token as `CLAUDE_CODE_OAUTH_TOKEN`, on Haiku.
3. Send each a short prompt every ten minutes for four hours, from a script that logs the
   time, the session, the `result` subtype and any API error; then resume one session
   per machine by id.
4. Afterwards: list each config directory, the keychain item names (rogue), and
   `claude auth status` for the owner's login.

## Success criteria
- Every turn of the six sessions over four hours completes, and the resumes keep their ids.
- No credential file or keychain item appears; the owner is still logged in.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.
- [[RUN-20261111-q7np8h]] rogue's half: three Haiku sessions on one token, a turn each
  every ten minutes for four hours, then a resume; running (the second machine's half
  waits for one)

## Conclusion
Written once, by a human, when status becomes `concluded`.
