---
id: EXP-20261123-j9wyqh
type: experiment
schema_version: 1
title: Does libghostty-vt's VT formatter restore a shell's screen, scrollback and alternate screen in a fresh terminal, on the machine's own platform?
project: PRJ-0001
status: concluded            # planned | active | paused | concluded | abandoned
outcome: positive           # null | positive | negative | inconclusive | infra
influenced_by: []
created: 2026-11-23
author: agent:claude
protocols: []
materials: []
tags: []
review: pending
---

## Question
ADR-0078 has a per-user holder keep each shell's screen in libghostty-vt and, on attach,
send it as VT sequences from libghostty-vt's formatter, so a fresh terminal on the client
shows what the shell shows. Does that hold, built for the machine's own platform (the
holder runs on macOS and Linux, where the app's build is for Android)? Three cases: a
plain shell with long output (the scrollback), a styled screen with the cursor mid-line,
and a full-screen program in the alternate screen, then leaving it.

## Hypothesis and predictions
H1: the formatter's VT output, with its extras on (cursor, style, modes, scrolling
region), fed to a new terminal of the same size, gives the same plain text, the same
cursor and the same active screen.
- If H1 holds: the plain text of both terminals is equal for the primary screen with its
  scrollback, the cursor row and column match, and in the alternate-screen case the alt
  screen matches.
- If the formatter writes the active screen only, leaving the alternate screen shows the
  restored terminal without the primary screen's contents: then the holder has to write
  the primary screen first and the alternate one after `CSI ?1049h`, or keep a mark.
- If it fails outright (no host build, the output does not restore), ADR-0078 falls back
  to replaying a bounded byte log.

## Plan
1. Build libghostty-vt for the host from the flake (as the Android one is, without the
   Android target) and a small C probe against it.
2. The probe feeds a terminal 80x24 with 10,000 lines cap: 12,000 numbered lines, then a
   styled prompt with the cursor mid-line; formats VT; feeds a second terminal; compares
   plain text (whole screen and scrollback) and cursor.
3. Then enters the alternate screen, draws a box and moves the cursor, formats, restores
   into a third terminal, compares the alt screen; then sends `CSI ?1049l` to both and
   compares the primary screens.
4. Records sizes: formatter output bytes and the time to format.

## Success criteria
Primary screen and scrollback equal, cursor equal, alternate screen equal: H1 holds and
the holder sends one formatted screen. A difference only on leaving the alternate screen
is a partial success that names the extra step the holder needs. Anything else, the
fallback.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.

- [[RUN-20261123-mqgb4x]] the probe on aarch64-darwin: screen, 10,000-line scrollback and cursor restored exactly from one formatted snapshot; the alternate screen too, once the holder keeps the primary screen as it was when a program entered the alternate one (the formatter reaches the active screen only); the library also builds for x86_64-linux

## Conclusion
Draft by agent:claude, for the owner to approve.

H1 holds for the primary screen and needs one extra step for the alternate one
([[RUN-20261123-mqgb4x]]). On aarch64-darwin one formatted snapshot (314 KB of VT in about
3 ms) restored the screen, a 10,000-line scrollback, a styled prompt and the cursor
exactly in a fresh terminal, once the scrollback's byte cap was turned off. The formatter
writes the active screen only, as the second prediction feared, and switching the source
to read the primary screen leaves a mode set on it; so the holder formats and keeps the
primary screen just before a program enters the alternate one, and a snapshot is that
followed by the alternate screen with its modes. With that, the alternate screen and the
return from it restored exactly. This is what ADR-0078's holder does, and the byte-log
fallback is not needed. libghostty-vt also built for x86_64-linux, but the probe ran only
on the Mac.
