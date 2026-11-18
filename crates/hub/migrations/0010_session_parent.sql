-- The session a resumed or forked one came from, from its transcript (phase
-- PHS-20261117-f22kxe item 15); empty for one that came from none.
ALTER TABLE sessions ADD COLUMN parent_id TEXT NOT NULL DEFAULT '';
