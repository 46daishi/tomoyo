import { getDb } from '$lib/db';

export const TIMEFRAME_OPTIONS = [
    { value: 'today', label: 'Today' },
    { value: '7d', label: 'Last 7 Days' },
    { value: '30d', label: 'Last 30 Days' },
    { value: 'year', label: 'This Year' },
    { value: 'all', label: 'All Time' },
];

/** @param {string} timeframe */
function timeframeCutoff(timeframe) {
    const now = new Date();
    switch (timeframe) {
        case 'today':
            return Math.floor(new Date(now.getFullYear(), now.getMonth(), now.getDate()).getTime() / 1000);
        case '7d':
            return Math.floor(Date.now() / 1000) - 7 * 86400;
        case '30d':
            return Math.floor(Date.now() / 1000) - 30 * 86400;
        case 'year':
            return Math.floor(new Date(now.getFullYear(), 0, 1).getTime() / 1000);
        case 'all':
        default:
            return null;
    }
}

/**
 * @param {{ mediaId?: number | null, timeframe?: string }} args
 */
export async function getReadingStats({ mediaId = null, timeframe = 'all' }) {
    const db = await getDb();
    const cutoff = timeframeCutoff(timeframe);

    const [row] = await db.select(
        `SELECT
            COALESCE(SUM(moji_read), 0) as total_moji,
            COALESCE(SUM(sentences_read), 0) as total_sentences,
            COALESCE(SUM(COALESCE(ended_at, last_updated_at) - started_at), 0) as total_seconds,
            MAX(started_at) as last_read,
            COUNT(*) as session_count
         FROM sessions
         WHERE ($1 IS NULL OR media_id = $1)
           AND ($2 IS NULL OR started_at >= $2)`,
        [mediaId, cutoff]
    );

    const totalHours = row.total_seconds / 3600;
    return {
        totalMoji: row.total_moji,
        totalSentences: row.total_sentences,
        totalSeconds: row.total_seconds,
        lastRead: row.last_read,
        sessionCount: row.session_count,
        mojiPerHour: totalHours > 0 ? row.total_moji / totalHours : 0,
        sentencesPerHour: totalHours > 0 ? row.total_sentences / totalHours : 0,
        avgSentenceLength: row.total_sentences > 0 ? row.total_moji / row.total_sentences : 0,
    };
}

/** @param {number | null | undefined} [mediaId] */
export async function getReadingStreak(mediaId = null) {
    const db = await getDb();
    const dayRows = await db.select(
        `SELECT DISTINCT date(started_at, 'unixepoch', 'localtime') as day
         FROM sessions WHERE $1 IS NULL OR media_id = $1 ORDER BY day DESC`,
        [mediaId]
    );
    return computeStreaks(dayRows.map((/** @type {any} */ r) => r.day));
}

/**
 * @param {string[]} days
 */
function computeStreaks(days) {
    if (days.length === 0) return { currentStreak: 0, longestStreak: 0 };
    /** @type {(s: string) => Date} */
    const toDate = (s) => new Date(s + 'T00:00:00');
    const oneDayMs = 86400000;

    let longestStreak = 1;
    let run = 1;
    for (let i = 1; i < days.length; i++) {
        const diff = (toDate(days[i - 1]).getTime() - toDate(days[i]).getTime()) / oneDayMs;
        run = diff === 1 ? run + 1 : 1;
        longestStreak = Math.max(longestStreak, run);
    }

    const todayStr = new Date().toISOString().slice(0, 10);
    const yesterdayStr = new Date(Date.now() - oneDayMs).toISOString().slice(0, 10);

    let currentStreak = 0;
    if (days[0] === todayStr || days[0] === yesterdayStr) {
        currentStreak = 1;
        for (let i = 1; i < days.length; i++) {
            const diff = (toDate(days[i - 1]).getTime() - toDate(days[i]).getTime()) / oneDayMs;
            if (diff === 1) currentStreak += 1;
            else break;
        }
    }
    return { currentStreak, longestStreak };
}

/**
 * @param {number | null | undefined} [mediaId]
 * @param {number} [weeks]
 */
export async function getMojiActivityByDay(mediaId = null, weeks = 52) {
    const db = await getDb();
    const rows = await db.select(
        `SELECT date(started_at, 'unixepoch', 'localtime') as day, SUM(moji_read) as moji
         FROM sessions WHERE $1 IS NULL OR media_id = $1
         GROUP BY day ORDER BY day DESC LIMIT $2`,
        [mediaId, weeks * 7]
    );
    return rows.map((/** @type {any} */ r) => ({ date: r.day, studyMinutes: r.moji ?? 0 }));
}

/**
 * @param {number | null | undefined} [mediaId]
 * @returns {Promise<number[]>}
 */
export async function getActivityYears(mediaId = null) {
    const db = await getDb();
    const rows = await db.select(
        `SELECT strftime('%Y', started_at, 'unixepoch', 'localtime') as year
         FROM sessions WHERE $1 IS NULL OR media_id = $1
         GROUP BY year ORDER BY year DESC`,
        [mediaId]
    );
    return rows.map((/** @type {any} */ r) => Number(r.year));
}

/**
 * @param {number | null | undefined} [mediaId]
 * @param {number} [year]
 */
export async function getMojiActivityByYear(mediaId = null, year = new Date().getFullYear()) {
    const db = await getDb();
    const start = `${year}-01-01`;
    const end = `${year + 1}-01-01`;
    const rows = await db.select(
        `SELECT date(started_at, 'unixepoch', 'localtime') as day, SUM(moji_read) as moji
         FROM sessions
         WHERE ($1 IS NULL OR media_id = $1)
           AND date(started_at, 'unixepoch', 'localtime') >= $2
           AND date(started_at, 'unixepoch', 'localtime') < $3
         GROUP BY day ORDER BY day ASC`,
        [mediaId, start, end]
    );
    return rows.map((/** @type {any} */ r) => ({ date: r.day, studyMinutes: r.moji ?? 0 }));
}

