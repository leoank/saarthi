---
id: EXP-20261025-25f6vm
type: experiment
schema_version: 1
title: Does the usage endpoint answer with the access token read from the keychain, read only, without disturbing the login?
project: PRJ-0001
status: concluded            # planned | active | paused | concluded | abandoned
outcome: positive             # null | positive | negative | inconclusive | infra
influenced_by: [EXP-20261016-rq1402, EXP-20261016-rq1408]
created: 2026-10-25
author: agent:claude
protocols: []
materials: []
tags: [usage, credentials, keychain, phase-1.5]
review: pending
---

## Question
ADR-0030 has the daemon call `GET api.anthropic.com/api/oauth/usage` with the owner's OAuth
access token to show the 5-hour and 7-day utilisation in the app. [[EXP-20261016-rq1402]]
called the endpoint by hand with a credential JSON on stdin; [[EXP-20261016-rq1408]] showed
that a credential copied into a second store revokes the login on the next refresh, and
that on macOS the live pair sits in the keychain item `Claude Code-credentials`, read back
by Claude Code on version change. Unchecked: that a daemon reading that item once a minute,
never writing it, leaves the owner's login untouched across a refresh; that the keychain
grants a launchd agent the read without a prompt; and that a token read moments before
Claude Code refreshes it still answers (or fails cleanly and is re-read).

## Hypothesis and predictions
H1: reading the keychain item with the `security` API from the daemon's process does not
change the item, its version, or the login; Claude Code's own refresh goes on as before.
H2: the first read from a launchd agent prompts for keychain access once (or not at all for
a generic password created by the same user), after which reads are silent. H3: a 401 from
the endpoint right after a refresh is recovered by re-reading the item. If H1 is false we
expect a logout or a revoked token after the next refresh, and the design falls back to
Claude Code's cached value (refreshed by `/usage` only).

## Plan
1. A scratch keychain item with the same shape as Claude Code's (hashed name, as the
   phase 0 scripts did) and a daemon build that reads it: confirm the read is silent from
   launchd and leaves the item unchanged (version, mtime, contents).
2. With the owner present: the daemon reads the real item read only (printing an 8-hex
   hash of the token, never the token) and calls the endpoint; compare the utilisation
   object with `claude -p "/usage"`.
3. Wait across one access-token refresh (`expiresAt` within 5 min) with the daemon polling
   once a minute; `claude auth status` stays logged in and the daemon's next reads succeed.

## Success criteria
- The item is byte-identical before and after an hour of reads; `claude auth status` is
  logged in; no revocation.
- The endpoint answers from the daemon with the same shape as EXP-20261016-rq1402.
- Across a refresh, at most one failed call, recovered by the next read.

## Runs
- [[RUN-20261025-9fytgj]] step 1, a scratch item and a fake endpoint: the daemon's poller reads the item silently with no terminal, calls the endpoint with the bearer and beta header, the windows reach the hub, the item is unchanged, the token in no log
- [[RUN-20261025-c35jpc]] step 2, the real item and the real endpoint, with the owner present: read with no prompt and no terminal, 40% and 40% as `/usage` showed, item and login unchanged
- [[RUN-20261025-c5xjbh]] step 3, the same polling across the token's expiry: Claude Code's refresh at 12:55Z left the login intact and the daemon reads the new token, but the endpoint answered 429 for half an hour after about 30 calls in 30 minutes

## Findings
- The mechanics hold: with no controlling terminal the daemon reads a keychain item
  through `security find-generic-password -w` within the ten-second limit, and the
  endpoint's windows reach the hub as `usage.five_hour_pct`, `usage.seven_day_pct` with
  `usage.age_s` 0, the names the sampler already uses for Claude Code's cached value
  ([[RUN-20261025-9fytgj]]).
- Not shown: the real item's access control, which Claude Code set when it created it. Steps
  2 and 3 need the owner at the keyboard, and until they pass the poller stays off
  (`--claude-usage` is not set for rogue).
- The real item reads with no prompt from a process with no terminal: `/usr/bin/security`
  may read what Claude Code wrote, and the windows match `/usage` (40% and 40%,
  [[RUN-20261025-c35jpc]]).
- H1 holds across a refresh: Claude Code refreshed the token itself at 12:55:36Z, before
  its expiry, the login stayed, nothing else touched the item, and the daemon's next read
  used the new token ([[RUN-20261025-c5xjbh]]). H3 was not exercised: no call fell between
  the refresh and the expiry.
- New: the endpoint limits its callers. After about 30 calls in 30 minutes it answered
  `429 Too Many Requests` to every poll for half an hour. A poll once a minute cannot run
  all day; the interval has to be longer and a 429 has to slow the poller down. Whether
  the limit is shared with Claude Code's own `/usage` is not known.

## Conclusion
Draft by agent:claude, for the owner to approve.

H1 holds: reading the keychain item read only leaves it and the login unchanged, including
across Claude Code's own refresh at 12:55Z, after which the daemon read the new token
([[RUN-20261025-9fytgj]], [[RUN-20261025-c5xjbh]]). H2 holds without even the one prompt:
with no terminal, `security` reads the real item silently, and the endpoint's windows
matched `/usage` (40% and 40%, [[RUN-20261025-c35jpc]]). H3 was not exercised: no call fell
between the refresh and the old token's expiry.

New, and it changed the design: the endpoint rate-limits its callers, answering 429 to
every poll for half an hour after about 30 calls in 30 minutes ([[RUN-20261025-c5xjbh]]).
The poller of ADR-0030 now runs every five minutes and backs off on a 429 (phase 1.5 step
6, the owner's choice). Whether the limit is shared with Claude Code's own `/usage` is
not known.
