---
id: AGS-20261111-htctc0
type: agent_session
schema_version: 1
title: Write the saarthi Nix modules in the repo as their source of truth, and rename manthan's repos entry
model: claude-opus-5-5
harness: claude-code
started: 2026-11-11T16:13+00:00
finished: 2026-11-11T16:14+00:00
created_entities: []
edited_entities: [PRJ-0001]
outcome: done             # done | failed | aborted | pending
created: 2026-11-11
author: agent:claude
---

## Task
The owner asked for the saarthi Nix modules to be written in this repo, which becomes
their source of truth, with neusis rewritten on them afterwards; and for manthan's
`[repos]` entry to be renamed.

## What happened
- Ported neusis' `rc2s2-hub.nix` and `rc2s2-node.nix` to `nix/hub-module.nix` and
  `nix/node-module.nix`, exported as `nixosModules.saarthi-hub` and
  `homeModules.saarthi-node`, options under `services.saarthi-*`, packages defaulting to
  this flake's. Added the hub's `noNotify` and `extraArgs`; the node puts `saarthi` on
  PATH. Fleet wiring (agenix secrets, `/srv` volume, ntfy, the `claude` wrapper) stays in
  neusis.
- `checks.modules` (`nix/module-tests.nix`) ports the neusis tests: the hub in a real
  NixOS evaluation, the node on Darwin and Linux against stubs of the home-manager options
  it sets. Checked that a wrong expectation fails it.
- ADR-0041 supersedes ADR-0012; architecture and the guide's hub and node setup pages show
  the modules.
- manthan: `[repos]` key `rc2s2` became `saarthi` (url `github:leoank/saarthi`, same local
  path), and the namespace links in PRJ-0002 (`continued_in`, `code`, one link) and
  AGS-20261102-cw6h33 follow; its validate passes. Uncommitted there, beside the owner's
  own change to `notes/tasks.md`.
- Decided without asking: the `services.*` namespace; stubs rather than a home-manager
  input for the node test; neusis not touched.
