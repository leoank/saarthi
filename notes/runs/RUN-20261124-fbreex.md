---
id: RUN-20261124-fbreex
type: run
schema_version: 1
kind: compute               # bench | compute | assembly | calibration
project: PRJ-0001
experiment: EXP-20261124-1pg2cq
protocols: []
status: done              # planned | in_progress | done | aborted
created: 2026-11-24
author: agent:claude
script: notes/runs/RUN-20261124-fbreex/run.sh           # executable that re-creates this run, relative to the repo root
---

## Notes
Run on ghost001 by the owner on 2026-11-24, as root, with the invocation in the script's
header; python3 came through nix. Both hub tokens (`mrclmb`, `bodygram`) answered alike:

| path | status | body |
|---|---|---|
| `GET /api/claude_code/settings` | 404 | `not_found_error`, "Not found." |
| `GET /api/claude_code/policy_limits` | 200 | keys `compliance_taints`, `defaults`, `monitoring_notice`, `restrictions` |
| `GET /api/oauth/profile` | 403 | `permission_error`, "OAuth token does not meet scope requirement any_of(user:profile, user:office)" |
| `POST /v1/messages/count_tokens` (control) | 200 | key `input_tokens` |

The profile is closed to a hub token by scope, not by path: a setup-token lacks
`user:profile`. Policy limits are open to it. The settings 404 is the generic one: it does
not say whether the organizations have no managed settings or the path is not served to
this token; it can be told only against an organization that has managed settings set.

## Reproduce
Run `script` from the repo root in the project's dev shell at `commit`; it must produce the
same outputs. Note here what it cannot control.

## Checklist
- [ ] 
