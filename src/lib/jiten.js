import { fetch as tauriFetch } from '@tauri-apps/plugin-http';
import { getDb } from '$lib/db';

const API = 'https://api.jiten.moe/api/media-deck';

/**
 * @param {string} url
 * @returns {Promise<any|null>} parsed JSON, or null on any failure (offline,
 *   timeout, non-2xx, bad JSON) — Jiten data is best-effort enrichment.
 */
async function getJson(url, { timeoutMs = 10000 } = {}) {
    try {
        const res = await Promise.race([
            tauriFetch(url),
            new Promise((_, reject) => setTimeout(() => reject(new Error('timeout')), timeoutMs)),
        ]);
        if (!res.ok) return null;
        return await res.json().catch(() => null);
    } catch (err) {
        console.warn(`Jiten request failed: ${url}`, err);
        return null;
    }
}

/**
 * @param {string} vndbId e.g. "v17"
 * @returns {Promise<number[]>} Jiten deck ids (usually zero or one)
 */
export async function findDeckIdsByVndbId(vndbId) {
    if (!vndbId) return [];
    const data = await getJson(`${API}/by-link-id/2/${encodeURIComponent(vndbId)}`);
    if (!Array.isArray(data)) return [];
    return data
        .map((d) => (Number.isInteger(d) ? d : d?.deckId))
        .filter((d) => Number.isInteger(d));
}

/**
 * @param {string} title
 * @returns {Promise<number[]>} deck ids, best match first
 */
export async function searchDecksByTitle(title) {
    if (!title?.trim()) return [];
    const data = await getJson(`${API}/get-media-decks?titleFilter=${encodeURIComponent(title.trim())}`);
    const list = data?.data;
    if (!Array.isArray(list)) return [];
    return list.map((d) => d?.deckId).filter((d) => Number.isInteger(d));
}

/**
 * @param {number} deckId
 * @returns {Promise<{ deckId: number, characterCount: number|null, difficulty: number|null, uniqueWords: number|null }|null>}
 */
export async function getDeckStats(deckId) {
    if (!Number.isInteger(deckId)) return null;
    const data = await getJson(`${API}/${deckId}/detail`);
    const deck = data?.data?.mainDeck;
    if (!deck) return null;
    const num = (/** @type {unknown} */ v) => (typeof v === 'number' && Number.isFinite(v) ? v : null);
    return {
        deckId,
        characterCount: num(deck.characterCount),
        difficulty: num(deck.difficultyRaw),
        uniqueWords: num(deck.uniqueWordCount),
    };
}

/**
 * Difficulty band labels exactly as shown on jiten.moe
 * (Jiten.Web/app/utils/difficultyColours.ts): floor of the raw score,
 * clamped to 0-5. Note this differs slightly from the API's integer
 * `difficulty` field, which uses model-specific thresholds
 * (DifficultyMapper.cs) — the deck page prose ("average, 2.9 out of 5")
 * follows the floored raw value, so this does too.
 * @param {number|null|undefined} raw
 * @returns {string|null}
 */
export function difficultyLabel(raw) {
    const band = difficultyBand(raw);
    if (band == null) return null;
    const names = ['Beginner', 'Easy', 'Average', 'Hard', 'Expert', 'Insane'];
    return names[band];
}

/**
 * Band index 0-5 for a raw score, exactly as on jiten.moe
 * (Jiten.Web/app/utils/difficultyColours.ts).
 * @param {number|null|undefined} raw
 * @returns {number|null}
 */
export function difficultyBand(raw) {
    if (typeof raw !== 'number' || !Number.isFinite(raw)) return null;
    return Math.min(Math.max(Math.floor(raw), 0), 5);
}

/**
 * Resolve the Jiten deck for a media item: exact VNDB link first, then
 * automatic title match (top hit — ambiguous titles can mismatch, but the
 * deck id is cached and surfaced via link so a wrong match is visible).
 * @param {{ vndbId?: string|null, title?: string }} media
 * @returns {Promise<number|null>}
 */
export async function resolveDeckForMedia({ vndbId, title }) {
    if (vndbId) {
        const ids = await findDeckIdsByVndbId(vndbId);
        if (ids.length > 0) return ids[0];
    }
    if (title) {
        const ids = await searchDecksByTitle(title);
        if (ids.length > 0) return ids[0];
    }
    return null;
}

/**
 * @param {number} mediaId
 * @returns {Promise<{ deckId: number, characterCount: number|null, difficulty: number|null, uniqueWords: number|null }|null>}
 */
export async function getCachedJitenStats(mediaId) {
    const db = await getDb();
    const rows = await db.select(
        'SELECT jiten_deck_id AS deckId, jiten_char_count AS characterCount, jiten_difficulty AS difficulty, jiten_unique_words AS uniqueWords FROM media WHERE id = $1',
        [mediaId]
    );
    const row = rows?.[0];
    if (!row || !Number.isInteger(row.deckId)) return null;
    return {
        deckId: row.deckId,
        characterCount: row.characterCount ?? null,
        difficulty: row.difficulty ?? null,
        uniqueWords: row.uniqueWords ?? null,
    };
}

/**
 * @param {number} mediaId
 * @param {{ deckId: number, characterCount: number|null, difficulty: number|null, uniqueWords: number|null }} stats
 */
export async function saveJitenStats(mediaId, stats) {
    const db = await getDb();
    await db.execute(
        'UPDATE media SET jiten_deck_id = $1, jiten_char_count = $2, jiten_difficulty = $3, jiten_unique_words = $4, jiten_fetched_at = unixepoch() WHERE id = $5',
        [stats.deckId, stats.characterCount, stats.difficulty, stats.uniqueWords, mediaId]
    );
}

/**
 * Cached deck stats for a media item: DB first, network (then cache) on miss.
 * @param {number} mediaId
 * @param {{ vndbId?: string|null, title?: string }} media
 */
export async function loadJitenStats(mediaId, { vndbId, title }) {
    if (!Number.isInteger(mediaId)) return null;
    const cached = await getCachedJitenStats(mediaId).catch(() => null);
    if (cached) return cached;
    const deckId = await resolveDeckForMedia({ vndbId, title });
    if (deckId == null) return null;
    const stats = await getDeckStats(deckId);
    if (!stats) return null;
    await saveJitenStats(mediaId, stats).catch((err) => console.warn('Jiten cache write failed:', err));
    return stats;
}
