---
id: EXP-20261106-qt94y4
type: experiment
schema_version: 1
title: Does a headless stream-json claude -p session ask a PermissionRequest hook given only through --settings, and which flags start it in bypass mode?
project: PRJ-0001
status: concluded           # planned | active | paused | concluded | abandoned
outcome: positive         # null | positive | negative | inconclusive | infra
influenced_by: []
created: 2026-11-06
author: agent:claude
protocols: []
materials: []
tags: []
review: pending
---

## Question
A session the owner spawned on rogue failed every tool that needed approval, and nothing
reached the app: rogue's `~/.claude/settings.json` had lost rc2s2's hook block. Can the
spawner give its sessions the PermissionRequest hook itself, through `--settings`, and
does that double up when the user's settings carry the same hook? Which flags start a
headless session in bypass mode, for the owner's request to spawn with
`--dangerously-skip-permissions`?

## Hypothesis and predictions
Without a hook, `claude -p` refuses a tool that needs approval at once. A PermissionRequest
hook from `--settings` is asked like one from the settings file ([[EXP-20261016-rq1405]]
found the hook fires in `-p` mode); identical hooks from two sources are asked once. If the
`--settings` hook is not asked, the spawner needs the SDK's `can_use_tool` callback instead.
`--dangerously-skip-permissions` and `--permission-mode bypassPermissions` both bypass, and
the hook is not asked.

## Plan
1. A local HTTP server stands in for the node's hook endpoint and answers allow or deny.
2. Per case, a fresh directory under `target/live-work-perm-exp` and one stream-json
   `claude -p` (haiku) asked to run one Bash command that writes `probe.txt`: no hook; the
   hook through `--settings` answering allow, then deny; the hook both through
   `--settings` and in the directory's `.claude/settings.json`; the skip flag; the modes
   `bypassPermissions`, `acceptEdits` and `plan`.
3. Record the session's `permissionMode` from `init`, the hook calls, whether the file was
   written, and the result's `permission_denials`.

## Success criteria
The `--settings` hook is asked exactly once per request and its answer decides; both bypass
flags run the command without a hook call.

## Runs
One line per run after its link, what was tried and what it showed; `yajna validate`
rejects a bare link here. Metric runs also land in results.tsv.
- [[RUN-20261106-qtaf57]] eight cases on Claude Code 2.1.318: no hook refused at once; the `--settings` hook asked once and obeyed (twice for a deny, when the model retried), once also with a project copy; both bypass flags ran unasked; plan ran nothing

## Conclusion
Draft by agent:claude, for the owner to approve.

The predictions hold on Claude Code 2.1.318 ([[RUN-20261106-qtaf57]]): with no hook a tool
that needs approval is refused at once; a PermissionRequest hook given only through
`--settings` is asked once per request and its answer decides (a deny was asked twice only
because the model retried); the same hook also in the project's settings is asked once.
`--dangerously-skip-permissions` and `--permission-mode bypassPermissions` both start the
session in `bypassPermissions` and run the tool unasked; `acceptEdits` still asks for Bash;
`plan` runs nothing. The SDK's `can_use_tool` is not needed.

ADR-0037 followed: the spawner passes the node's hooks block as `--settings` to every
session it starts or resumes, and a spawn carries a permission mode. A scratch node and hub
confirmed both by hand.
