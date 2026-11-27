-- Policies the hub stores beside its bootstrap file (ADR-0081, phase PHS-20261126-f3e51h
-- item 8): Cedar text by id, taken only once the whole set validates against the schema.
CREATE TABLE policies (
  id TEXT PRIMARY KEY,
  text TEXT NOT NULL,
  updated_ms INTEGER NOT NULL,
  updated_by TEXT NOT NULL
);
