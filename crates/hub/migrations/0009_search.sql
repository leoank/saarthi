-- Search across sessions (phase PHS-20261117-f22kxe item 9, ADR-0063): FTS5 indexes that
-- hold no text of their own (contentless), as atuin's do. search_text has a row per
-- transcript line with words in it (a prompt, a reply), under the event's rowid;
-- search_title one per session, its title and directory, under the session's rowid. A
-- background task fills both and keeps them current, recording how far it got here.
CREATE VIRTUAL TABLE search_text USING fts5(
  body, content = '', contentless_delete = 1, tokenize = 'unicode61 remove_diacritics 2'
);
CREATE VIRTUAL TABLE search_title USING fts5(
  title, cwd, content = '', contentless_delete = 1, tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TABLE search_state (
  id            INTEGER PRIMARY KEY CHECK (id = 1),
  events_upto   INTEGER NOT NULL,
  sessions_upto INTEGER NOT NULL
);
INSERT INTO search_state VALUES (1, 0, 0);
