---
id: AGS-20261112-2bwr8j
type: agent_session
schema_version: 1
title: A terminal session under the node's default Claude account, and a terminal dashboard
model: claude-opus-5-5
harness: claude-code
started: 2026-11-12T02:09+00:00
finished: 2026-11-12T02:09+00:00
created_entities: []
edited_entities: [PHS-20261106-3702ke]
outcome: done             # done | failed | aborted | pending
created: 2026-11-12
author: agent:claude
---

## Task
The owner asked whether a session started at the terminal could run under one of the
hub's Claude accounts (the wrapped `claude` used the machine's login), and whether `saarthi
ctl` should become a terminal UI with the app's features that make sense there. Asked: they
chose terminal sessions following the node's default account (changed from the app through
the hub), and a dashboard without conversations.

## What happened
- Terminal account (ADR-0046): a unary `ClaudeToken` call on the node service; `saarthi
  run` asks the daemon over the wrapper socket before starting its program, puts the token
  in `CLAUDE_CODE_OAUTH_TOKEN`, prints the account and names it in its hello; the node
  reports the session under it and the hub keeps it when a node reports the session
  without one. `--claude-token <name>` and `--own-login`; a named token that cannot be had
  stops the wrapper, the default falls back to the own login and says why. Tested end to
  end (hub, node, socket), on the wrapper binary against a stand-in daemon, and live
  against a scratch hub: no default, default `work`, own login, unknown name (exit 1).
- Dashboard (ADR-0047): `saarthi tui` in the node binary on ratatui 0.30 (crossterm only,
  about forty crates). State, keys and updates unit-tested; the screen rendered on a test
  backend; run in a pseudo-terminal against the scratch hub (connects, help, new-session
  form, quits and restores the terminal).
- Decided without asking: the dashboard leaves out launch profiles, history, resume and
  expiry editing; a new session from it always creates the directory if missing.
