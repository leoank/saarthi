---
id: AGS-20261124-gf6jge
type: agent_session
schema_version: 1
title: A default model per hub Claude account, so sessions under a hub token start on the model the organization uses
model: claude-opus-5-5
harness: claude-code
started: 2026-11-24T14:59+00:00
finished: null
created_entities: [EXP-20261124-gfp268, RUN-20261124-gftzzx]
edited_entities: [PRJ-0001]
outcome: pending          # done | failed | aborted | pending
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
