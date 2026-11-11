# ADR-0041: The hub and node Nix modules live in this repo as their source of truth

Status: accepted
Date: 2026-11-11

## Context
ADR-0012 put the modules in the project until they moved to neusis, the owner's fleet
config. In practice they were written in neusis (`rc2s2-hub.nix`, `rc2s2-node.nix`), so
every new flag meant a change in two repos, and anyone else running saarthi had to copy
them. After the rename (ADR-0040) the owner asked for the modules to be written here and
for neusis to be rewritten to use them ([[AGS-20261111-htctc0]]).

## Decision
The flake exports `nixosModules.saarthi-hub` (`nix/hub-module.nix`) and
`homeModules.saarthi-node` (`nix/node-module.nix`), configured under
`services.saarthi-hub` and `services.saarthi-node`. Their package defaults to this
flake's build for the host's system, so a consumer needs only the flake input. They are
the neusis modules renamed, with one option per flag, plus the hub's `noNotify` and
`extraArgs`, and the node puts `saarthi` on the user's PATH.

What belongs to one deployment stays with it: secrets (agenix paths), the data volume,
ntfy and the `claude` wrapper under `saarthi run` remain in neusis.

`checks.modules` evaluates both: the hub in a NixOS system, the node, for Darwin and
Linux, against stubs of the home-manager options it sets.

Settled by the agent, for the owner to revisit: the option namespace `services.*`
rather than `neusis.services.*`; stubs instead of a home-manager flake input for the node
test.

Alternatives: keep the modules in neusis (two repos per flag change, nothing to offer
other users); move only the hub module (the node changes more often).

## Consequences
Supersedes ADR-0012. A new flag lands with its module option and test in the same
commit. neusis drops its copies, imports these and sets `services.saarthi-*`; its tests of
the old modules go with them.
