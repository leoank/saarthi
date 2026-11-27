-- An app's labels on a session it spawned (phase PHS-20261126-f3e51h item 5), a JSON object
-- of strings such as {"job":"1234"}: set on the spawn, kept through a resume, carried to a
-- fork, matched by a list. Recorded with the launch before the node is asked, like
-- launched_by, and taken from there by the session's first insert.
ALTER TABLE launches ADD COLUMN labels TEXT NOT NULL DEFAULT '{}';
ALTER TABLE sessions ADD COLUMN labels TEXT NOT NULL DEFAULT '{}';
