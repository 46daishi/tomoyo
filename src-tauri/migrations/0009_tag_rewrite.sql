-- 1. Build the new table with the key you actually want now: media_id-based, no tag text required
CREATE TABLE word_tags_new (
    word_id INTEGER NOT NULL,
    media_id INTEGER NOT NULL REFERENCES media(id) ON DELETE CASCADE,
    PRIMARY KEY (word_id, media_id)
);

-- 2. Backfill from whatever old rows already had media_id set (unlikely, but cheap to include),
--    plus reconstruct from the old tag text where possible
INSERT OR IGNORE INTO word_tags_new (word_id, media_id)
SELECT word_id, media_id FROM word_tags WHERE media_id IS NOT NULL
UNION
SELECT wt.word_id, m.id
FROM word_tags wt
JOIN media m ON m.tag = wt.tag OR (m.tag IS NULL AND m.title = wt.tag)
WHERE wt.media_id IS NULL;

-- 3. Swap tables
DROP TABLE word_tags;
ALTER TABLE word_tags_new RENAME TO word_tags;