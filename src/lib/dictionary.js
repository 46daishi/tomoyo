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

export async function addSentenceEntry({ sentenceText, tag = null, translation = null, mediaId = null }) {
    const db = await getDb();
    await db.execute(
        'INSERT INTO sentences (sentence_text, tag, translation, media_id) VALUES ($1, $2, $3, $4)',
        [sentenceText, tag, translation, mediaId]
    );
}

export async function getWordWithDetails(wordId) {
    const db = await getDb();
    const [word] = await db.select('SELECT * FROM words WHERE id = $1', [wordId]);
    if (!word) return null;

    const tags = await db.select('SELECT tag FROM word_tags WHERE word_id = $1', [wordId]);
    const sentences = await db.select(
        'SELECT * FROM word_sentences WHERE word_id = $1 GROUP BY sentence_text ORDER BY created_at DESC',
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
        `SELECT w.*, GROUP_CONCAT(DISTINCT wt.tag) as tags,
                COUNT(DISTINCT ws.id) as sentence_count
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