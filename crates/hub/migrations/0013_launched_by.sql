-- Who launched a session (ADR-0082), as the hub knows it and never as asked: `client:<name>`
-- for one a client's call spawned (or a resume of it forked), `node:<name>` for one its
-- node started. Kept through a resume. Sessions from before record their node.
ALTER TABLE sessions ADD COLUMN launched_by TEXT NOT NULL DEFAULT '';
UPDATE sessions SET launched_by = 'node:' || COALESCE((SELECT name FROM nodes WHERE nodes.id = sessions.node_id), '');
-- The launcher of a session the hub asks a node to start, written before the node is asked,
-- so the session's first report, which may come before the node's reply, finds it.
CREATE TABLE launches (
  node_id TEXT NOT NULL,
  session_id TEXT NOT NULL,
  launched_by TEXT NOT NULL,
  PRIMARY KEY (node_id, session_id)
);
