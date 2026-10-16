-- Media category for tab filtering and per-type modal options.
-- Existing rows default to visual novels (the app's original library).
ALTER TABLE media ADD COLUMN media_type TEXT NOT NULL DEFAULT 'visual_novel';
