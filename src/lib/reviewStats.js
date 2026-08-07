import { getDb } from '$lib/db';

// ── Sessions (time tracking) — same crash-resilience pattern as sessions.js ──

export async function startReviewSession(sessionType, mediaId = null) {
    const db = await getDb();
    const result = await db.execute(
        'INSERT INTO review_sessions (session_type, media_id, started_at, last_updated_at) VALUES ($1, $2, unixepoch(), unixepoch())',
        [sessionType, mediaId]
    );
    return result.lastInsertId;
}

export async function endReviewSession(sessionId) {
    if (!sessionId) return;
    const db = await getDb();
    await db.execute(
        'UPDATE review_sessions SET ended_at = unixepoch(), last_updated_at = unixepoch() WHERE id = $1',
        [sessionId]
    );
}

export async function touchReviewSession(sessionId) {
    if (!sessionId) return;
    const db = await getDb();
    await db.execute('UPDATE review_sessions SET last_updated_at = unixepoch() WHERE id = $1', [sessionId]);
}

export async function recoverDanglingReviewSessions() {
    const db = await getDb();
    await db.execute('UPDATE review_sessions SET ended_at = last_updated_at WHERE ended_at IS NULL');
}

// ── Per-item log — drives counts now, and picker weighting later ──

export async function logReviewedItem({ sessionId, reviewType, itemKey, mediaId = null }) {
    const db = await getDb();
    await db.execute(
        'INSERT INTO review_log (session_id, review_type, item_key, media_id) VALUES ($1, $2, $3, $4)',
        [sessionId, reviewType, String(itemKey), mediaId]
    );
    await touchReviewSession(sessionId);
}

// ── Stats for the Review tab ──

export async function getReviewStats(mediaId = null) {
    const db = await getDb();

    const countRows = await db.select(
        `SELECT review_type, COUNT(*) as count
         FROM review_log
         WHERE $1 IS NULL OR media_id = $1
         GROUP BY review_type`,
        [mediaId]
    );
    const counts = { word: 0, sentence: 0 };
    for (const row of countRows) counts[row.review_type] = row.count;

    const timeRows = await db.select(
        `SELECT session_type, SUM(COALESCE(ended_at, last_updated_at) - started_at) as seconds
         FROM review_sessions
         WHERE $1 IS NULL OR media_id = $1
         GROUP BY session_type`,
        [mediaId]
    );
    const timeSpent = { word: 0, sentence: 0 };
    for (const row of timeRows) timeSpent[row.session_type] = row.seconds ?? 0;

    // Streak/last-review are always global, per your spec — never
    // media-scoped, so mediaId is deliberately not applied here.
    const dayRows = await db.select(
        `SELECT DISTINCT date(reviewed_at, 'unixepoch', 'localtime') as day
         FROM review_log ORDER BY day DESC`
    );
    const days = dayRows.map((r) => r.day);
    const { currentStreak, longestStreak } = computeStreaks(days);

    return {
        wordReviewCount: counts.word,
        sentenceReviewCount: counts.sentence,
        wordTimeSeconds: timeSpent.word,
        sentenceTimeSeconds: timeSpent.sentence,
        lastReviewDate: days[0] ?? null,
        currentStreak,
        longestStreak,
    };
}

function computeStreaks(days) {
    if (days.length === 0) return { currentStreak: 0, longestStreak: 0 };

    const toDate = (s) => new Date(s + 'T00:00:00');
    const oneDayMs = 86400000;

    let longestStreak = 1;
    let run = 1;
    for (let i = 1; i < days.length; i++) {
        const diff = (toDate(days[i - 1]) - toDate(days[i])) / oneDayMs;
        run = diff === 1 ? run + 1 : 1;
        longestStreak = Math.max(longestStreak, run);
    }

    const todayStr = new Date().toISOString().slice(0, 10);
    const yesterdayStr = new Date(Date.now() - oneDayMs).toISOString().slice(0, 10);

    let currentStreak = 0;
    if (days[0] === todayStr || days[0] === yesterdayStr) {
        currentStreak = 1;
        for (let i = 1; i < days.length; i++) {
            const diff = (toDate(days[i - 1]) - toDate(days[i])) / oneDayMs;
            if (diff === 1) currentStreak += 1;
            else break;
        }
    }

    return { currentStreak, longestStreak };
}

// ── Picker weighting — not consumed yet, but the schema/log already
// supports it: never-reviewed items simply won't appear in this map. ──

export async function getReviewWeighting(reviewType, mediaId = null) {
    const db = await getDb();
    const rows = await db.select(
        `SELECT item_key, COUNT(*) as times_reviewed, MAX(reviewed_at) as last_reviewed_at
         FROM review_log
         WHERE review_type = $1 AND ($2 IS NULL OR media_id = $2)
         GROUP BY item_key`,
        [reviewType, mediaId]
    );
    return Object.fromEntries(
        rows.map((r) => [r.item_key, { timesReviewed: r.times_reviewed, lastReviewedAt: r.last_reviewed_at }])
    );
}

export async function getReviewActivityByDay(weeks = 52) {
    const db = await getDb();
    const rows = await db.select(
        `SELECT date(reviewed_at, 'unixepoch', 'localtime') as day,
                COUNT(*) as count
         FROM review_log
         GROUP BY day
         ORDER BY day DESC
         LIMIT $1`,
        [weeks * 7]
    );
    return rows.map((r) => ({ date: r.day, studyMinutes: r.count }));
}