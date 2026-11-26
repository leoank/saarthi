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
- [ ] [[PHS-20261114-rbv2fs]] Notebooks, a terminal and voice: yajna through a service relay, a node terminal, push to talk
- [ ] [[PHS-20261117-f22kxe]] The app at a glance: lessons from ovis's design and UX
- [ ] [[PHS-20261118-kbka9b]] Workspaces and the blue flame: the app as a home for agents, notebooks, terminals and nitya

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
- [[EXP-20261016-rq1408]] Can a Claude Code credential be leased into a fresh config dir, and does a refresh rotate it; yes: the file is used and the keychain ignored, but a refresh rotates and revokes the old pair, so each concurrent store needs its own login (ADR-0018, later ADR-0039)
- [[EXP-20261016-rq1409]] GPU metrics on macOS without a vendor API; positive, IOKit PerformanceStatistics without sudo
- [[EXP-20261017-rwda0a]] does the observe-only node daemon see what a real Claude Code session does; positive within its scope
- [[EXP-20261019-ayya0b]] does the hub record every node event exactly once across hub and node restarts; positive, with line_id dedup
- [[EXP-20261019-ayya0c]] does deduplicating transcript lines by uuid at the hub scale; positive, adopted
- [[EXP-20261019-cqpw0d]] Do held permission requests resolve from a client against real Claude Code; yes in -p turns: allow runs the tool, deny stops it and the model quotes the message, an ignored request times out to a refusal (ADR-0013)
- [[EXP-20261019-fgm60e]] Does intervention work in an interactive session under rc2s2 run; yes: question answers, keystroke release (Enter, Escape, 2), socket prompts, interrupt with follow-up and kill all land, though interrupts and terminal refusals fire no Stop
- [[EXP-20261021-a2ty0f]] Does the phone app intervene in a real Claude Code session; yes on the emulator: answer, deny with message, prompt, interrupt with follow-up and kill all landed, in the first app and the rebuilt one
- [[EXP-20261025-25f4bf]] Does a prompt typed into the wrapper's PTY arrive as a plain user turn and queue when busy; yes to both, and it is not held in bypass mode, so ADR-0027 needs no wait-for-Stop fallback
- [[EXP-20261025-25f6vm]] Does the usage endpoint answer with the keychain token read only, without disturbing the login; yes, with no prompt and the login intact across a refresh, but it returns 429 to once-a-minute polls, so the poller runs every five minutes and backs off (ADR-0030)
- [[EXP-20261025-25f8mm]] Do turn_duration lines and summed usage match /cost; one turn_duration per completed turn (within 40 ms of the hooks), none for an interrupted one; totals match the transcript, while /cost adds tokens of interrupted requests
- [[EXP-20261025-35gght]] Does PermissionRequest fire for AskUserQuestion in a bypass-permissions session; it does, so the hypothesis failed and no PreToolUse hold is needed; the missed question was the node not forwarding that evening
- [[EXP-20261106-37f79k]] A spawned stream-json claude -p is in the registry at once (status stuck busy, so idle/busy come from the stream), stops a turn on an interrupt control request and exits 0 on closed stdin; positive (ADR-0034)
- [[EXP-20261106-62st4b]] A headless claude -p --resume over stream-json keeps the session id, transcript file and context, and registers under the same id; positive, a resume is the old session running again (ADR-0036)
- [[EXP-20261106-qt94y4]] A PermissionRequest hook given only through --settings is asked once per request and decides, even with a project copy, and both the skip flag and --permission-mode bypassPermissions run unasked; positive (ADR-0037)
- [[EXP-20261107-413q6m]] A claude setup-token token alone, with an empty config dir, runs turns and resumes, reports rate limits and leaves the keychain and the owner's login alone; positive (ADR-0038, ADR-0039)
- [[EXP-20261107-413q82]] A stream-json session spawned through mxc seatbelt keeps stdin control, gets hooks over a unix socket, runs nix, git and SSH, refuses keys and home, and resumes from its own config dir; positive, though a kill leaves the tool's process group running (ADR-0038)
- [[EXP-20261110-g5mh7g]] One setup-token shared by three sessions on rogue for four hours: 72 turns, none failed; active, the second machine's half still to run
- [[EXP-20261110-g5mht5]] GET /api/oauth/usage refuses a setup-token (403, it lacks the user:profile scope); negative, so an account's windows come from the rate_limit_events of sessions under its token (ADR-0042)
- [[EXP-20261110-g5mh2z]] Can one claude setup-token token be revoked on its own, where, and how fast does a running session holding it stop; planned, for ADR-0039
- [[EXP-20261110-g5mh89]] A seatbelt-sandboxed claude reaches a token-adding proxy on the node through ANTHROPIC_UNIX_SOCKET with an http:// base URL, holding only a dummy, with no TLS interception; positive, but only /v1/messages goes through it (ADR-0050)
- [[EXP-20261111-q6sgt8]] With CLAUDE_CODE_OAUTH_TOKEN set in the default config dir, claude auth status moves from the keychain's claude.ai to oauth_token; positive, though a second account's windows were never seen to move
- [[EXP-20261112-rq8kgc]] tonic gRPC over libtailscale's listen and dial, two identities in one process, peers named from status_json, no fds leaking into children; positive on darwin and aarch64-linux, the first dial after a keyless restart needing one retry (ADR-0048)
- [[EXP-20261113-1h4ztf]] libtailscale cross-built for arm64 Android under Nix, carrying the app's gRPC to a hub with no Tailscale app; positive once built as a c-shared library with a getifaddrs netmon getter and TS_LOGS_DIR set from Go (ADR-0049)
- [[EXP-20261114-d82rcz]] Every hub-token session reaching the Claude API through the hub; positive: the TUI works with a dummy token, a hub away up to 90 s costs one retry, a request is the whole context (0.6 MB median) (ADR-0051)
- [[EXP-20261114-raqjjy]] A streamed request from the app to a node's unix-socket service through the hub, in a WebView; positive: pages, ranges and events arrive whole, events in one round trip, and an EventSource bridge over a WebMessagePort replaced shouldInterceptRequest, which buffers (ADR-0052, ADR-0056)
- [[EXP-20261114-raqjf0]] libghostty-vt built for Android arm64 under Nix, drawing a shell on the phone; positive: builds unpatched, takes output in at 91 MB/s and draws a flood at 60 fps where Termux's library manages 0.5 MB/s (ADR-0054)
- [[EXP-20261114-raqk9b]] Which speech recognition transcribes short coding and lab commands well enough, on the phone or on the node, in a quiet room and at the bench; planned, for ADR-0055
- [[EXP-20261114-raqkym]] Do the app's bench timers fire on time with the screen off for hours, and do checklist ticks made out of reach arrive once each when back in reach; planned, for ADR-0053
- [[EXP-20261117-de4f82]] /yajna:project spawned from the app; positive: it runs, its interview first as plain-text messages since -p lacks AskUserQuestion (ADR-0060), then as held questions through the node's question tool (ADR-0061)
- [[EXP-20261118-298ryc]] set_model, an effort change and list_models in a claude -p stream-json session; positive: all work, effort through apply_flag_settings, each reply's transcript line records model and effort (phase item 18)
- [[EXP-20261121-3k3cxv]] Does the notifications stream the app holds to the hub (ADR-0075) survive Doze and network changes, how fast, at what battery cost; on the emulator it came through forced deep idle in 14 s and back by itself after airplane mode, the phone's day still to run
- [[EXP-20261123-j9wyqh]] libghostty-vt's VT formatter restoring a shell's screen in a fresh terminal; positive: screen, scrollback and cursor restore exactly, and the alternate screen too once the holder keeps the primary screen from when a program entered it (ADR-0078)
- [[EXP-20261124-1pg2cq]] Under the node's proxy, Claude Code sends only `/v1/messages` through it; its account, settings and start-up calls go straight to Anthropic with the dummy token, so letting `/api/` paths through the proxy (or faking them there) would change nothing; and a hub token is refused the profile (scope) and managed settings (404), answered only on policy limits, which carry no default model: a hub-token session gets Claude Code's default model unless saarthi names one
- [[EXP-20261124-gfp268]] `ANTHROPIC_MODEL` sets a hub-token session's main model in `-p` mode and at a terminal, below `--model` and above a `model` in settings: `saarthi run` can give an account's default model that way