/**
 * Global profile stats for the analytics sidebar.
 * @returns {Promise<{ mediaCount: number, firstUsed: number | null, wordCount: number, sentenceCount: number }>}
 */
export async function getProfileStats() {
    const db = await getDb();
    const [row] = await db.select(
        `SELECT
            (SELECT COUNT(*) FROM media) as media_count,
            (SELECT MIN(ts) FROM (
                SELECT MIN(created_at) as ts FROM media
                UNION ALL
                SELECT MIN(started_at) as ts FROM sessions
                UNION ALL
                SELECT MIN(created_at) as ts FROM word_sentences
                UNION ALL
                SELECT MIN(read_at) as ts FROM sentence_read_events
            )) as first_used,
            (SELECT COUNT(*) FROM words) as word_count,
            (SELECT COUNT(DISTINCT sentence_text) FROM word_sentences) as sentence_count`
    );
    return {
        mediaCount: row.media_count ?? 0,
        firstUsed: row.first_used ?? null,
        wordCount: row.word_count ?? 0,
        sentenceCount: row.sentence_count ?? 0,
    };
}

/**
 * @param {number | null | undefined} [mediaId]
 * @param {number} [days]
 * @returns {Promise<Array<{ key: string, mined: number }>>}
 */
export async function getWordsMinedByDay(mediaId = null, days = 30) {
    const db = await getDb();
    const cutoff = Math.floor(Date.now() / 1000) - days * 86400;
    const rows = await db.select(
        `SELECT date(created_at, 'unixepoch', 'localtime') as day, COUNT(*) as count
         FROM word_sentences
         WHERE ($1 IS NULL OR media_id = $1) AND created_at >= $2
         GROUP BY day ORDER BY day ASC`,
        [mediaId, cutoff]
    );
    const byDay = Object.fromEntries(rows.map((/** @type {any} */ r) => [r.day, r.count]));
    const result = [];
    for (let i = days - 1; i >= 0; i--) {
        const key = new Date(Date.now() - i * 86400000).toISOString().slice(0, 10);
        result.push({ key, mined: byDay[key] ?? 0 });
    }
    return result;
}

/**
 * Counts mined words per status, honoring the media filter exactly like the
 * dictionary's getWords() (a word counts if linked via word_tags or a mined
 * sentence for that media).
 *
 * @param {number | null | undefined} [mediaId]
 * @returns {Promise<Array<{ status: number, count: number }>>}
 */
export async function getWordStatusCounts(mediaId = null) {
    const db = await getDb();
    const rows = await db.select(
        `SELECT w.status, COUNT(DISTINCT w.id) as count
         FROM words w
         LEFT JOIN word_tags wt ON wt.word_id = w.id
         LEFT JOIN word_sentences ws ON ws.word_id = w.id
         WHERE $1 IS NULL OR wt.media_id = $1 OR ws.media_id = $1
         GROUP BY w.status`,
        [mediaId]
    );
    return rows.map((/** @type {any} */ r) => ({ status: r.status, count: r.count }));
}

/**
 * @param {number | null | undefined} [mediaId]
 * @param {number} [days]
 */
export async function getDailyMoji(mediaId = null, days = 30) {
    const db = await getDb();
    const cutoff = Math.floor(Date.now() / 1000) - days * 86400;
    const rows = await db.select(
        `SELECT date(started_at, 'unixepoch', 'localtime') as day,
                SUM(moji_read) as moji,
                SUM(COALESCE(ended_at, last_updated_at) - started_at) as seconds
         FROM sessions WHERE ($1 IS NULL OR media_id = $1) AND started_at >= $2
         GROUP BY day ORDER BY day ASC`,
        [mediaId, cutoff]
    );
    const byDay = Object.fromEntries(rows.map((/** @type {any} */ r) => [r.day, { moji: r.moji ?? 0, seconds: r.seconds ?? 0 }]));

    const result = [];
    for (let i = days - 1; i >= 0; i--) {
        const key = new Date(Date.now() - i * 86400000).toISOString().slice(0, 10);
        const entry = byDay[key] ?? { moji: 0, seconds: 0 };
        result.push({ key, moji: entry.moji, minutes: Math.round(entry.seconds / 60) });
    }
    return result;
}

/**
 * @param {number | null | undefined} [mediaId]
 * @param {number} [months]
 */
export async function getMonthlyMoji(mediaId = null, months = 12) {
    const db = await getDb();
    const cutoff = Math.floor(Date.now() / 1000) - months * 31 * 86400;
    const rows = await db.select(
        `SELECT strftime('%Y-%m', started_at, 'unixepoch', 'localtime') as month,
                SUM(moji_read) as moji,
                SUM(COALESCE(ended_at, last_updated_at) - started_at) as seconds
         FROM sessions WHERE ($1 IS NULL OR media_id = $1) AND started_at >= $2
         GROUP BY month ORDER BY month ASC`,
        [mediaId, cutoff]
    );
    const byMonth = Object.fromEntries(
        rows.map((/** @type {any} */ r) => [r.month, { moji: r.moji ?? 0, minutes: Math.round((r.seconds ?? 0) / 60) }])
    );

    const result = [];
    const now = new Date();
    for (let i = months - 1; i >= 0; i--) {
        const d = new Date(now.getFullYear(), now.getMonth() - i, 1);
        const key = `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}`;
        const entry = byMonth[key] ?? { moji: 0, minutes: 0 };
        result.push({ key, moji: entry.moji, minutes: entry.minutes });
    }
    return result;
}