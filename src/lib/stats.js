import { getDb } from '$lib/db';

export async function getMediaStats(mediaId) {
    const db = await getDb();

    const [session] = await db.select(
        `SELECT SUM(moji_read) as moji, SUM(sentences_read) as sentences,
                SUM(COALESCE(ended_at, unixepoch()) - started_at) as seconds
         FROM sessions WHERE media_id = $1`,
        [mediaId]
    );

    const [lookups] = await db.select(
        'SELECT COUNT(*) as count FROM lookup_events WHERE media_id = $1',
        [mediaId]
    );

    const [mined] = await db.select(
        'SELECT COUNT(DISTINCT word_id) as count FROM word_sentences WHERE media_id = $1',
        [mediaId]
    );

    const moji = session?.moji ?? 0;
    const difficulty = moji > 0 ? (lookups?.count ?? 0) / moji : null;

    return {
        mojiRead: moji,
        sentencesRead: session?.sentences ?? 0,
        hoursSpent: (session?.seconds ?? 0) / 3600,
        lookupsDone: lookups?.count ?? 0,
        wordsMined: mined?.count ?? 0,
        difficulty,
    };
}