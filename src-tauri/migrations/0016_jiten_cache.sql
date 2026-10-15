-- Cached Jiten.moe deck stats per media (Phase 1: character count,
-- unique words, difficulty). Resolved via VNDB id, falling back to
-- automatic title match; NULL deck id means unresolved/unfetched.
ALTER TABLE media ADD COLUMN jiten_deck_id INTEGER;
ALTER TABLE media ADD COLUMN jiten_char_count INTEGER;
ALTER TABLE media ADD COLUMN jiten_difficulty REAL;
ALTER TABLE media ADD COLUMN jiten_unique_words INTEGER;
ALTER TABLE media ADD COLUMN jiten_fetched_at INTEGER;
