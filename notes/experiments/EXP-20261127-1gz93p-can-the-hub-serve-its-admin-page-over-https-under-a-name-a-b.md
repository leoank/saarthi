---
id: EXP-20261127-1gz93p
type: experiment
schema_version: 1
title: Can the hub serve its admin page over HTTPS under a name a browser trusts, so a passkey registers and signs in?
project: PRJ-0001
status: planned           # planned | active | paused | concluded | abandoned
outcome: null             # null | positive | negative | inconclusive | infra
influenced_by: []
created: 2026-11-27
author: agent:claude
protocols: []
materials: []
tags: []
review: pending
---

## Question
ADR-0082 signs people in to the hub's admin page with passkeys, which a browser registers
only on a secure origin. The hub serves no HTTPS today. Can its admin listener get a
certificate a browser trusts, from the tailnet (tsnet's certificate for the hub's tailnet
name, which needs HTTPS enabled on the tailnet) or from a file, and does a passkey then
register and sign in from the owner's Mac and phone? Phase [[PHS-20261126-f3e51h]] item 18
waits on it.

## Hypothesis and predictions
- H1: tsnet gives the hub a certificate for its tailnet name (`GetCertificate` through its
  local client), and an axum listener on the tailnet serves TLS with it. If true, a browser
  on a tailnet device opens `https://<hub>.<tailnet>.ts.net` without a warning; if false,
  the certificate request fails or the browser warns.
- H2: a passkey registers and signs in on that origin with webauthn-rs, from Safari on the
  Mac and from the phone's browser, the RP ID being the tailnet name. If false, the
  browser refuses the origin or the RP ID.
- H3: a certificate given by file (for a name the owner controls) works the same; the
  fallback for a tailnet without HTTPS.

## Plan
1. A spike in `spikes/`: a tsnet node with a page of two buttons (register, sign in) over
   webauthn-rs, served with the tailnet's certificate.
2. Run it on the owner's tailnet with a throwaway hostname; open it from the Mac and the
   phone; register and sign in; record what each browser does.
3. The same with a certificate from a file.
4. Remove the test machine from the tailnet afterwards.

## Success criteria
A passkey registers and signs in from both devices over one of the two origins without a
certificate warning. If neither works, ADR-0082's sign-in is revisited before item 18.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.

## Conclusion
Written when status becomes `concluded`; an agent's draft waits for a human to approve it.
