---
id: RUN-20261117-de5vqw
type: run
schema_version: 1
kind: compute               # bench | compute | assembly | calibration
project: PRJ-0001
experiment: EXP-20261117-de4f82
protocols: []
status: done              # planned | in_progress | done | aborted
created: 2026-11-17
author: agent:claude
commit: 82914b3
script: notes/runs/RUN-20261117-de5vqw/run.sh
---

## Notes
Run on rogue on 2026-11-17 by hand, step by step as `run.sh` now does it: saarthi 0.11.1
(hub and node), Claude Code 2.1.323 with the yajna plugin from home-manager, Haiku, the
node's own login, so the session ran in the default permission mode and asked to run four
read-only `Bash` commands, each allowed through the hub.

What happened (`conversation.txt`, the assistant's side):
- `/yajna:project` ran: it loaded the `yajna` skill and read the notebook (`yajna tree`,
  tasks, log, references) before asking anything, as the command says.
- It looked for `AskUserQuestion` (`ToolSearch select:AskUserQuestion`) and did not find
  it: a `-p` session has no such tool, as [[EXP-20261016-rq1404]] found. It said so and
  asked in plain text instead, one topic with three numbered options and a recommendation,
  then ended its turn; the session went idle, waiting for a prompt.
- No question was held at the hub; nothing was written to the notebook.

So *New project* works as a spawn whose interview is a conversation: the person answers
in the prompt bar, not with the question cards. Getting cards would need a question tool
the spawned session can call (for instance one the node serves to it), or an interactive
session under a PTY.

The minutes between steps were the allow loop and Claude Code's start (an MCP server it
launches); the turn itself was quick. What the script cannot control: the model's
wording, and Claude Code's tool set in `-p`, which may change.

## Reproduce
Run `script` from the repo root in the project's dev shell at `commit`; it must produce the
same outputs. Note here what it cannot control.

## Checklist
- [x] the command ran in a spawned session
- [x] how its interview arrived: plain text, session idle 
