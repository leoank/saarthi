---
id: EXP-20261106-62st4b
type: experiment
schema_version: 1
title: Does a finished session resumed headless with claude -p --resume over stream-json keep its session id, transcript file and context, and show in the registry under that id
project: PRJ-0001
status: concluded           # planned | active | paused | concluded | abandoned
outcome: positive         # null | positive | negative | inconclusive | infra
influenced_by: [EXP-20261016-rq1405, EXP-20261106-37f79k]
created: 2026-11-06
author: agent:claude
protocols: []
materials: []
tags: []
review: pending
---

## Question
Phase 2 step 2 resumes a finished session from the phone through the spawner. Whether the
app shows the resumed session as the same conversation or a new one depends on what
`claude -p --resume <id>` over stream-json does to the session's identity, which
[[EXP-20261016-rq1405]] checked only for a plain `-p` resume on an older Claude Code.

## Hypothesis and predictions
H1: the resumed process keeps the session id, appends to the same transcript, keeps the
context, and registers under that id. If it forks a new id or file, the hub has to link
the new session to the old one and the app show them as one.

## Plan
1. `notes/runs/RUN-20261106-632b3d/run.sh`: a one-turn session that is told a code word,
   then the same id resumed over stream-json and asked for the word; compare the init's
   session id, the registry entry and the transcript files before and after.

## Success criteria
- The init and the registry carry the original id, the reply has the code word, and the
  original transcript grows with no new file.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.
- [[RUN-20261106-632b3d]] a code word kept across a headless resume on 2.1.318: same id in init and registry, transcript 31 to 41 lines, no new file; H1 holds

## Conclusion
Draft by agent:claude, for the owner to approve.

H1 holds on Claude Code 2.1.318 ([[RUN-20261106-632b3d]]): `claude -p --resume <id>` over
stream-json, run in the session's directory, keeps the session id in its `init` and in the
registry, appends to the same transcript (31 to 41 lines, no new file) and keeps the
context (the code word came back). A resume is the old session running again, so the hub
needs no link between sessions; ADR-0036 resumes finished sessions in place on this. The
resume must run in the session's own directory, where its transcript lives.
