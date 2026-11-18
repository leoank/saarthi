# ADR-0063: Search across sessions with a contentless FTS5 index at the hub, and paged sessions

Status: accepted
Date: 2026-11-18

## Context
The phase *The app at a glance* ([[PHS-20261117-f22kxe]], item 9) asks for ways to find
things: search across sessions, as atuin's threads do (atuin was studied on 2026-11-17 and
its lessons went into the phase), search within a session, a map of the owner's prompts, and
older items paged in as the person scrolls up. The hub's record already holds every
transcript line; on rogue's copy one session runs to 22,987 events and 127 MB, all of
which went to the phone each time it was opened.

## Decision
- **The hub indexes, with SQLite FTS5, contentless** (migration 0009): `search_text` holds
  the words of each prompt and reply (a transcript `user` line's typed text, an
  `assistant` line's text blocks; never a tool's result or text injected for the model)
  under the event's rowid; `search_title` each session's title and directory under its
  rowid. The index stores no text of its own: the record has it already. One background
  task fills both from where it got to (`search_state`) and keeps them current, so a line
  is searchable a second or two after it arrives and there is one writer; a title is
  re-indexed when the node reports a new one.
- **A query matches every word, the last as a prefix**; a session ranks by its best match
  (bm25), a title match counting five times, scaled down with age (half at 30 days). The
  snippet is cut from the matching line around the first query word. `Search` returns the
  sessions with their snippet and the line's seq.
- **Paging rides on `StreamSession`**: `tail` starts a session's stream at its last N
  transcript lines, preceded by the node's latest usage, context and queue reports from
  before them; `before_seq` with `tail` sends one older page and ends. The core's
  `older` turns a page into items, its first turn counted from the usage report before
  it. The app opens a session with 300 lines and brings in a page whenever the person
  nears the top, keyed so the view stays put.
- **Search within a session and the prompt map are the app's**, over what it has loaded; a
  search first brings in every older page.

## Alternatives considered
- An index that stores the text (a normal FTS5 table): doubles what the record keeps.
- External-content FTS5 over `events`: the payload is JSON, not the words, and keeping
  such an index in step needs triggers on every insert.
- Indexing on every stored event, in the node service's path: a second writer next to the
  backfill, and a slower path for every event; one indexer is simpler.
- A separate RPC for pages: `StreamSession` already streams events, which a large page
  needs (a unary reply is capped at 4 MB).
- Searching within a session at the hub: the app has the items, rendered as it shows them.

## Consequences
- Search is server-side and fast; the index on rogue's copy of 35,000 prompts and replies
  took three seconds to build and 14 MB.
- Opening a long session sends its tail, not its whole history.
- A search within a long session still brings the whole session to the phone, page by page.
