# ADR-0047: A terminal dashboard, not a terminal app

Status: accepted
Date: 2026-11-12

## Context
`saarthi ctl` (ADR-0021) does everything the app does, one command at a time, for scripts
and for trying things. The owner asked whether it should become a proper terminal UI,
taking those features of the app that make sense in a terminal. The app's centre is a
session's conversation; in a terminal the person has Claude Code itself for that, so a
second conversation view would duplicate it. What a terminal lacks is the overview: which
sessions run where, what waits for a decision, how the accounts stand. The owner chose the
dashboard over a dashboard with conversations.

## Decision
- `saarthi tui`, beside `ctl` in the same binary, with the same hub and token
  (`ctl.toml`, `--hub`, `--token-file`). `ctl` stays as it is, for scripts.
- One screen: nodes with their live sessions (foldable), the held requests and questions,
  and a line of the accounts' 5-hour and 7-day usage and token expiry, kept current from
  the control service's `Watch` stream and reloaded on every (re)connect.
- Keys act on what is selected: allow, deny with a message, answer a question (single and
  multi-select), prompt, interrupt and end a session, start one (node, directory, prompt,
  model, permission mode, account).
- ratatui on crossterm, with the state kept apart from drawing and I/O so keys and updates
  are unit-tested and the screen is checked on a test backend.

## Alternatives considered
- A conversation view too: about twice the work, and the terminal already has Claude Code.
- Growing `ctl` into the UI (no subcommand opens it): `ctl`'s output is for scripts; a
  separate subcommand keeps it plain.
- Launch profiles, history, resume and expiry editing on the dashboard: rarer; `ctl` and
  the app have them, and they can come later.

## Consequences
- A person at a terminal answers requests and watches every node without the phone.
- The dashboard follows the control service's API; a new client feature needs a key and a
  line here as well as a screen in the app.
- ratatui and its dependencies (about forty crates) join the node binary.
