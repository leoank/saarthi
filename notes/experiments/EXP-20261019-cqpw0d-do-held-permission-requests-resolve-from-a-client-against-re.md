---
id: EXP-20261019-cqpw0d
previous_id: manthan/EXP-0013
type: experiment
schema_version: 1
title: Do held permission requests resolve from a client against real Claude Code?
project: PRJ-0001
status: concluded            # planned | active | paused | concluded | abandoned
outcome: positive             # null | positive | negative | inconclusive | infra
influenced_by: [EXP-20261016-rq1401, EXP-20261019-ayya0b]
created: 2026-10-19
author: agent:claude
protocols: []
materials: []
tags: [intervention, phase-1, node, hub, permissions]
review: pending
---

## Question
Step 4 of phase 1 makes the node hold Claude Code's PermissionRequest hook, report it to
the hub as pending, and answer it with whatever a client decides through the hub's control
service; when nobody decides, the hold runs out and the dialog is left to Claude. The
integration tests drive the hook endpoint with synthetic inputs. Does the same chain hold
against a real Claude Code turn: does an allow from a client run the tool, does a deny
stop it and reach the model with its message, and does an ignored request fall through?

## Hypothesis and predictions
[[EXP-20261016-rq1401]] measured the allow shape and the hold in `-p` mode, so an allow from
`rc2s2 ctl` should run the tool. Deny was never tried: if Claude Code takes
`{"behavior": "deny", "message": ...}` as documented, the tool does not run and the model
sees the message, so its reply reflects it. An ignored request answered `{}` after the
hold should behave as if no hook were configured, which in `-p` mode without a permission
tool means the tool is refused. If not, the turn shows a different shape: the tool runs
despite a deny, the message never reaches the model, or the fall-through hangs.

## Plan
1. Run a scratch hub and a node linked to it on this machine (spare ports, generated
   tokens, a temporary database), the node reading live state read only, its hold set to
   a few seconds. Three throwaway `claude -p` turns on haiku in a scratch directory, each
   asked to create one file with `touch`, hooks pointed at the node through `--settings`
   from `rc2s2 node hooks`, `CLAUDE*` and `AI_AGENT` stripped. A watcher resolves the
   first turn's request with `rc2s2 ctl allow`, the second with `rc2s2 ctl deny` and a
   message, and leaves the third alone.
2. Later, with the wrapper: AskUserQuestion answered from a client in an interactive
   session (the tool does not exist in `-p` mode), and the keystroke release.

## Success criteria
- Each turn's PermissionRequest reaches the hub as a pending request for Bash with the
  `touch` command.
- Allow: the file exists, and the request closes as resolved.
- Deny: the file does not exist, the request closes as resolved, and Claude's reply shows
  it got the message.
- Ignored: the request closes on timeout after the hold, and the turn ends without the
  file.

## Runs
- [[RUN-20261019-c958hw]] allow, deny and fall-through from three `claude -p` turns (plan 1)

## Findings
- Against real Claude Code, a request held by the node reaches a client through the hub
  and the client's decision is applied: allow ran the tool 334 ms after the hold began,
  deny stopped it in 264 ms ([[RUN-20261019-c958hw]]).
- The deny shape `{"behavior": "deny", "message": ...}` works: Claude's reply quoted the
  message exactly, so the model sees it.
- An ignored request closes on timeout after the hold and `{}` leaves the decision to
  Claude Code; in `-p` mode that is a refusal.
- Claude Code runs `touch` inside its working directory without asking, so a check of
  permission handling needs a command or path that always asks.

## Conclusion
Draft by agent:claude, for the owner to approve.

Yes, against real Claude Code in `-p` turns ([[RUN-20261019-c958hw]]). Each turn's
PermissionRequest reached the hub as a pending Bash request; an allow from `rc2s2 ctl` ran
the tool 334 ms after the hold began; a deny with a message stopped it in 264 ms and Claude
quoted the message, so the documented deny shape reaches the model; an ignored request
closed on timeout and `{}` left the decision to Claude Code, a refusal in `-p` mode. The
hold-and-resolve chain of phase 1 step 4.1 stands as built (ADR-0013). Aside: Claude Code
runs `touch` inside its working directory without asking, so a permission check needs a
command or path that always asks. Plan step 2 (questions, keystroke release) went to
[[EXP-20261019-fgm60e]].
