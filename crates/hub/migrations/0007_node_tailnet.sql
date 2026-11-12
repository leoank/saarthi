-- Phase 3: embedded tsnet (ADR-0048). The tailnet a node's link last came over and who
-- that tailnet said the caller was; both empty for a link over plain TCP. Recorded only.
ALTER TABLE nodes ADD COLUMN tailnet TEXT NOT NULL DEFAULT '';
ALTER TABLE nodes ADD COLUMN tailnet_peer TEXT NOT NULL DEFAULT '';
