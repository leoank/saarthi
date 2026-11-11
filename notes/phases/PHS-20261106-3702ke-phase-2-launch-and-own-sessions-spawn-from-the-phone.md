---
id: PHS-20261106-3702ke
type: phase
schema_version: 1
title: "Phase 2: launch and own sessions"
project: PRJ-0001
status: active
since: a1296bf15083131872845ddf305e2c9337a90051
until: null               # the commit it finished at: yajna phase done
created: 2026-11-06
author: agent:claude
tags: []
---

## Goal
Start, kill, resume and look back on sessions from the phone: spawn a headless session on a node in a chosen directory, kill any session, resume a finished one by id, browse past sessions with their events and cost, and run them under named tokens the hub holds (`docs/architecture.md`, Phase 2). Built in the order of ADR-0034: spawn and kill first under the node's own login, then resume and history, named tokens last.

## Progress
One item per piece of work, ticked when it lands; add items as the plan grows and keep the
done ones. The project page's *Progress* lists this phase on one line.
- [ ] 1. Spawn and kill under the node's own login, and the keep-awake switch (ADR-0034;
  [[EXP-20261106-37f79k]] checked the stream-json session it rests on)
  - [x] 1.1 Node, hub and `rc2s2 ctl`: `--spawn-root` and `--claude`, the spawner owning
    stdin and stdout (prompt, interrupt, kill, idle and busy from the stream), kill for
    bare sessions, `SetKeepAwake`; spawn roots with the node; unit and end-to-end tests,
    and checked against a scratch hub with the real Claude Code 2.1.318: spawn, interrupt,
    prompt, kill (an interrupt first, or a busy session outlives its closed input until
    the grace kill), a bare `claude -p` killed by SIGTERM, keep-awake on and off with
    `caffeinate` held during a busy turn (2026-11-06; [[AGS-20261106-37f739]])
  - [x] 1.2 The app: a new session from the home screen (node, directory, prompt, model),
    kill in every session's menu, the keep-awake switch on the node's health; checked on
    the emulator against the scratch hub: started on Haiku and opened at once, prompted,
    killed, keep-awake switched on (2026-11-06; [[AGS-20261106-37f739]])
  - [x] 1.3 The neusis node module: `spawnRoots` and the claude path (neusis cdef11d,
    rogue under `~/workspace`, claude from the login shell); rogue switched and ghost001's
    hub deployed by the owner (2026-11-06; [[AGS-20261106-37f739]])
  - [x] 1.4 From the owner's first use: the first prompt optional (the session starts
    idle) and a missing directory created under the root on request, on by default in the
    app (ADR-0035); unit-tested and checked live against a scratch hub; takes effect on
    rogue and ghost001 with the next bump (2026-11-06; [[AGS-20261106-5q9jas]])
  - [x] 1.5 From the owner's use: a spawned session's permission requests failed at once
    when the user's settings had lost the hooks; the node now gives every session it
    starts its hooks with `--settings`, and a spawn picks a permission mode, Bypass
    included (ADR-0037, [[EXP-20261106-qt94y4]]); unit and end-to-end tests, checked live
    (an approval from `ctl` ran the tool, Bypass ran it unasked); takes effect with the
    next bump (2026-11-06; [[AGS-20261106-qt8y8v]])
- [x] 2. Resume a finished session by id, from the phone; session history: past sessions
  per node with their events and cost. One History screen titled by first prompts with
  tokens and cost, resume in place of any finished session under a spawn root, from its
  screen or `rc2s2 ctl resume` (ADR-0036, the owner's choices; [[EXP-20261106-62st4b]]
  showed a headless resume keeps the id and transcript). Unit and end-to-end tests;
  checked against a scratch hub with the real Claude Code (history, a code word kept
  across two resumes) and on the emulator (History, resume from an ended session's
  screen); takes effect on rogue and ghost001 with the next bump (2026-11-06;
  [[AGS-20261106-62ss4t]])
- [ ] 3. Named `claude setup-token` tokens the hub holds, chosen per session, with a
  default per node (ADR-0039, superseding the leased slots of ADR-0018, so
  [[EXP-20261016-rq1408]]'s refresh run no longer gates it)
  - [ ] 3.1 Check the assumptions first: one token used from two machines at once
    ([[EXP-20261110-g5mh7g]]), the usage endpoint with a token ([[EXP-20261110-g5mht5]]),
    revoking one token alone ([[EXP-20261110-g5mh2z]])
    - [x] 3.1.1 The usage endpoint refuses a setup-token; any turn under it streams the
      account's windows, so usage per account comes from sessions (ADR-0042;
      [[RUN-20261111-q60gf1]], [[RUN-20261111-q6afjv]]); and a token in the environment
      wins over the keychain login on the host ([[EXP-20261111-q6sgt8]]) (2026-11-11;
      [[AGS-20261111-q5z55t]])
    - [ ] 3.1.2 One token shared by several sessions for hours: rogue's half running
      ([[RUN-20261111-q7np8h]]); a second machine's half to come
    - [ ] 3.1.3 Revoking one token alone: needs the owner to revoke a throwaway token
  - [x] 3.2 The hub reads `--claude-tokens` and keeps each name's fingerprint and dates;
    a default token per node; a spawn or resume names a token or takes the default, and
    the node takes it from the command; `rc2s2 ctl` and the app list tokens, pick one on
    *New session* and set a node's default. Also each account's 5-hour and 7-day usage
    from the sessions under its token, with alerts per account (ADR-0042). Unit and
    end-to-end tests; checked live against a scratch hub with the real Claude Code (the
    default token applied, the account's windows recorded on it, the own login picked,
    an unknown token refused, no token in the logs); the APK builds (2026-11-11;
    [[AGS-20261111-q5z55t]])
  - [x] 3.3 Expiry: a year after first seen or set by hand from the app or `ctl`; a
    reminder a month before, notification kind `token` (2026-11-11;
    [[AGS-20261111-q5z55t]])
  - [ ] 3.4 neusis: the hub module provides the token file as an agenix secret. The
    module's `claudeTokensFile` option is in this repo (ADR-0041); the secret and its
    wiring in neusis are the owner's
  - [ ] 3.5 The egress proxy that adds the token, so a sandbox holds a dummy (phase 3
    with the sandbox's network policy; [[EXP-20261110-g5mh89]] decides its transport)

## Notes
Migrated from PRJ-0001's Progress by `yajna phase migrate`.
