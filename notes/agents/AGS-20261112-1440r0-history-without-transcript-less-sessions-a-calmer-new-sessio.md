---
id: AGS-20261112-1440r0
type: agent_session
schema_version: 1
title: History without transcript-less sessions, a calmer New session screen, foldable nodes, keep-awake only where it works
model: claude-opus-5-5
harness: claude-code
started: 2026-11-12T01:01+00:00
finished: 2026-11-12T01:31+00:00
created_entities: []
edited_entities: [RUN-20261111-q7np8h, EXP-20261110-g5mh7g]
outcome: done             # done | failed | aborted | pending
created: 2026-11-12
author: agent:claude
---

## Task
The owner, with ghost's hub running again after a Claude tokens name it refused: what the
many `claude-code-cwd-*` sessions in History are and whether to filter them; a better New
session screen ("jarring, not polished"); a way to fold a node's live sessions; and why
the keep-awake switch on ghost says it lets the machine sleep.

## What happened
- `claude-code-cwd-*`: empty Claude Code project directories on rogue (only `memory/`),
  for working directories `$TMPDIR/claude-code-cwd-XXXXXX` made Nov 7 to 10. Neither
  saarthi, the installed Claude Code binary, the workspace nor neusis has the name; what
  made them is unknown. None kept a transcript, so History now lists only sessions with
  at least one transcript line (hub `Db::history`), whatever made them. Unit test fails
  without the filter.
- New session: a full-screen sheet in place of the chip-filled dialog: the prompt on top,
  "Where" (node segments, the directory with recent ones as chips, a create checkbox),
  "Options" as settings rows opening menus (model, permissions with a line each, Claude
  account, launch profile), a red notice for bypass, one full-width Start button. Checked
  on the emulator against a scratch hub.
- Home: a node's header folds its sessions (remembered across launches) and shows the
  count and how many wait. Checked on the emulator.
- Keep-awake holds only on macOS (`caffeinate`); a Linux node now leaves out the
  `node.keep_awake*` metrics, so the app shows no switch for it.
- Recorded [[RUN-20261111-q7np8h]]: 72 turns, none failed, resumes worked.
- The owner then asked for the Linux hold (ADR-0045). On ghost001 logind refused every
  inhibitor to ank ("Access denied"; polkit not running); curiosity was booted into
  Windows, so the real lock is untested. The node takes a blocking `idle:sleep` lock with
  `systemd-inhibit` around `cat`, checks once at start, and reports no keep-awake metrics
  where refused. Unit tests use a stand-in `systemd-inhibit`. The guide shows the polkit
  rule a machine needs; not added to neusis.
