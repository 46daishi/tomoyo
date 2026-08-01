-- Status levels (New -> Recognized -> Familiar -> Learned -> Known), stored
-- as an integer 0-4 for easy cycling/ordering. status_updated_at tracks when
-- a word last changed status, for the review feature (e.g. scheduling
-- reviews based on time-at-status).
ALTER TABLE words ADD COLUMN status INTEGER NOT NULL DEFAULT 0;
ALTER TABLE words ADD COLUMN status_updated_at INTEGER NOT NULL DEFAULT 0;

UPDATE words SET status_updated_at = unixepoch() WHERE status_updated_at = 0;