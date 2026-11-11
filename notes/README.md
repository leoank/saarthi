---
id: PRJ-0001
origin: manthan/PRJ-0002
type: project
schema_version: 1
title: saarthi - Guiding agents to victory
status: active            # active | paused | concluded | archived
code: .
created: 2026-10-16
author: ank
tags: []
review: pending
---

## Introduction
There are a lot of existing ways to drive agents remotely.
We do not want to reinvent that, what we are looking for is a highly custom and integrated command and control center.
This should involve writing a server that is aware of all the running agentic session. It should be able to intercept and intervene.
The server should maintain records of all the runs and node health metrics.
We also need small wrappers for claude, antigravity cli, pi and other agent harness to implement the server features.
There should be multiple clients like: android, iOS, web and custom integrations into another app.

## Progress
The phases of the project's `docs/architecture.md`, one `PHS` page each with its own
checklist; phase 1's steps follow `docs/HANDOFF.md` section 4. Each ticked item says what
landed and links the pages with the detail.
- [x] [[PHS-20261106-3701mw]] Phase 0: spikes
- [ ] [[PHS-20261106-3701r1]] Phase 1: watch and intervene
- [ ] [[PHS-20261106-3701nh]] Phase 1.5: fit for daily use
- [ ] [[PHS-20261106-3702ke]] Phase 2: launch and own sessions
- [ ] [[PHS-20261106-3702q3]] Phase 3: confinement and many networks
- [ ] [[PHS-20261106-37024q]] Phase 4: autonomous starts and more clients

## Open questions
- Idle sessions were reported as missing from the app until the owner typed in them
  (2026-10-25). By the time it was looked at they were listed; the daemon scans the
  registry every 2 s, so the cause is unknown. Noted only; reopen if it comes back, with
  the exact observation (session start time against the node's, the hub's session row).
- Should a finished turn push only for sessions touched from the phone (prompted,
  interrupted, or under the wrapper)? Decided after a while of use (ADR-0029).
