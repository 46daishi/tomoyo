-- Per-media opt-out from all Jiten.moe lookups and UI. Unset (0) keeps
-- the previous behaviour; set (1) skips resolving, fetching, caching
-- and display for that entry everywhere.
ALTER TABLE media ADD COLUMN jiten_excluded INTEGER NOT NULL DEFAULT 0;
