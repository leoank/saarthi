-- Sessions deleted from a client (ADR-0065): their ids, so nothing a node sends for them
-- later brings them back.
CREATE TABLE deleted_sessions (
  node_id    TEXT NOT NULL,
  session_id TEXT NOT NULL,
  deleted_ms INTEGER NOT NULL,
  PRIMARY KEY (node_id, session_id)
);