- Claude Code 2.1.302 skips HTTP hooks for SessionStart ("HTTP hooks are not supported for
  SessionStart" in its debug log), so the daemon first hears of a session from its registry
  entry or its first other hook. Does anything in the daemon depend on SessionStart arriving?
- Remaining after phase 0: see the end of `docs/architecture.md`.
- Rebrand to **saarthi**, "Guiding agents to victory" (the owner, 2026-11-06): done in
  this repo on 2026-11-11 (ADR-0040, [[AGS-20261111-ffkhjn]]): binaries, crates, protocol,
  app id and label, default paths, living docs, the notebook's namespace, and the code moved
  to `github:leoank/saarthi`. Left to the owner: the neusis modules and their option names,
  the flake input there, and the links to this project in `manthan`. Since then the modules
  live here (ADR-0041, [[AGS-20261111-htctc0]]) and manthan links by `saarthi`; neusis is
  to be rewritten on them.

## Design
The architecture is `docs/architecture.md`
and its decision records are in `docs/decisions/` beside it. What a reader must know before
touching the code:
- A daemon per machine observes every Claude Code session through hooks, the transcript
  and Claude Code's session registry, and holds one gRPC stream to the hub; clients talk
  only to the hub, through a separate control service.
- Permission dialogs are held in the PermissionRequest hook and released by a client's
  decision, a keystroke at a wrapped terminal, or a timeout; prompts go over Claude Code's
  messaging socket; interrupt and kill need the `saarthi run` wrapper.
- The hub's SQLite record stores each event once, by node and seq and by transcript line
  id, so restarts on either side neither lose nor duplicate anything.

## Lessons
- The published `libtailscale-sys` crate runs `go build` and downloads modules, which the
  Nix sandbox forbids; a hand-written sys crate replaces it ([[EXP-20261016-rq1406]]).
- An idle timer to decide when a held dialog goes back to the terminal is not needed: a
  keystroke there is queued until the hook returns, so the wrapper releases on the
  keystroke itself ([[EXP-20261016-rq1401]], [[EXP-20261016-rq1403]]).
- An MCP `--permission-prompt-tool` for headless sessions is not needed: PermissionRequest
  fires in `-p` mode ([[EXP-20261016-rq1405]]).
- Terminal output cannot be parsed for state: the TUI positions text with cursor moves
  ([[EXP-20261016-rq1403]]). Nor can the workspace trust dialog be driven through the PTY; sessions
  start in trusted directories ([[EXP-20261016-rq1401]]).
- Hooks passed with `--settings` never see SessionStart; the daemon's hooks belong in the
  settings file ([[EXP-20261016-rq1401]]).
- A Claude credential must never be copied into a second store: the first refresh revokes
  the other copy, the owner's own login included ([[EXP-20261016-rq1408]]). A
  `claude setup-token` token does not refresh, so it can be shared; sessions run under
  named ones the hub holds instead of leased slots (ADR-0039).
- `touch` inside Claude Code's working directory needs no approval, so it cannot test
  permission handling; use a path outside it ([[EXP-20261019-cqpw0d]]).
- The node's hook block went into the owner's `~/.claude/settings.json` on 2026-10-25
  (`rc2s2 node hooks` merged over the existing keys); it is the manual step the node module
  leaves out, and the hub on ghost001 holds no requests without it ([[AGS-20261025-0t3kjw]]).
- The usage endpoint (`api/oauth/usage`) rate-limits its callers: after about 30 calls in
  30 minutes it answered 429 to every call for half an hour; poll it every few minutes at
  most and back off on a 429 ([[RUN-20261025-c5xjbh]]).

## Experiments
- [[EXP-20261016-rq1401]] hold a PermissionRequest in a hook and answer it from another process; positive, works
- [[EXP-20261016-rq1402]] transcript JSONL shapes and where usage and rate-limit state live; positive, mapped
- [[EXP-20261016-rq1403]] PTY injection for prompt, interrupt and keyboard-activity detection; positive, works with caveats
- [[EXP-20261016-rq1404]] answering AskUserQuestion from a hook; positive, multi-select and free text covered
- [[EXP-20261016-rq1405]] headless multi-turn input and the permission callback; positive, resumable
- [[EXP-20261016-rq1406]] libtailscale as a static archive under Nix linked into Rust; positive, builds on both platforms
- [[EXP-20261016-rq1407]] mxc seatbelt sandbox around claude under Nix; positive, policy settled
- [[EXP-20261016-rq1408]] leasing a Claude Code credential into a fresh config dir; active, refresh run pending
- [[EXP-20261016-rq1409]] GPU metrics on macOS without a vendor API; positive, IOKit PerformanceStatistics without sudo
- [[EXP-20261017-rwda0a]] does the observe-only node daemon see what a real Claude Code session does; positive within its scope
- [[EXP-20261019-ayya0b]] does the hub record every node event exactly once across hub and node restarts; positive, with line_id dedup
- [[EXP-20261019-ayya0c]] does deduplicating transcript lines by uuid at the hub scale; positive, adopted
- [[EXP-20261019-cqpw0d]] do held permission requests resolve from a client against real Claude Code; active, allow, deny and fall-through work in -p
- [[EXP-20261019-fgm60e]] does intervention work in an interactive session under rc2s2 run; active, question, keystroke release, socket prompt, interrupt and kill all work
- [[EXP-20261021-a2ty0f]] does the phone app intervene in a real Claude Code session; active, question, deny, prompt, interrupt and kill from the app all land
- [[EXP-20261025-25f4bf]] does a prompt typed into the wrapper's PTY arrive as a plain user turn, and does one typed during a running turn queue rather than interrupt; active, yes to both, queued without interrupting
- [[EXP-20261025-25f6vm]] does the usage endpoint answer with the access token read from the keychain, read only, without disturbing the login; active, yes across a refresh, but it refuses polls once a minute (429)
- [[EXP-20261025-25f8mm]] do the transcript's turn_duration lines and the summed usage match what Claude Code's /cost reports; active, counts match the transcript, /cost adds interrupted requests
- [[EXP-20261025-35gght]] does the PermissionRequest hook fire for AskUserQuestion in a bypass-permissions session, and can PreToolUse hold and answer it instead; planned
- [[EXP-20261106-37f79k]] Does a headless stream-json claude -p session started by the daemon show in the session registry, stop its turn on an interrupt control request over stdin, and end cleanly when stdin closes
- [[EXP-20261106-62st4b]] Does a finished session resumed headless with claude -p --resume over stream-json keep its session id, transcript file and context, and show in the registry under that id
- [[EXP-20261106-qt94y4]] Does a PermissionRequest hook given only through --settings reach a headless session, and which flags bypass: yes, asked once even when also in a settings file; --dangerously-skip-permissions or --permission-mode bypassPermissions (ADR-0037)
- [[EXP-20261107-413q6m]] Does a `claude setup-token` token alone run, resume and report rate limits without the keychain or the owner's login; active, yes to all
- [[EXP-20261107-413q82]] Does a stream-json session through mxc seatbelt keep stdin control, hooks over a unix socket, nix, git and SSH inside, and resume; active, yes, but a kill leaves the Bash tool's process group running
- [[EXP-20261107-413q6m]] Does a long-lived token from claude setup-token, given only as CLAUDE_CODE_OAUTH_TOKEN with an empty config dir, run headless stream-json turns, resume, and report rate limits, without touching the keychain or the owner login
- [[EXP-20261107-413q82]] Does a headless stream-json claude session spawned through mxc seatbelt keep stdin control (prompt, interrupt), deliver hooks over a unix socket, reach the nix daemon, ssh agent and git identity, and leave its registry entry and transcript in the sandbox config dir for resume
- [[EXP-20261110-g5mh7g]] Does one claude setup-token token, used by several headless sessions at once on two machines, keep working through hours of turns, with no session or the owner's login disturbed; planned, for ADR-0039
- [[EXP-20261110-g5mht5]] Does GET /api/oauth/usage, the endpoint behind --claude-usage, accept a claude setup-token token, and if not what other source gives an idle account's usage; planned, for ADR-0039
- [[EXP-20261110-g5mh2z]] Can one claude setup-token token be revoked on its own, where, and how fast does a running session holding it stop; planned, for ADR-0039
- [[EXP-20261110-g5mh89]] Can a seatbelt-sandboxed claude reach an egress proxy on the node that adds a setup-token as its credential, with a dummy token inside and no TLS interception; planned, for ADR-0039
