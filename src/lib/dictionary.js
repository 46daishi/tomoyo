import { getDb } from '$lib/db';

export async function mineWord({
    dictId, spelling, reading, definitions, wordType,
    mediaId = null, sentenceText, highlightStart, highlightEnd, translation = null,
}) {
    const db = await getDb();

    const existing = await db.select('SELECT id FROM words WHERE id = $1', [dictId]);
    let wordId;

    if (existing.length > 0) {
        wordId = existing[0].id;
    } else {
        await db.execute(
            'INSERT INTO words (id, spelling, reading, definitions, word_type) VALUES ($1, $2, $3, $4, $5)',
            [dictId, spelling, reading, JSON.stringify(definitions), wordType]
        );
        wordId = dictId;
    }

    if (mediaId !== null) {
        await db.execute(
            'INSERT OR IGNORE INTO word_tags (word_id, media_id) VALUES ($1, $2)',
            [wordId, mediaId]
        );
    }

    await db.execute(
        'INSERT INTO word_sentences (word_id, sentence_text, highlight_start, highlight_end, media_id, translation) VALUES ($1, $2, $3, $4, $5, $6)',
        [wordId, sentenceText, highlightStart, highlightEnd, mediaId, translation]
    );

    return wordId;
}

export async function mineWordWithTags({ dictId, spelling, reading, definitions, wordType, mediaIds = [] }) {
    const db = await getDb();

    const existing = await db.select('SELECT id FROM words WHERE id = $1', [dictId]);
    let wordId;

    if (existing.length > 0) {
        wordId = existing[0].id;
    } else {
        await db.execute(
            'INSERT INTO words (id, spelling, reading, definitions, word_type) VALUES ($1, $2, $3, $4, $5)',
            [dictId, spelling, reading, JSON.stringify(definitions), wordType]
        );
        wordId = dictId;
    }

    for (const mediaId of mediaIds) {
        await db.execute(
            'INSERT OR IGNORE INTO word_tags (word_id, media_id) VALUES ($1, $2)',
            [wordId, mediaId]
        );
    }

    return wordId;
}

export async function getWordMineStatus({ dictId, sentenceText, mediaId = null }) {
    const db = await getDb();

    const existing = await db.select('SELECT id FROM words WHERE id = $1', [dictId]);
    if (existing.length === 0) {
        return 'new';
    }

    const matching = await db.select(
        `SELECT 1 FROM word_sentences
         WHERE word_id = $1
           AND sentence_text = $2
           AND ((media_id IS NULL AND $3 IS NULL) OR media_id = $3)
         LIMIT 1`,
        [dictId, sentenceText, mediaId]
    );

    return matching.length > 0 ? 'same' : 'different';
}

export async function addSentenceEntry({ sentenceText, tag = null, translation = null, mediaId = null }) {
    const db = await getDb();
    await db.execute(
        'INSERT INTO sentences (sentence_text, tag, translation, media_id) VALUES ($1, $2, $3, $4)',
        [sentenceText, tag, translation, mediaId]
    );
}

// Translation isn't stored per unique sentence — it's a column on
// word_sentences, so the same sentence_text can have multiple rows (one per
// mined word / media occurrence). Updating "a sentence's" translation means
// updating it everywhere that sentence_text appears, so they don't drift.
export async function updateSentenceTranslation({ sentenceText, translation }) {
    const db = await getDb();
    await db.execute(
        'UPDATE word_sentences SET translation = $1 WHERE sentence_text = $2',
        [translation, sentenceText]
    );
}

// Lookup counts per word (word_id in lookup_events), for the "looked up N
// times" badge on word cards. Filterable by media so it stays consistent
// with whatever the media filter is currently showing.
export async function getLookupCounts({ mediaId = null } = {}) {
    const db = await getDb();
    const rows = await db.select(
        `SELECT word_id, COUNT(*) as count
         FROM lookup_events
         WHERE word_id IS NOT NULL
           AND ($1 IS NULL OR media_id = $1)
         GROUP BY word_id`,
        [mediaId]
    );
    return Object.fromEntries(rows.map((r) => [r.word_id, r.count]));
}

