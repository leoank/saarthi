---
id: AGS-20261117-drdxrg
type: agent_session
schema_version: 1
title: "the app's new-project spawn, and what learnharness does with the claude CLI"
model: "claude-opus-5-5"
harness: "claude-code"
started: 2026-11-17T11:58+00:00
finished: 2026-11-17T12:51+00:00
created_entities: [EXP-20261117-de4f82, RUN-20261117-de5vqw, RUN-20261117-e42048]
edited_entities: [PHS-20261114-rbv2fs]
outcome: done          # done | failed | aborted | pending
created: 2026-11-17
author: agent:claude
---

## Task
Phase item 4: *New project* in the app, a spawn of `/yajna:project <brief>` in a chosen
notebook (ADR-0053). Midway the owner asked for a subagent to read how the learnharness
repo drives the `claude` CLI and to learn from it.

## What happened
- [[EXP-20261117-de4f82]], [[RUN-20261117-de5vqw]]: spawned as the app would, the command
  runs and reads the notebook, but a `-p` session has no `AskUserQuestion`, so the
  interview comes as plain-text messages and the session waits idle for a prompt.
- Built (ADR-0060, 0.12.0): a *New project* sheet on the notebooks screen (lab notebooks
  under a node's spawn roots; the kind picks yajna's command; brief, model, Claude
  account) that spawns the command and opens the session; back from it returns to the
  notebooks. The core now shows a prompting command (`<command-message>` first) as the
  person's prompt, `/yajna:project <brief>`, not raw markup. Checked on the emulator:
  the sheet, the spawn, the session, the first question, and an answer typed in the
  prompt bar carrying the interview on.
- learnharness (github.com/amosblomqvist/learnharness, read by a subagent): it runs
  `claude -p` with stream-json and gives the session its own `ask_user_question` tool
  from an MCP server, holding the call until the person answers; MCP tool timeouts
  raised, the tool pre-approved. Also: a `message_stop` with `abandoned_blocks` is a
  stream retry, not the end of a turn; `get_context_usage` reports context left.
- Decided without asking: lab notebooks only.
- The owner then asked to adopt the lessons and build the question tool (ADR-0061): the
  node serves `ask_user_question` over MCP on its hook port, each spawned host session
  gets it with its own bearer token, the tool pre-approved, an appended prompt and long
  MCP timeouts; a call is held as a pending question and its answers return as the tool's
  result ([[RUN-20261117-e42048]]: two questions held and answered). After each turn the
  spawner asks `get_context_usage` and records it; the app shows *context N%*. The
  stream-retry lesson needed nothing: turns end on `result`. Nothing committed.
