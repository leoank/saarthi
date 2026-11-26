---
id: EXP-20261025-25f8mm
type: experiment
schema_version: 1
title: Do the transcript's turn_duration lines and the summed usage match what Claude Code's /cost reports?
project: PRJ-0001
status: concluded            # planned | active | paused | concluded | abandoned
outcome: positive             # null | positive | negative | inconclusive | infra
influenced_by: [EXP-20261016-rq1402]
created: 2026-10-25
author: agent:claude
protocols: []
materials: []
tags: [usage, turn-time, cost, phase-1.5]
review: pending
---

## Question
ADR-0030 shows each turn's time and tokens from two transcript facts: the `turn_duration`
system line Claude Code writes after a turn (`durationMs`), and the `usage` events the
daemon records once per assistant message id ([[EXP-20261016-rq1402]]). The session total
is their sum. Unchecked: that there is exactly one `turn_duration` per turn (including an
interrupted one, which fires no Stop hook), that it brackets the turn the person sees, and
that the summed tokens match what Claude Code itself reports with `/cost` (and, for a
subscription, how `/cost` phrases it).

## Hypothesis and predictions
H1: every completed turn has one `turn_duration` line, written at its end, and an
interrupted turn has none or one with the partial time. H2: the sum of the daemon's `usage`
totals by model over a session equals the input, output and cache token counts `/cost`
prints, within the tokens of the `/cost` turn itself. If H2 is false the difference shows
which messages the daemon double counts or misses (subagent transcripts are a candidate,
[[EXP-20261016-rq1402]]).

## Plan
1. A haiku session under the wrapper in a scratch directory: five turns of different sizes,
   one tool call, one interrupted turn, one subagent (Agent tool); then `/cost` at the
   terminal.
2. From the hub's record of this session only: the `turn_duration` lines against the
   UserPromptSubmit and Stop hook times; the summed `usage` by model against the `/cost`
   output captured from the PTY.

## Success criteria
- One `turn_duration` per completed turn, within a second of Stop minus UserPromptSubmit.
- Summed tokens by model equal `/cost`'s, or the difference is explained by a named line.

## Runs
- [[RUN-20261025-2g2812]] six turns with a tool call, an interrupt and a subagent, then /cost: one turn_duration per completed turn within 40 ms of the hooks; the daemon's totals equal the transcript's, and /cost is higher by the interrupted request

## Findings
- Every completed turn writes one `turn_duration` line, within 40 ms of Stop minus
  UserPromptSubmit; an interrupted turn writes none and fires no Stop, so it shows no time
  ([[RUN-20261025-2g2812]]).
- The daemon's usage totals, once per message id with subagent transcripts included, equal
  a hand count of the transcript; a message with thinking and text is two lines with the
  same usage, so the id rule is the right one ([[RUN-20261025-2g2812]]).
- `/cost` counts more: the tokens of requests the transcript never records, such as an
  interrupted stream. The app's numbers are the transcript's; the difference is noted, not
  recoverable from the transcript ([[RUN-20261025-2g2812]]).

## Conclusion
Draft by agent:claude, for the owner to approve.

H1 holds ([[RUN-20261025-2g2812]]): every completed turn writes one `turn_duration` line,
within 40 ms of Stop minus UserPromptSubmit; an interrupted turn writes none and fires no
Stop, so it shows no time. H2 holds against the transcript but not against `/cost`: the
daemon's totals, once per message id with subagent transcripts included, equal a hand count
of the transcript, and the id rule is right since a message with thinking and text is two
lines with one usage. `/cost` is higher by the tokens of requests the transcript never
records, here the interrupted stream; that difference is named, not recoverable. The app's
numbers under ADR-0030 (phase 1.5 step 5) are the transcript's, with that gap noted.