// Status levels: 0 New, 1 Recognized, 2 Familiar, 3 Learned, 4 Known.
// status_updated_at is refreshed on every change so a future review
// feature can use "time at current status" for scheduling.
export async function updateWordStatus({ wordId, status }) {
    const db = await getDb();
    await db.execute(
        'UPDATE words SET status = $1, status_updated_at = unixepoch() WHERE id = $2',
        [status, wordId]
    );
}

export async function getWordWithDetails(wordId) {
    const db = await getDb();
    const [word] = await db.select('SELECT * FROM words WHERE id = $1', [wordId]);
    if (!word) return null;

    const tags = await db.select(
        `SELECT COALESCE(m.tag, m.title) as tag
         FROM word_tags wt
         JOIN media m ON m.id = wt.media_id
         WHERE wt.word_id = $1`,
        [wordId]
    );
    const sentences = await db.select(
        'SELECT * FROM word_sentences WHERE word_id = $1 ORDER BY created_at DESC',
        [wordId]
    );

    return {
        ...word,
        definitions: JSON.parse(word.definitions),
        tags: tags.map((t) => t.tag),
        sentences,
    };
}

export async function getWords({ mediaId = null } = {}) {
    const db = await getDb();
    return db.select(
        `SELECT w.*,
                GROUP_CONCAT(DISTINCT COALESCE(m.tag, m.title)) as tags,
                COUNT(DISTINCT ws.sentence_text) as sentence_count
         FROM words w
         LEFT JOIN word_tags wt ON wt.word_id = w.id
         LEFT JOIN media m ON m.id = wt.media_id
         LEFT JOIN word_sentences ws ON ws.word_id = w.id
         WHERE $1 IS NULL OR wt.media_id = $1 OR ws.media_id = $1
         GROUP BY w.id
         ORDER BY w.created_at DESC`,
        [mediaId]
    );
}

export async function getMediaTagColors() {
    const db = await getDb();
    const rows = await db.select('SELECT tag, color FROM media WHERE tag IS NOT NULL');
    return Object.fromEntries(rows.map((r) => [r.tag, r.color]));
}

export async function getSentencesForWord(wordId) {
    const db = await getDb();
    return db.select(
        `SELECT id, sentence_text, translation, MIN(created_at) as created_at
         FROM word_sentences 
         WHERE word_id = $1 
         GROUP BY sentence_text
         ORDER BY created_at DESC`,
        [wordId]
    );
}

// A word can be mined more than once under different media, so word_tags
// can hold tags from several media for the same word — meaning a sentence
// can display a tag for a media that isn't its own word_sentences.media_id.
// The media filter has to match that same "any of its tags belongs to this
// media" logic (via each tag's owning media, matched by media.tag), not
// just the row's own media_id, or a sentence with a visible tag for the
// selected media could still be filtered out.
export async function getAllSentences({ mediaId = null } = {}) {
    const db = await getDb();
    return db.select(
        `SELECT ws.id, ws.sentence_text, ws.translation, ws.media_id, ws.created_at,
                GROUP_CONCAT(DISTINCT COALESCE(m.tag, m.title)) as tags
         FROM word_sentences ws
         LEFT JOIN word_tags wt ON wt.word_id = ws.word_id
         LEFT JOIN media m ON m.id = wt.media_id
         WHERE $1 IS NULL OR EXISTS (
             SELECT 1
             FROM word_sentences ws2
             LEFT JOIN word_tags wt2 ON wt2.word_id = ws2.word_id
             WHERE ws2.sentence_text = ws.sentence_text
               AND (ws2.media_id = $1 OR wt2.media_id = $1)
         )
         GROUP BY ws.sentence_text
         ORDER BY ws.created_at DESC`,
        [mediaId]
    );
}

