---
id: AGS-20261112-rpj0rk
type: agent_session
schema_version: 1
title: "Embedded tsnet in hub and node: decisions, de-risking experiment, implementation"
model: claude-opus-5-5
harness: claude-code
started: 2026-11-12T22:29+00:00
finished: null
created_entities: [EXP-20261112-rq8kgc, RUN-20261112-s5yqfc, RUN-20261112-s5z9yk]
edited_entities: [PHS-20261106-3702q3]
outcome: pending          # done | failed | aborted | pending
created: 2026-11-12
author: agent:claude
---

## Task
Start phase 3's networking: embed tsnet in the hub and the node, before the sandbox egress
proxy (the owner's order, 2026-11-12). Settle the decisions, de-risk the parts nobody has
run, then build it.

## What happened
- The owner's choices, asked in two rounds: "tsnet proxy" means embedded tsnet in hub and
  node first, then the sandbox egress proxy ([[EXP-20261110-g5mh89]]); a first join by an
  auth key file (agenix in neusis), the state dir carrying the identity after; the plain
  TCP path kept beside tsnet, tsnet opt-in by config; the hub on several tailnets from
  the start; the peer's tailnet identity recorded, tokens stay the authorisation.
- Checked libtailscale upstream: `ac7ed43` (2026-10-30) marks handed-out descriptors
  close-on-exec, which matters for a node that spawns `claude`; pinned it. Same Go
  modules as `d1dc45d` (identical vendorHash). Decided myself: the C API is declared by
  hand as in the spike, and the peer is named from `tailscale_status_json`, not the
  LocalAPI over `tailscale_loopback`.
- [[EXP-20261112-rq8kgc]] planned with `spikes/11-tsnet-grpc/`; it builds on
  aarch64-darwin. The remote builders spirit and oppy did not answer, so the linux build
  waits for ghost001.
- The experiment's runs, with the owner's key in a mode-600 file in the scratchpad:
  [[RUN-20261112-s5yqfc]] on this Mac and [[RUN-20261112-s5z9yk]] on ghost001 (built
  there; it is aarch64-linux). Every check passed on both. The surprise: after a start
  from saved state `tailscale_up` returns before the netmap names the peers, so the first
  dial can say `no such host`. The recorded results leave out tailnet addresses and the
  tailnet's DNS name, since the repo is public.
- ADR-0048 written, then built: crate `saarthi-tailnet`, the hub's `--tailnet`, the
  node's `--tailnet`, migration 0007 and `Node.tailnet`/`tailnet_peer`, `ctl nodes`, the
  flake's libtailscale, the modules' options and their tests, the guide and the
  architecture. Decided myself: a tailnet that cannot be joined is logged and left out
  (ADR-0044's rule); the node's identity starts in the background and the link reports
  `not on the tailnet yet` until it is up; an `ephemeral` option exists for the live
  test only; the hub's state defaults to `tailnet/` beside the database.
- Checked: the workspace tests, the live test against the owner's tailnet, the module
  tests, and the real binaries: a hub on the tailnet, a node joining with the key and
  again without it, `ctl nodes` showing `tailnet home: saarthi-try (leoank@github)`.
- Versions to 0.4.0 (app versionCode 4), per the owner's rule.
- For the owner: the spike and test machines left on the tailnet (`saarthi-spike-*`,
  `saarthi-spike-linux-*`, `saarthi-try-hub`, `saarthi-try`) to remove, and whether to
  revoke the key; neusis (2.3) is theirs.
