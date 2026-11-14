# ADR-0054: A terminal on a node through the node

Status: accepted
Date: 2026-11-14

## Context
The owner wants a terminal on any of their machines from the phone, rendered with Ghostty's
library ([[AGS-20261114-rapbh2]]). An SSH client on the phone would need keys there and a
route to each machine. The node already runs programs under PTYs for `saarthi run` and has
an authenticated link to the hub that the phone reaches.

## Decision
- **The node opens the shell.** A control call, `Terminal`, a bidirectional stream: the
  node and the window size, then keystrokes and resizes up, and the PTY's output down. The
  node starts the owner's login shell under a PTY in their home directory and ends it when
  the stream closes. It goes over a node-service stream the node dials back, as in
  ADR-0052.
- **Off unless enabled.** A node offers it only with `--terminal` (and the module's
  option); a shell from the phone is the owner's whole account on that machine. Host
  spawns already allow as much, but a shell is the first thing an attacker with a client
  token would want.
- **Rendering.** libghostty-vt holds the terminal's state and the app draws its cells, if
  [[EXP-20261114-raqjf0]] shows it builds for Android and keeps up; otherwise the terminal
  emulator library Termux uses. The byte stream is the same either way.
- A dropped stream ends the shell; keeping it for a reconnect (or running it in tmux) is
  left for later.

## Alternatives considered
- SSH from the phone: keys on the phone, a route to every machine, and a second way in.
- A web terminal (ttyd) behind the service relay in a WebView: no native rendering or
  keyboard handling, which is what the owner asked for.
- Mosh: survives roaming, but needs UDP to every machine.

## Consequences
- With `--terminal`, a stolen client token is a shell on every such node; client tokens are
  already that powerful through spawns.
- The hub sees every keystroke and every byte of output in plain text.
