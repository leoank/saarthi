---
id: RUN-20261117-e42048
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
script: notes/runs/RUN-20261117-e42048/run.sh
---

## Notes
Run on rogue on 2026-11-17 by hand, step by step as `run.sh` does it, with the uncommitted
node of ADR-0061 on top of 82914b3: the node's question tool given to the spawned session.
Haiku, the node's own login, Claude Code 2.1.323.

What happened (`conversation.txt`):
- After two read-only commands the interview's first question was held at the hub as a
  pending question (kind question, tool `AskUserQuestion`'s shape), four options, through
  `mcp__saarthi__ask_user_question`: no plain-text question, no `ToolSearch` for
  `AskUserQuestion`.
- Answered as the app answers (`saarthi ctl answer`), the answer came back as the tool's
  result ("The person answered: ... Best steep time, one tea") and the same turn went on
  to the second question, held the same way ("Why", four options).
- A separate one-line spawn showed the context report after its turn: a daemon `context`
  event with 29,131 of 1,000,000 tokens (3%).

Not controlled: the model's wording and its choice of the tool, which the appended prompt
asks for; a question held longer than the few seconds here was not tried.

## Reproduce
Run `script` from the repo root in the project's dev shell at `commit`; it must produce the
same outputs. Note here what it cannot control.

## Checklist
- [x] the first question held at the hub
- [x] its answer returned as the tool's result, the turn going on
- [x] a context report after a turn 
