-- Phase PHS-20261117-f22kxe item 3: what a session is about, from its transcript as the node
-- reports it in a daemon `meta` event: its best title and where that came from (a name given
-- to it, Claude Code's ai-title, a summary), and its git branch. Empty until reported.
ALTER TABLE sessions ADD COLUMN title TEXT NOT NULL DEFAULT '';
ALTER TABLE sessions ADD COLUMN title_source TEXT NOT NULL DEFAULT '';
ALTER TABLE sessions ADD COLUMN git_branch TEXT NOT NULL DEFAULT '';
