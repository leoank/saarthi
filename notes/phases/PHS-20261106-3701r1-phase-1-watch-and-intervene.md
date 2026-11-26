---
id: PHS-20261106-3701r1
type: phase
schema_version: 1
title: "Phase 1: watch and intervene"
project: PRJ-0001
status: done
since: null               # the commit it started at: yajna phase start
until: ee5d51d2194fc88348830a101db6d4835869b99c
created: 2026-11-06
author: agent:claude
tags: []
---

## Goal
Phase 1: watch and intervene

## Progress
One item per piece of work, ticked when it lands; add items as the plan grows and keep the
done ones. The project page's *Progress* lists this phase on one line.
- [x] 1. Cargo workspace and `crates/proto`: event, node and control protobuf packages,
  one bidirectional `Link` stream per node (2026-10-17; [[manthan/AGS-20261017-p8g0qe]])
- [x] 2. Node daemon, observe only: hook endpoint, transcript tailer with usage once per
  message id, session registry, health with the macOS GPU (2026-10-17; [[EXP-20261017-rwda0a]])
- [x] 3. Hub, record only: tonic and SQLite, node registration, reconnect with a buffer,
  events stored once by seq and by transcript line id (2026-10-19; [[EXP-20261019-ayya0b]],
  [[EXP-20261019-ayya0c]], [[manthan/AGS-20261019-aycgtw]])
- [x] 4. Intervention: permission hold and release, AskUserQuestion answers, prompts,
  interrupt and kill (2026-10-19)
  - [x] 4.1 Hold and resolve: the node's hold, the hub's `pending` table and control
    service, `rc2s2 ctl`; allow, deny and fall-through checked live in `-p`
    (2026-10-19; [[EXP-20261019-cqpw0d]], [[manthan/AGS-20261019-bsxr8w]])
  - [x] 4.2 Prompts over the session's messaging socket, with a note when a bypass-mode
    session will hold them at its terminal; live check in 4.4 (2026-10-19)
  - [x] 4.3 The `rc2s2 run` wrapper: Claude under a PTY mirrored to the terminal,
    interrupt (Escape, Ctrl-U, follow-up prompt) and kill from a client, a keystroke
    releasing a held request, terminal replies not counted as typing; live check in 4.4
    (2026-10-19)
  - [x] 4.4 Live checks in an interactive session under the wrapper: AskUserQuestion
    answered from a client, keystroke release, socket prompt, interrupt mid-turn, kill
    (2026-10-19; [[EXP-20261019-fgm60e]])
- [x] 5. Client core (UniFFI) and the Android app; ntfy push. The Android toolchain comes
  from the project flake with the SDK licence accepted; development runs on an emulator
  on the owner's Mac; the hub pushes to a configurable ntfy topic, a local ntfy in
  development (owner's choices, 2026-10-19)
  - [x] 5.1 Hub: a session's event stream (stored, then live), health history, and ntfy
    notifications for held requests and finished turns (2026-10-19)
  - [x] 5.2 `crates/core`: the client in Rust with UniFFI 0.32 bindings (lists, watch,
    session stream as conversation items, health, allow, deny, answers, prompt, interrupt,
    kill), tested against a real hub; Kotlin bindings generate (2026-10-19)
  - [x] 5.3 Android toolchain in the project flake: `nix develop .#android` with SDK 35, NDK
    29, an arm64 emulator (Android 15 boots headless in 32 s), cargo-ndk, Gradle and a
    rust-overlay toolchain; the core cross-builds for arm64 Android (2026-10-19)
  - [x] 5.4 The Compose app over the core (`android/`): connect, nodes and sessions, pending
    requests with allow, deny and answers, the live conversation with prompt, interrupt
    and kill, health charts; smoke-tested on the emulator against a scratch hub and node
    with synthetic data, allow resolving a held request end to end (2026-10-19)
  - [x] 5.5 Push on the phone through UnifiedPush with the ntfy app as distributor
    (ADR-0026): the hub posts to registered endpoints, the app notifies with Allow and
    Deny; delivered and allowed from the notification on the emulator (2026-10-19)
  - [x] 5.6 Live checks on the emulator against a hub and a node on the Mac: question,
    deny, prompt, interrupt and kill from the app in a real wrapped session (2026-10-19;
    [[EXP-20261021-a2ty0f]])
- [x] 6. Nix modules: hub (NixOS), node (home-manager, launchd and systemd). Authored in
  the owner's neusis repo; the hub and a self-hosted ntfy run on ghost001, reachable over
  the tailnet, with pre-shared tokens as agenix secrets (2026-10-25; [[AGS-20261024-ta4ae5]])
- [x] 7. The app made usable: Material 3 screens with navigation, auto-connect and
  reconnect, requests as cards with dialogs, the conversation as chat that follows,
  health charts over a chosen span, a launcher icon; the core shows socket prompts as
  typed. Checked on the emulator against a real wrapped session (2026-10-25;
  [[RUN-20261025-166zqd]], [[AGS-20261025-0t3kjw]])
- [x] 8. The session screen like the Claude Code terminal, loosely: prompts as boxed
  lines, Claude's text as markdown, tool calls as one line with the output folded to a
  line count until tapped, thinking folded; the core parses every transcript shape (slash
  and `!` commands, task notices, meta text, images, errors) so nothing shows raw
  (2026-10-25; [[AGS-20261025-1mm7j6]])
- Phase 1 is done when, from the phone:
  - [x] every node and every live session is listed, with the conversation streaming live
  - [x] a permission dialog can be allowed or denied, and falls through to the terminal
    when ignored
  - [x] a question from Claude can be answered
  - [x] a session under the wrapper can be prompted and interrupted
  - [x] a notification arrives when a session blocks or finishes
  - [x] health shows as a chart per node
  Each checked on the emulator in items 5.6 and 7 and used daily from the phone since
  phase 1.5 (ticked 2026-11-26; [[AGS-20261126-f3e4hp]])

## Notes
Migrated from PRJ-0001's Progress by `yajna phase migrate`.
