---
id: RUN-20261126-hseepp
type: run
schema_version: 1
kind: compute               # bench | compute | assembly | calibration
project: PRJ-0001
experiment: EXP-20261126-f3e5v3
protocols: []
status: done              # planned | in_progress | done | aborted
created: 2026-11-26
author: agent:claude
script: spikes/17-two-nodes/run-3.sh           # executable that re-creates this run, relative to the repo root
commit: 8637e4a88459b5467d9d63f2ebebceef45d1cb37
host: rogue
outputs:
  - path: notes/runs/RUN-20261126-hseepp/run.log
    sha256: 0dfb7f38e29df193ce7649f662b2de00278339d62115c7dfdedffcf4ec386a9b
---

## Notes
Two nodes in one process, `spikes/17-two-nodes/src/main.rs`: each assembled from
`saarthi_node`'s parts as the hub's tests do, with a Claude dir, wrapper socket, shell socket
and hook port of its own, no registry scan, and a stand-in for claude that logs its input and
answers each prompt with a result. A hub of their own on `127.0.0.1:7813`. `run.log` is the run.

1. **Both work side by side.** Both register; each spawns a session, listed under its own node
   only; each process reads exactly its own two prompts (the spawn's and a later `prompt`).
   The hub restarts and both are back within a second.
2. **The tailnet statics.** The same, with node q set to dial through its tailnet
   (`link::dial_via_tailnet()`, as `serve --tailnet` does; no tailnet is joined, so q never
   connects). p registered once, over plain TCP, because its first dial won the race with
   q's setting. After the hub restarts p fails five times in 40 s with q's error, `not on the
   tailnet yet`, and stays offline: the setting is the process's, not q's. Had q joined, p would
   have dialled through q's identity.

H4's prediction that tailnet would be fine was about tsnet, and EXP-20261112-rq8kgc still
holds for it; the collision is saarthi's own two statics in `link.rs`.

Ran before the spike was committed, on files identical to `commit` (8637e4a adds the scripts unchanged; `crates/` is the same at both).

## Reproduce
Run `script` from the repo root in the project's dev shell at `commit`; it must produce the
same outputs. Note here what it cannot control.

## Checklist
- [ ] 
