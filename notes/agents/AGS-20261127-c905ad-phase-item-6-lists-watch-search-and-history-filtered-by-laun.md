---
id: AGS-20261127-c905ad
type: agent_session
schema_version: 1
title: "Phase item 6: lists, watch, search and history filtered by launcher and label, and a proper filter in the app"
model: claude-opus-5-5
harness: claude-code
started: 2026-11-27T11:10+00:00
finished: 2026-11-27T11:12+00:00
created_entities: []
edited_entities: [PHS-20261126-f3e51h]
outcome: done             # done | failed | aborted | pending
created: 2026-11-27
author: agent:claude
---

## Task
The owner asked to push and build item 6 of [[PHS-20261126-f3e51h]], with a proper filtering
UI in the app rather than the row of chips Home and History had.

## What happened
- Hub: one `Filter` (launcher exact, or every launcher of a kind ending in `:`, and labels)
  on `ListSessions`, `History`, `Search` and `Watch`; in SQL for lists and history (so pages
  stay right), per session for search and watch. Watch drops session updates the filter
  leaves out; node and pending updates all come. `ListNodesResponse.you` names the caller as
  a launcher, so a client can pick its own sessions.
- Core: `SessionInfo.launched_by` and `labels`, a `SessionFilter` record for history and
  search, `Hub::you()`. `ctl sessions|history --by <launcher>`; `ctl sessions` prints
  the launcher.
- App: Home's scope chips and History's node chips replaced by one filter, shared and
  remembered (the old scope carries over): a bar saying what it keeps, with a count and a
  clear button, opening a sheet of sections (launched by: anyone, this phone, by hand,
  any client, each launcher seen; labels: typed, or picked from those the sessions carry;
  machine; repository, the busiest six then *show all*), each choice with the sessions it
  keeps. Rows say *by ctl* in place of *spawn*. History and search pass the hub its part;
  repository stays Home's (the hub knows no repositories).
- Checked on the emulator against the scratch hub restarted on this build, with two idle
  labelled sessions spawned by `ctl` (no model turn): the bar, the sheet and its counts,
  `team=infra` narrowing Home to the two, History empty under it (no transcript yet) and
  full once cleared. The first build cut *by ctl* off the end of a long path, so it moved
  to the tag beside the state; labels moved above machine and repository.
- Decided without asking: "this phone" before the hub has said its name keeps nothing,
  rather than everything; the guide's sessions page has a *Filtering* section.
