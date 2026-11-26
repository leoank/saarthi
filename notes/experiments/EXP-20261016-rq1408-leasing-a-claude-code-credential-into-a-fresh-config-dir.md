---
id: EXP-20261016-rq1408
previous_id: manthan/EXP-0008
type: experiment
schema_version: 1
title: Leasing a Claude Code credential into a fresh config dir
project: PRJ-0001
status: concluded           # planned | active | paused | concluded | abandoned
outcome: positive             # null | positive | negative | inconclusive | infra
influenced_by: []
created: 2026-10-16
author: ank
protocols: []
materials: []
tags: [credentials, phase-0, leasing]
review: pending
---

## Question
Can a Claude Code credential be copied into a fresh `CLAUDE_CONFIG_DIR` and used for a
headless session without breaking the original login, and does a refresh rotate the
token? Fourth open question.

## Hypothesis and predictions
On macOS the OAuth credential lives in the login keychain under "Claude Code-credentials"
and `.credentials.json` is a fallback. A copied `.credentials.json` in a fresh config
dir is probably ignored while the keychain entry exists, so leasing may require the
keychain to be absent, a file-only mode, or an API key. A refresh may rotate the refresh
token and invalidate the copy or the original.

## Plan
1. Record where the credential is read from: keychain entry attributes and the file's
   key set, without printing secrets.
2. Create a temporary config dir with the copied file, run `claude -p "reply ok"` with
   `CLAUDE_CONFIG_DIR` set, and see whether it authenticates and from where.
3. Compare the original credential before and after; check the main session still
   works.
4. Check whether `apiKeyHelper` in settings gives a cleaner lease path.

## Success criteria
- A definite answer on whether an OAuth credential can be leased per session and how.
- The original login still works at the end.

## Findings
- **An OAuth credential can be leased per session.** Write the credential JSON into
  `<state dir>/.credentials.json` and start claude with `CLAUDE_CONFIG_DIR=<state dir>`. With
  that variable set, claude looks for a keychain item under a hashed service name
  (`Claude Code-credentials-<sha256(dir)[:8]>`, found in [[EXP-20261016-rq1407]]), so the host keychain is
  ignored and the file is used; nothing is written back to the keychain.
- **The state dir is the session's whole world**: transcript under `code/`, settings,
  plugins, session registry. The daemon creates it per session, mounts it into the sandbox,
  and removes it afterwards, which also wipes the lease.
- **Getting the credential into the hub**: on macOS the live one is in the login keychain
  (`security find-generic-password -s "Claude Code-credentials" -w`); on Linux it is
  `~/.claude/.credentials.json`. The file on this Mac was stale, so enrollment must read the
  keychain, not the file.
- **A refresh rotates and revokes** ([[RUN-20261017-dssg3d]], forced by backdating `expiresAt`
  in a leased copy of the main account): the token endpoint returned a new access token and a
  new refresh token, and within a minute every other user of the old pair failed with
  `authentication_failed`, including the owner's interactive session, which needed `/login`.
  The old refresh token then answered `invalid_grant` and claude wiped that store to empty
  tokens. A fresh `/login` is a separate grant: the owner's new login left the lease's
  rotated pair working. Refresh is attempted when the access token is within five minutes of
  `expiresAt`; access tokens last eight hours, refresh tokens about thirty days from login,
  and rotation keeps the refresh expiry.
- **Where the refreshed pair is saved**: on macOS with `CLAUDE_CONFIG_DIR` set, into a keychain
  item `Claude Code-credentials-<sha256(configdir)[:8]>`, after which `.credentials.json` is
  deleted. Read-back on a Mac must look there. The save path inside a seatbelt sandbox without
  keychain access is still untested.
- **Grants are independent and revocable** ([[RUN-20261017-ebc0qq]]): a forced refresh on the
  lease's own grant left the owner's fresh login untouched, an older pair of the same grant
  was rejected and wiped, and `claude auth logout` with the slot's config dir revoked the
  grant at the server (the saved access token got 401 afterwards).
- **Consequence for the hub**: a credential is a grant and a grant is one session's worth. One
  account needs one login per concurrent session (slots), the hub leases a slot to one
  session at a time, reads it back afterwards, and never copies a person's live keychain
  credential into a lease. `claude auth status` with the slot's config dir is a free health
  probe (`loggedIn`, `subscriptionType`); `claude auth logout` there is the revoke button.
- An API key through `ANTHROPIC_API_KEY` or `apiKeyHelper` remains the simpler lease for
  accounts that have one; it was not tested here.

## Runs
- [[RUN-20261016-rq14w7]] lease of the keychain credential into a temp config dir
- [[RUN-20261017-dssg3d]] forced refresh inside a lease: rotation, revocation, keychain write-back
- [[RUN-20261017-ebc0qq]] control: refresh on the lease's own grant, logout revokes
- [[RUN-20261017-m3gg0y]] planned, deferred: do many sessions on one credential store survive a refresh

## Conclusion
Draft by agent:claude, for the owner to approve.

Yes, an OAuth credential can be leased per session, and a refresh rotates it. Written to
`<dir>/.credentials.json` and started with `CLAUDE_CONFIG_DIR=<dir>`, Claude Code uses the
file and ignores the host keychain ([[RUN-20261016-rq14w7]]). A refresh returns a new
access and refresh token and revokes the old pair within a minute, so a copy of a live
login kills the original (the owner's session needed `/login`); on macOS the refreshed pair
is saved to a hashed keychain item and the file is deleted ([[RUN-20261017-dssg3d]]).
Separate logins are separate grants: a refresh on one leaves the other alone, and
`claude auth logout` revokes a grant at the server ([[RUN-20261017-ebc0qq]]). The original
login works at the end.

This led to the leased slots of ADR-0018, one login per concurrent store, later superseded
by the non-refreshing `claude setup-token` tokens of ADR-0039. Left unchecked: whether many
sessions on one store survive a refresh ([[RUN-20261017-m3gg0y]], planned, not run; with
ADR-0039 it no longer gates anything), the save path inside a sandbox without keychain
access, and an API key as the lease.
