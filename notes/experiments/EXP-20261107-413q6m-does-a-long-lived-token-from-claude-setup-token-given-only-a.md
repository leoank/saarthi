---
id: EXP-20261107-413q6m
type: experiment
schema_version: 1
title: Does a long-lived token from claude setup-token, given only as CLAUDE_CODE_OAUTH_TOKEN with an empty config dir, run headless stream-json turns, resume, and report rate limits, without touching the keychain or the owner login
project: PRJ-0001
status: concluded          # planned | active | paused | concluded | abandoned
outcome: positive         # null | positive | negative | inconclusive | infra
influenced_by: []
created: 2026-11-07
author: agent:claude
protocols: []
materials: []
tags: [confinement, phase-3, credentials]
review: pending
---

## Question
A sandboxed session cannot share the owner's keychain login (a second store of one grant
is revoked at the first refresh, [[RUN-20261017-dssg3d]]) and leases are not built yet.
`claude setup-token` makes a separate, long-lived grant. Does that token, given only as
`CLAUDE_CODE_OAUTH_TOKEN` to a session whose `CLAUDE_CONFIG_DIR` is an empty directory,
run a headless stream-json turn, resume it, and report `rate_limit_event`s, while the
keychain and the owner's login are untouched?

## Hypothesis and predictions
If the token is a plain bearer credential with no refresh, a turn completes with no
`.credentials.json` written and no new `Claude Code-credentials-*` keychain item, the
resumed session keeps its id, and the owner's interactive sessions keep working. If it
needs a refresh or a keychain item, the sandbox design needs the lease route
(phase 2 step 3) first.

## Plan
1. The owner runs `claude setup-token` in their own terminal and saves the token to
   `~/.config/rc2s2/claude-oauth-token` (mode 600). The agent never prints it.
2. List the keychain items named `Claude Code-credentials*` (names only) before.
3. `claude -p` stream-json with an empty config dir and the token in the environment:
   one turn, then `--resume` with a code word; record `rate_limit_event`, the files the
   config dir holds afterwards, and the keychain item names again.
4. Check the owner's own login still answers (`claude auth status`, default dir).

## Success criteria
- A turn and a resume complete with only the token, the code word survives the resume.
- No keychain item is created or changed; no credential file appears in the config dir.
- The owner's login is still logged in afterwards.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.
- [[RUN-20261107-44gt73]] placeholder made before `yajna result`; its script is the one the result run records
- [[RUN-20261107-4dxwmx]] the token alone with an empty config dir: a turn and a resume under one id kept the code word, two `rate_limit_event`s (five_hour, allowed), `apiKeySource` none, no credential file written, keychain item names unchanged, the owner still logged in

## Findings
Every prediction held ([[RUN-20261107-4dxwmx]], Claude Code 2.1.318): `CLAUDE_CODE_OAUTH_TOKEN`
alone runs headless stream-json turns and `--resume`s them in a config directory that
starts empty and never receives a credential file; the keychain's `Claude Code-credentials*`
items are the same before and after, and the owner's own login stays logged in. Rate-limit
events still arrive (`five_hour`, `allowed`). The token is its own grant, so it needs no
lease and no refresh while it lasts (a year, by `claude setup-token`'s description).

## Conclusion
Draft by agent:claude, for the owner to approve.

The prediction holds ([[RUN-20261107-4dxwmx]], Claude Code 2.1.318): a `claude setup-token`
token given only as `CLAUDE_CODE_OAUTH_TOKEN`, with an empty `CLAUDE_CONFIG_DIR`, runs a
headless stream-json turn and a `--resume` under the same id that keeps the code word, and
reports `rate_limit_event`s. No credential file is written, the keychain's item names are
unchanged, and the owner's own login stays logged in. The token is a grant of its own, so
a sandbox needs neither a lease nor a refresh while it lasts.

ADR-0038 runs sandboxed spawns on such a token instead of waiting for leases, and ADR-0039
makes named setup tokens held by the hub the way sessions run. Not checked: what happens
when the token expires or is revoked mid-session.
