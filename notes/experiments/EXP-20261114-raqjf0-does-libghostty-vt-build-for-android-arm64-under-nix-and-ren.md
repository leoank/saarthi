---
id: EXP-20261114-raqjf0
type: experiment
schema_version: 1
title: Does libghostty-vt build for Android arm64 under Nix and render a node's shell on the phone at a usable speed
project: PRJ-0001
status: concluded            # planned | active | paused | concluded | abandoned
outcome: positive             # null | positive | negative | inconclusive | infra
influenced_by: []
created: 2026-11-14
author: agent:claude
protocols: []
materials: []
tags: [terminal, app, android]
review: pending
---

## Question
ADR-0054 renders a node's shell on the phone with libghostty-vt, Ghostty's terminal state
library, drawing the cells itself. Does it build for Android (arm64-v8a) with the NDK
under Nix, link into the app beside the Rust core, and keep up with a busy shell at a
usable frame rate?

## Hypothesis and predictions
H1: libghostty-vt's C API cross-compiles with Zig for `aarch64-linux-android` against the
NDK's sysroot, and a Compose canvas drawing its cells keeps 60 frames a second while
`yes`, `htop` and a long build's output run. If the build fails or the frame rate drops
below 30, the fallback is an existing Android terminal emulator library (the one Termux
uses) behind the same byte stream; the node side does not change.

## Plan
1. Build libghostty-vt for `aarch64-linux-android` in a Nix derivation from a pinned
   Ghostty commit; record the Zig version and what had to be patched.
2. A throwaway Android app in `spikes/15-libghostty-android/`: JNI to feed bytes and read
   the cell grid, a Compose canvas drawing it, keys from the soft keyboard, plus Esc, Ctrl
   and arrows on a row above it.
3. Feed it a local PTY's output (`/system/bin/sh`), then recorded output from a node:
   `htop`, `vim`, a colourful `nix build` log, `yes` for ten seconds. Measure frames a second
   and input latency.
4. Repeat 3 with the Termux emulator library for comparison.

## Success criteria
- It builds reproducibly under Nix, or the step that fails is named.
- The frame rate and look under the four loads, against the fallback; one is chosen.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.
- [[RUN-20261114-s2jk7j]] the build: Ghostty v1.4.0's libghostty-vt builds for aarch64-linux-android (API 26) under Nix with Zig 0.17.1 and the NDK, unpatched; 2.1 MB stripped, 16 KB pages, needs only libc and libm; a smoke test passes on the host and links for Android
- [[RUN-20261118-f1904n]] the phone: libghostty-vt behind a JNI shim, drawn on a Compose canvas, takes a shell's output in at about 91 MB/s and draws a 10-second flood at 60 fps (worst frame 8.4 ms); Termux's library parses about 0.5 MB/s on the UI thread, 40 to 80 times slower

## Conclusion
Draft by agent:claude, for the owner to approve.

H1 is true. Ghostty v1.4.0's libghostty-vt builds for `aarch64-linux-android` (API 26)
under Nix with Zig 0.17.1 and the NDK, unpatched, 2.1 MB stripped and needing only libc
and libm ([[RUN-20261114-s2jk7j]]). On the owner's Fold4, behind a JNI shim and drawn on a
Compose canvas, it took a shell's output in at about 91 MB/s and drew a 10-second `yes`
flood at 60 fps throughout, worst frame 8.4 ms; Termux's emulator library parses on the
UI thread at about 0.5 MB/s, 40 to 80 times slower, so a busy shell would stall on it
([[RUN-20261118-f1904n]]).

libghostty-vt was chosen, as ADR-0054 set out on this condition, and the app's terminal
draws its cells. Input latency, in the plan, was not measured; 60 fps is the app's
default rate on the phone, not the library's limit.
