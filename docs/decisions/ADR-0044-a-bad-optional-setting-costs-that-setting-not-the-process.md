# ADR-0044: A bad optional setting costs that setting, not the process

Status: accepted
Date: 2026-11-12

## Context
The node canonicalized every `--spawn-root` at start and exited when one was missing, so a
typo would show at once. A NixOS deployment set `~/workspace` as ghost001's spawn root on a
host where that directory did not exist; the node would have exited at every start and
systemd would have restarted it every five seconds. Its sessions, health and hub link would
all have gone with the one setting. A `--profiles` file that did not parse stopped the node
the same way, and a `--claude-tokens` file that did not parse stopped the hub, taking every
node's record and relay down with it. The owner asked that neither crash.

## Decision
- A missing spawn root is created at start. One that cannot be created, or is not a
  directory, is left out with an error in the log; the node runs with the roots that
  remain, and with none, spawning is off, as without the flag.
- A `--profiles` file that cannot be read or parsed is left out with an error in the log;
  every spawned session runs on the host.
- A `--claude-tokens` file that cannot be read or parsed is left out with an error in the
  log; the hub offers no Claude tokens and sessions run under each node's own login.
- What the process cannot do without stays fatal: the hub's node and client token files and
  database, and binding its port; the node's hub token file and its hook endpoint.

## Alternatives considered
- Skip a missing root without creating it: the node runs, but spawns and usage probes there
  stay refused until someone makes the directory by hand. The root is something the owner
  declared, so making it is what they meant.
- Keep refusing at start and create the directory in the Nix module: fixes the deployments
  the module writes, not a hand-run node, and still crashes on any other bad setting.

## Consequences
- A typo in a spawn root makes a directory instead of an error at start; the log says
  which. The node's spawn roots, as clients see them, show what is in effect.
- A mistake in an optional file shows in the log and in what clients see (no profiles, no
  Claude accounts), not as a service in a restart loop.
