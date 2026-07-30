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