import { getDb } from '$lib/db';

export async function logLookupEvent({ mediaId = null, wordId = null, surfaceText }) {
    const db = await getDb();
    await db.execute(
        'INSERT INTO lookup_events (media_id, word_id, surface_text) VALUES ($1, $2, $3)',
        [mediaId, wordId, surfaceText]
    );
}

export async function getFrequentUnknownWords(mediaId = null, minCount = 3, limit = 50) {
    const db = await getDb();
    const query = mediaId
        ? `SELECT surface_text, COUNT(*) as count FROM lookup_events
           WHERE word_id IS NULL AND media_id = $1
           GROUP BY surface_text HAVING count >= $2 ORDER BY count DESC LIMIT $3`
        : `SELECT surface_text, COUNT(*) as count FROM lookup_events
           WHERE word_id IS NULL
           GROUP BY surface_text HAVING count >= $1 ORDER BY count DESC LIMIT $2`;
    const params = mediaId ? [mediaId, minCount, limit] : [minCount, limit];
    return db.select(query, params);
}