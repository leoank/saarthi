---
id: EXP-20261118-298ryc
type: experiment
schema_version: 1
title: Do set_model, an effort change and list_models work in a claude -p stream-json session?
project: PRJ-0001
status: active            # planned | active | paused | concluded | abandoned
outcome: null             # null | positive | negative | inconclusive | infra
influenced_by: []
created: 2026-11-18
author: agent:claude
protocols: []
materials: []
tags: []
review: pending
---

## Question
Can a session the node spawns (`claude -p`, stream-json) have its model and reasoning
effort switched from the app, and can the app read both back from the transcript? Phase
[[PHS-20261117-f22kxe]] item 18 rests on it.

## Hypothesis and predictions
H1: the SDK's `set_model` control request switches a running `-p` session's model, an
effort change has a control request too, and `list_models` gives the catalog. If true, the
next reply's `message.model` is the new model and the transcript records the effort; if
false, the chip is read-only for spawned sessions and switching needs a restart.

## Plan
1. Find the control requests Claude Code 2.1.326 understands (its binary's strings):
   `set_model`, `list_models`, `apply_flag_settings` (whose settings take `effortLevel`).
2. Script one Haiku session: a turn, `list_models`, `set_model sonnet`, effort `low`, a turn;
   read the transcript for the model and effort of each reply.

## Success criteria
Both changes answered `success`, the second reply from the new model, and the transcript
saying so.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.

- [[RUN-20261118-29b3r2]] `list_models`, `set_model` and `apply_flag_settings` with `effortLevel` all work in a `-p` session; each reply's transcript line has its model and `effort`.
- [[RUN-20261118-29b3r2]] compute run, not yet described

## Conclusion
Written once, by a human, when status becomes `concluded`.
