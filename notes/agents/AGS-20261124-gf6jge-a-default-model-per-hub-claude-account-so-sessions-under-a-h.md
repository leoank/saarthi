---
id: AGS-20261124-gf6jge
type: agent_session
schema_version: 1
title: A default model per hub Claude account, so sessions under a hub token start on the model the organization uses
model: claude-opus-5-5
harness: claude-code
started: 2026-11-24T14:59+00:00
finished: 2026-11-24T15:23+00:00
created_entities: [EXP-20261124-gfp268, RUN-20261124-gftzzx]
edited_entities: [PRJ-0001, PHS-20261106-3702ke]
outcome: done             # done | failed | aborted | pending
created: 2026-11-24
author: agent:claude
---

## Task
Sessions under a hub token start on Claude Code's own default model (Sonnet), not the
organization's (Opus): the hub cannot fetch managed settings with its tokens
([[EXP-20261124-1pg2cq]]). The owner chose a default model per hub account, set in the
app, used by spawns and resumes without a model and by `saarthi run`.

## What happened
- Before designing, [[EXP-20261124-gfp268]] checked the one untested assumption: that
  `ANTHROPIC_MODEL` sets the model under a hub token, below `--model`
  ([[RUN-20261124-gftzzx]]: yes, in both modes, and above a `model` in settings).
- Decided without asking: the model lives on the hub's token record (a column of
  `claude_tokens`), kept when a token is renewed; it is checked as the node checks a
  `/model`; `saarthi run` does not set `ANTHROPIC_MODEL` over one the environment already
  has; `ctl token-model` sets it too, as `ctl token-expiry` does the expiry. ADR-0079.
- Built: `SetClaudeTokenModel` and `ClaudeToken.default_model` on the control service,
  `ClaudeTokenReply.model` on the node service; the hub fills a spawn's or resume's empty
  model from the token's; the daemon passes it to `saarthi run`, which sets
  `ANTHROPIC_MODEL`; a Default model row on each card of the app's Claude accounts.
- Checked: the hub's token tests (spawn, picked model, other account, own login, resume,
  and the terminal reply) and the wrapper's (the variable set, and not over one already
  set); on the emulator against a scratch hub with two dummy tokens, Opus picked for one
  shows in the app and in `ctl tokens`.
