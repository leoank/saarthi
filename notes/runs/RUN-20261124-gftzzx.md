---
id: RUN-20261124-gftzzx
type: run
schema_version: 1
kind: compute               # bench | compute | assembly | calibration
project: PRJ-0001
experiment: EXP-20261124-gfp268
protocols: []
status: done              # planned | in_progress | done | aborted
created: 2026-11-24
author: agent:claude
script: notes/runs/RUN-20261124-gftzzx/run.sh          # executable that re-creates this run, relative to the repo root
---

## Notes
Run here (macOS, Claude Code 2.1.330's bare binary) on 2026-11-24; nothing left the
machine. The proxy's socket logged the `model` of each `POST /v1/messages`:

| case | models asked for |
|---|---|
| nothing set | `claude-sonnet-5-5` ×2 |
| `ANTHROPIC_MODEL=opus` | `claude-opus-5-5` ×2 |
| that, plus `--model sonnet` | `claude-sonnet-5-5` ×2 |
| `settings.json` `model: haiku` | `claude-haiku-5-5` ×2 |
| that, plus `ANTHROPIC_MODEL=opus` | `claude-opus-5-5` ×2 |
| `ANTHROPIC_MODEL=opus`, at a terminal | `claude-opus-5-5` ×2, `claude-haiku-4-5-20251001` ×1 |

The two requests per case are the turn and its retry after the 401. The terminal's Haiku
request is Claude Code's small, fast model, which `ANTHROPIC_MODEL` does not touch.

Differed from the plan: a first attempt used `model: sonnet` in settings, which cannot be
told from the default, and the terminal stopped at onboarding, whose connectivity check
the refusing proxy fails; the script now uses `haiku` and writes a `.claude.json` with
onboarding done and the directory trusted, as on a machine already in use. The log is
`probe.log` beside the script.

## Reproduce
Run `script` from the repo root in the project's dev shell at `commit`; it must produce the
same outputs. Note here what it cannot control.

## Checklist
- [ ] 
