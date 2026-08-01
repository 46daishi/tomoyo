import { getDb } from '$lib/db';

export async function mineWord({
    dictId,           // entry.id from the tokenizer/JMdict lookup
    spelling,
    reading,
    definitions,
    wordType,
    tag,
    sentenceText,
    highlightStart,
    highlightEnd,
    mediaId = null,
    translation = null,
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

    await db.execute(
        'INSERT OR IGNORE INTO word_tags (word_id, tag) VALUES ($1, $2)',
        [wordId, tag]
    );

    await db.execute(
        'INSERT INTO word_sentences (word_id, sentence_text, highlight_start, highlight_end, media_id, translation) VALUES ($1, $2, $3, $4, $5, $6)',
        [wordId, sentenceText, highlightStart, highlightEnd, mediaId, translation]
    );

    return wordId;
}

// Reports how a candidate mine action relates to what's already stored:
// - 'new'       -> the word isn't in the dictionary at all yet
// - 'same'      -> the word exists AND was already mined from this exact
//                  sentence + media (mining again would be a pure duplicate)
// - 'different' -> the word exists, but not from this sentence + media
//                  (mining would add a new example, which is still useful)
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

export async function getWordWithDetails(wordId) {
    const db = await getDb();
    const [word] = await db.select('SELECT * FROM words WHERE id = $1', [wordId]);
    if (!word) return null;

    const tags = await db.select('SELECT tag FROM word_tags WHERE word_id = $1', [wordId]);
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
                GROUP_CONCAT(DISTINCT wt.tag) as tags,
                COUNT(DISTINCT ws.sentence_text) as sentence_count
         FROM words w
         LEFT JOIN word_tags wt ON wt.word_id = w.id
         LEFT JOIN word_sentences ws ON ws.word_id = w.id
         WHERE ($1 IS NULL OR ws.media_id = $1)
         GROUP BY w.id
         ORDER BY w.created_at DESC`,
        [mediaId]
    );
}

// Maps a word_tags.tag value to the color of the media sharing that same tag,
// e.g. word_tags.tag = 'test2' -> media.tag = 'test2' -> media.color.
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

// All mined sentences (from word_sentences, which is where mining actually
// writes — the standalone `sentences` table is unused). word_sentences has
// no tag column of its own, so tags come from the mined word(s) attached to
// each sentence via word_tags, same as how word cards get their tags.
//
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
                GROUP_CONCAT(DISTINCT wt.tag) as tags
         FROM word_sentences ws
         LEFT JOIN word_tags wt ON wt.word_id = ws.word_id
         WHERE $1 IS NULL OR EXISTS (
             SELECT 1
             FROM word_sentences ws2
             LEFT JOIN word_tags wt2 ON wt2.word_id = ws2.word_id
             LEFT JOIN media m2 ON m2.tag = wt2.tag
             WHERE ws2.sentence_text = ws.sentence_text
               AND (ws2.media_id = $1 OR m2.id = $1)
         )
         GROUP BY ws.sentence_text
         ORDER BY ws.created_at DESC`,
        [mediaId]
    );
}