import { getDb } from '$lib/db';

export async function startSession(mediaId) {
    const db = await getDb();
    const result = await db.execute(
        'INSERT INTO sessions (media_id, started_at, last_updated_at) VALUES ($1, unixepoch(), unixepoch())',
        [mediaId]
    );
    return result.lastInsertId;
}

export async function endSession(sessionId) {
    if (!sessionId) return;
    const db = await getDb();
    await db.execute(
        'UPDATE sessions SET ended_at = unixepoch(), last_updated_at = unixepoch() WHERE id = $1',
        [sessionId]
    );
}

export async function recordSentenceRead(sessionId, mojiCount) {
    if (!sessionId) return;
    const db = await getDb();
    await db.execute(
        'UPDATE sessions SET moji_read = moji_read + $1, sentences_read = sentences_read + 1, last_updated_at = unixepoch() WHERE id = $2',
        [mojiCount, sessionId]
    );
}

export async function recoverDanglingSessions() {
    const db = await getDb();
    await db.execute(
        'UPDATE sessions SET ended_at = last_updated_at WHERE ended_at IS NULL'
    );
}

export async function getMediaStats(mediaId) {
    const db = await getDb();

    const [sessionRows, wordRows] = await Promise.all([
        db.select(
            `SELECT
                MAX(COALESCE(ended_at, started_at)) AS last_active,
                COALESCE(SUM(moji_read), 0) AS moji_read,
                COALESCE(SUM(
                    CASE WHEN ended_at IS NOT NULL THEN ended_at - started_at ELSE 0 END
                ), 0) AS reading_seconds,
                COUNT(*) AS session_count
             FROM sessions
             WHERE media_id = $1`,
            [mediaId]
        ),
        db.select(
            `SELECT COUNT(DISTINCT word_id) AS words_mined
             FROM word_sentences
             WHERE media_id = $1`,
            [mediaId]
        )
    ]);

    const session = sessionRows[0] ?? { last_active: null, moji_read: 0, reading_seconds: 0, session_count: 0 };
    const words = wordRows[0] ?? { words_mined: 0 };

    return { ...session, words_mined: words.words_mined ?? 0 };
}