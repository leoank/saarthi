# ADR-0065: Deleted sessions stay deleted, and old transcripts can be dropped

Status: accepted
Date: 2026-11-18

## Context
The hub keeps every event of every session for good (the event log, ADR-0020). Nothing can
be deleted from the app, and a node that re-sends a session's lines (its buffer after a
reconnect, its transcripts after a restart or a resume) would bring back anything deleted
by hand at the hub. atuin's synced threads outgrew its shell history 25 times over (its
issue 4434); this hub's copy of one machine already holds a 127 MB session. The phase *The
app at a glance* ([[PHS-20261117-f22kxe]], item 16) asks for deleting that sticks and a
retention setting. The owner chose retention off by default (2026-11-18).

## Decision
- **Deleting a session** (`DeleteSession`, from the app's History or a session's menu)
  removes its row, its events and its search entries, and records its id in
  `deleted_sessions`. Anything a node sends for that id afterwards (events, session
  reports, held requests) is dropped, so it cannot come back. A live session is refused:
  kill it first. The node and Claude Code's own transcript are not touched; deleting is
  about the hub's record.
- **Retention** (`saarthi-hub --retain-days N`, off when not given): once an hour, a session
  that ended more than N days ago loses its events, but for what its History row and search
  need: its row (title, branch, parent), its latest `usage` and `meta` reports, and its
  first few `user` lines (its first prompt). The search index loses the dropped lines.
- **A dropped session that comes back** (resumed in place, so the node reads its transcript
  again) gets its lines again: retention thins what nobody touches; it does not forbid.

## Alternatives considered
- Deleting without a record of it: the next replay or re-read brings the session back.
- Tombstones for retention too: a resumed session would then lose its own history.
- Retention by size rather than age: harder to predict what is kept; age is what History
  shows.
- Deleting at the node as well (its transcript files): those are Claude Code's, and a
  terminal `claude --resume` needs them.

## Consequences
- A deleted session's id can never be used again on that node, which Claude Code's random
  ids make moot.
- SQLite reuses the freed pages; the file shrinks only with a `VACUUM`, which is left to
  the owner.
- With retention on, an old session opens with its first prompt and its totals, not its
  conversation.
