-- Phase 2 step 3: named Claude tokens (ADR-0039, ADR-0042). The values live only in the
-- --claude-tokens file; the record keeps, per name, a fingerprint, the dates, the reminder
-- and the latest usage windows seen under the token. A row stays when its name leaves the
-- file, so sessions keep naming it; only names in the file are offered.

CREATE TABLE claude_tokens (
  name               TEXT PRIMARY KEY,
  fingerprint        TEXT NOT NULL,
  first_seen_ms      INTEGER NOT NULL,   -- restarts when the fingerprint changes
  expires_ms         INTEGER,            -- set by hand; NULL for a year after first seen
  reminded_ms        INTEGER,            -- when the expiry reminder went out
  usage_ms           INTEGER,            -- when a session last reported its windows
  five_hour_pct      REAL,
  five_hour_resets_s INTEGER,
  seven_day_pct      REAL,
  seven_day_resets_s INTEGER
);

-- The token a node's spawned sessions run under when none is picked; '' for its own login.
ALTER TABLE nodes ADD COLUMN default_claude_token TEXT NOT NULL DEFAULT '';

-- The token a session last ran under, as the hub chose it at spawn or resume; '' for the
-- node's own login or a session the node did not start.
ALTER TABLE sessions ADD COLUMN claude_token TEXT NOT NULL DEFAULT '';
