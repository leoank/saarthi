---
id: AGS-20261123-mqc38r
type: agent_session
schema_version: 1
title: "Shells that persist: a per-user shell holder, reattach with the screen restored, several clients, the app reconnecting (ADR-0078)"
model: claude-opus-5-5
harness: claude-code
started: 2026-11-23T18:52+00:00
finished: 2026-11-23T19:56+00:00
created_entities: [EXP-20261123-j9wyqh, RUN-20261123-mqgb4x]
edited_entities: [PHS-20261114-rbv2fs]
outcome: done             # done | failed | aborted | pending
created: 2026-11-23
author: agent:claude
---

## Task
The owner found the app's shells unstable and lost on every disconnect, asked whether SSH
would do better (pointing at koh and a "tunnel shell"), and wanted shells that persist
until killed. They chose: survive node restarts, several clients mirrored as zmx does,
10,000 lines of scrollback.

## What happened
- Argued against SSH: persistence lives on the machine, not in the protocol; recorded as
  [[ADR-0078]], amending ADR-0054.
- [[EXP-20261123-j9wyqh]] with [[RUN-20261123-mqgb4x]]: libghostty-vt's VT formatter,
  built for the machine from the flake, restores screen, scrollback and alternate screen
  in a fresh terminal. Found on the way: the scrollback's byte cap must be off, and the
  formatter reaches only the active screen, so the holder keeps the primary screen when a
  program enters the alternate one (switching the source with mode 47 changed it: rejected).
  Proposed conclusion: H1 holds with that step. The probe ran on aarch64-darwin only; the
  library builds for x86_64-linux.
- Item 13: `saarthi shells serve`, a C shim over libghostty-vt linked statically, the node
  attaching through it, the module's `saarthi-shells` unit from the home profile so an
  upgrade does not restart it. Decided without asking: the holder answers no terminal
  queries (the clients do), a client falling 1,024 messages behind is dropped to reattach.
- Item 14: shell ids, `ListShells`, `KillShell` through the hub; the core tells ended from
  dropped. Item 15: Machines lists held shells; the app reattaches with backoff and holds
  typing.
- Another session's uncommitted work sat in `crates/node/src/{main,run}.rs`,
  `tests/run.rs` and `notes/README.md`; it was left alone, and `main.rs` committed hunk by
  hunk. Each commit was checked on exactly what was staged.

