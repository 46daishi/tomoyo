import { getDb } from '$lib/db';
import { scanSentenceSpans } from '$lib/lookup.js';
import { getKnownWordsMap } from '$lib/dictionary.js';

const STATUS_WEIGHT = { 0: 0.10, 1: 0.30, 2: 0.50, 3: 0.80, 4: 1.00 };
const MIN_MINED_WORDS_THRESHOLD = 300;

export async function getVocabularyCoverage(mediaId) {
    const db = await getDb();
    const [{ count: minedCount }] = await db.select('SELECT COUNT(*) as count FROM words');
    if (minedCount < MIN_MINED_WORDS_THRESHOLD) {
        return { gathering: true, percentage: null };
    }

    const sentences = await db.select(
        `SELECT sentence_text FROM sentence_read_events
         WHERE media_id = $1 ORDER BY read_at DESC, id DESC LIMIT 100`,
        [mediaId]
    );
    if (sentences.length === 0) return { gathering: true, percentage: null };

    const knownWordsMap = await getKnownWordsMap();
    let totalWeight = 0;
    let totalOccurrences = 0;

    for (const { sentence_text } of sentences) {
        for (const span of await scanSentenceSpans(sentence_text)) {
            const knownId = (span.entryIds ?? []).find((id) => knownWordsMap.has(id));
            const status = knownId !== undefined ? knownWordsMap.get(knownId) : undefined;
            totalWeight += status !== undefined ? STATUS_WEIGHT[status] : 0;
            totalOccurrences += 1;
        }
    }

    return {
        gathering: false,
        percentage: totalOccurrences > 0 ? Math.round((totalWeight / totalOccurrences) * 100) : null,
        sentenceCount: sentences.length,
    };
}