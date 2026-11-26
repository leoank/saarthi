---
id: EXP-20261117-de4f82
type: experiment
schema_version: 1
title: Does /yajna:project run in a session spawned from the app, and how does its interview reach the person?
project: PRJ-0001
status: concluded            # planned | active | paused | concluded | abandoned
outcome: positive             # null | positive | negative | inconclusive | infra
influenced_by: []
created: 2026-11-17
author: agent:claude
protocols: []
materials: []
tags: []
review: pending
---

## Question
ADR-0053 makes *New project* in the app a spawn of `/yajna:project <brief>` in a chosen
notebook, "whose interview arrives as questions the app already answers". Does the plugin
command run in a headless session the node spawns, and does its interview reach the person
as held `AskUserQuestion` questions?

## Hypothesis and predictions
H1: the command runs (the node spawns `claude` as the login shell finds it, with the
plugin), and its interview is held as questions. If the command is missing we expect an
unknown-command reply; if `AskUserQuestion` is not offered in `-p` (as
[[EXP-20261016-rq1404]] found for an older Claude Code) we expect the interview in plain
text, the session idle after each question.

## Plan
1. A scratch lab notebook, a hub and a node on loopback; spawn
   `/yajna:project <brief>` there on Haiku through `saarthi ctl spawn`, as the app would.
2. Allow its read-only commands as they are held; watch for a held question or the
   session going idle; read its conversation.

## Success criteria
The command runs and its first interview question reaches the hub in a form the app can
answer: a held question (H1), or a turn the person answers with a prompt.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.

- [[RUN-20261117-de5vqw]] spawned on loopback: the command runs and reads the notebook first,
  but `AskUserQuestion` is not in a `-p` session's tools, so it asks in plain text, one
  topic at a time with numbered options, and waits idle for a prompt
- [[RUN-20261117-e42048]] the same spawn with the node's question tool (ADR-0061): the interview's
  questions are held at the hub as question cards and their answers come back as the
  tool's result, the turn going on

## Conclusion
Draft by agent:claude, for the owner to approve.

H1 is half true. The command runs in a session the node spawns and reads the notebook
first, but `AskUserQuestion` is not among a `-p` session's tools, so the interview came in
plain text, one topic at a time with numbered options, the session idle for a prompt after
each ([[RUN-20261117-de5vqw]]). That reaches the person as messages answered from the
prompt bar, which ADR-0060 took for *New project*. With the node's own question tool
(ADR-0061) the same spawn's questions were held at the hub as question cards and the
answers came back as the tool's result, the turn going on ([[RUN-20261117-e42048]]), so
spawned sessions now ask that way. Both runs were on loopback with Haiku; the interview was
not followed to a created project.