export async function deleteSentence(sentenceText) {
    const db = await getDb();
    await db.execute('DELETE FROM word_sentences WHERE sentence_text = $1', [sentenceText]);
    await db.execute('DELETE FROM sentences WHERE sentence_text = $1', [sentenceText]);
}

export async function deleteWord({ wordId, mediaId = null }) {
    const db = await getDb();

    if (mediaId === null) {
        await db.execute('DELETE FROM words WHERE id = $1', [wordId]);
        return;
    }

    await db.execute('DELETE FROM word_sentences WHERE word_id = $1 AND media_id = $2', [wordId, mediaId]);
    await db.execute('DELETE FROM word_tags WHERE word_id = $1 AND media_id = $2', [wordId, mediaId]);

    const [remaining] = await db.select(
        `SELECT (SELECT COUNT(*) FROM word_tags WHERE word_id = $1) as tag_count,
                (SELECT COUNT(*) FROM word_sentences WHERE word_id = $1) as sentence_count`,
        [wordId]
    );
    if ((remaining?.tag_count ?? 0) + (remaining?.sentence_count ?? 0) === 0) {
        await db.execute('DELETE FROM words WHERE id = $1', [wordId]);
    }
}

export async function updateWordNotes({ wordId, notes }) {
    const db = await getDb();
    await db.execute(
        'UPDATE words SET notes = $1 WHERE id = $2',
        [notes, wordId]
    );
}

export async function clearDictionaryData({ mediaId = null }) {
    const db = await getDb();

    if (mediaId === null) {
        await db.execute('DELETE FROM words');
        await db.execute('DELETE FROM lookup_events');
        return;
    }

    await db.execute('DELETE FROM word_sentences WHERE media_id = $1', [mediaId]);
    await db.execute('DELETE FROM word_tags WHERE media_id = $1', [mediaId]);
    await db.execute(`
        DELETE FROM words
        WHERE id NOT IN (SELECT DISTINCT word_id FROM word_tags)
          AND id NOT IN (SELECT DISTINCT word_id FROM word_sentences)
    `);
    await db.execute('DELETE FROM lookup_events WHERE media_id = $1', [mediaId]);
}

export async function getKnownWordsMap() {
    const db = await getDb();
    const rows = await db.select('SELECT id, status FROM words');
    return new Map(rows.map((r) => [r.id, r.status]));
}

export async function getReviewPool({ mediaId = null, statuses = [] }) {
    if (statuses.length === 0) return [];
    const db = await getDb();
    const statusPlaceholders = statuses.map((_, i) => `$${i + 2}`).join(', ');

    return db.select(
        `SELECT w.* FROM words w
         WHERE w.status IN (${statusPlaceholders})
           AND ($1 IS NULL
             OR EXISTS (SELECT 1 FROM word_sentences ws WHERE ws.word_id = w.id AND ws.media_id = $1)
             OR EXISTS (SELECT 1 FROM word_tags wt WHERE wt.word_id = w.id AND wt.media_id = $1))`,
        [mediaId, ...statuses]
    );
}

export async function getSentenceReviewPool({ mediaId = null, onlyTranslated = false }) {
    const db = await getDb();
    return db.select(
        `SELECT 
            ws.sentence_text, 
            MAX(ws.translation) AS translation, 
            COALESCE(ws.media_id, MAX(wt.media_id)) AS media_id,
            GROUP_CONCAT(DISTINCT m.color) AS tag_colors
        FROM word_sentences ws
        LEFT JOIN word_tags wt ON wt.word_id = ws.word_id
        LEFT JOIN media m ON m.id = wt.media_id
        WHERE ($1 = 0 OR ws.translation IS NOT NULL)
          AND ($2 IS NULL OR ws.media_id = $2 OR wt.media_id = $2)
        GROUP BY ws.sentence_text
        ORDER BY MAX(ws.created_at) DESC`,
        [onlyTranslated ? 1 : 0, mediaId]
    );
}