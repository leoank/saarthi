---
id: EXP-20261025-35gght
type: experiment
schema_version: 1
title: Does the PermissionRequest hook fire for AskUserQuestion in a bypass-permissions session, and can PreToolUse hold and answer it instead?
project: PRJ-0001
status: concluded            # planned | active | paused | concluded | abandoned
outcome: negative             # null | positive | negative | inconclusive | infra
influenced_by: [EXP-20261016-rq1404, EXP-20261025-25f4bf]
created: 2026-10-25
author: agent:claude
protocols: []
materials: []
tags: [questions, bypass, hooks, phase-1.5]
review: pending
---

## Question
The owner asked a session under `rc2s2 run --dangerously-skip-permissions` to ask a question;
AskUserQuestion's dialog appeared at the terminal and nothing reached the phone. The node
holds questions at the PermissionRequest hook (ADR-0013, [[EXP-20261016-rq1404]]). In
bypass-permissions mode Claude Code skips permission checks, so that hook may never fire
for the tool while the dialog still shows, since a question has no permission to bypass.
[[EXP-20261016-rq1404]] showed PreToolUse can answer the question too, with
`permissionDecision: allow` and `updatedInput`. Which hooks fire for AskUserQuestion in
bypass mode, and can a PreToolUse hold carry the question to the phone and the answer back
the way the PermissionRequest hold does, including the keystroke release at the terminal?

## Hypothesis and predictions
H1: in bypass mode PreToolUse fires for AskUserQuestion with the questions in `tool_input`
and PermissionRequest does not, which is why the phone saw nothing. H2: holding PreToolUse
and returning `permissionDecision: allow` with `updatedInput` carrying `answers` resolves the
question with the answer from the phone, no dialog shown; returning without a decision
lets the dialog show, as a keystroke release needs. If H1 is false (PermissionRequest fires
in bypass mode too) the fault is elsewhere, in the live node or hub of that day. If H2 is
false the question must be shown as a notice only, with the answer typed at the terminal.

## Plan
1. A scratch hub and node as in [[RUN-20261025-2aymst]], one haiku session under the
   wrapper with `--dangerously-skip-permissions` and hooks through `--settings`; ask for a
   question with two options; from the hub's record list the hook kinds for the tool call
   and whether a pending request was opened; answer at the terminal.
2. With the node holding AskUserQuestion at PreToolUse when the session's permission mode
   is bypass: the same question, answered from `rc2s2 ctl answer`; the tool result and the
   reply show the answer; no dialog at the terminal. Then a keystroke at the terminal while
   held: the hold releases and the dialog shows.

## Success criteria
- Step 1 names the hooks that fire in bypass mode; a pending request is or is not opened.
- Step 2: the answer sent from the client reaches the model as the tool result, and a
  keystroke releases the hold to the terminal's dialog.

## Runs
- [[RUN-20261025-36bknd]] a question in a bypass-permissions session: PreToolUse and PermissionRequest both fire, the node holds it; H1 is false, the missed question was the node not forwarding that evening

## Findings
- In a `--dangerously-skip-permissions` session the PermissionRequest hook fires for
  AskUserQuestion as in any other mode, after PreToolUse, and the node opens a question
  request from it; the hold needs no PreToolUse path ([[RUN-20261025-36bknd]]).
- The owner's question that never reached the phone fell in a window when the node was not
  forwarding: before the hook block was installed, or while the daemon crash-looped after
  the first rogue switch with the old lock ([[RUN-20261025-36bknd]], [[AGS-20261025-2vf58e]]).
  Step 2 of the plan is moot unless it recurs.

## Conclusion
Draft by agent:claude, for the owner to approve.

H1 is false ([[RUN-20261025-36bknd]]): in a `--dangerously-skip-permissions` session the
PermissionRequest hook fires for AskUserQuestion after PreToolUse, as in any other mode,
and the node opens a question request from it. The owner's missed question fell in a window
when the node was not forwarding (hooks not yet installed, or the daemon crash-looping after
the first rogue switch, [[AGS-20261025-2vf58e]]). No PreToolUse path is needed, so H2 (step
2) was not tested and is moot unless the miss recurs; phase 1.5 step 9 closed with no code
change to the hold.
