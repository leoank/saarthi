---
id: EXP-20261114-raqkym
type: experiment
schema_version: 1
title: Do the app's bench timers fire on time with the screen off for hours, and do checklist ticks made out of reach arrive once each when back in reach
project: PRJ-0001
status: planned           # planned | active | paused | concluded | abandoned
outcome: null             # null | positive | negative | inconclusive | infra
influenced_by: []
created: 2026-11-14
author: agent:claude
protocols: []
materials: []
tags: [bench, app, android, yajna]
review: pending
---

## Question
At the bench the phone keeps a run sheet's timers and records its ticks (ADR-0053, yajna's
phase 2b run sheets and `events.jsonl`). Do timers set by the app fire on time with the
screen off and the app in the background for hours, through Doze? And do ticks made while
the phone cannot reach the hub arrive in the run's `events.jsonl` exactly once each, in
order, when it can again?

## Hypothesis and predictions
H1: exact alarms (`setExactAndAllowWhileIdle`) with a foreground service during a bench
session fire within 2 s of their time even after 3 h in Doze; if they drift or are
dropped, timers move into a foreground service that holds a wake lock for the session.
H2: ticks queued on the phone with an id each, sent through the relay when in reach and
written by yajna's service only if the id is new, land once each; if a retry duplicates
one, the service must deduplicate by the id (a requirement on yajna's write API).

## Plan
Runs after yajna's run sheets and `events.jsonl` exist (its phase 2b) and the relay
(ADR-0052).
1. A run sheet with timers of 1, 10, 60 and 180 minutes, started with the screen off and
   the phone left alone; record each firing's delay.
2. Tick ten steps in airplane mode, reconnect, and kill the app once mid-send; compare
   `events.jsonl` with the ticks.

## Success criteria
- Firing delays under the threshold for every timer, or the mechanism that meets it.
- Every tick once, in order, after the reconnect and the kill.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.

## Conclusion
Written once, by a human, when status becomes `concluded`.
