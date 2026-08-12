import { appDataDir, join } from '@tauri-apps/api/path';
import { mkdir, writeFile } from '@tauri-apps/plugin-fs';
import { fetch as tauriFetch } from '@tauri-apps/plugin-http';

const API = 'https://api.vndb.org/kana';

const VN_FIELDS =
    'id, title, alttitle, olang, titles { lang, title, latin, main }, image.url, length, length_minutes, rating';

/** @param {Record<string, any>} body */
async function postVn(body) {
    const res = await tauriFetch(`${API}/vn`, {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify(body),
    });
    if (!res.ok) {
        const detail = await res.text().catch(() => '');
        throw new Error(`VNDB API error (${res.status}): ${detail}`);
    }
    return res.json();
}

/**
 * Search visual novels by title.
 * @param {string} title
 * @param {{ limit?: number }} [opts]
 * @returns {Promise<Array<Record<string, any>>>}
 */
export async function searchVn(title, { limit = 10 } = {}) {
    const data = await postVn({
        filters: ['search', '=', title],
        fields: VN_FIELDS,
        sort: 'searchrank',
        results: limit,
    });
    return data.results ?? [];
}

/**
 * Fetch a single visual novel by its VNDB id (e.g. "v17").
 * @param {string} id
 * @returns {Promise<Record<string, any> | null>}
 */
export async function getVn(id) {
    const data = await postVn({
        filters: ['id', '=', id],
        fields: VN_FIELDS,
    });
    return (data.results ?? [])[0] ?? null;
}

/**
 * Download a VNDB cover image into the app's covers directory.
 * @param {string} imageUrl
 * @returns {Promise<string | null>} stored path (to save in media.cover_path), or null
 */
export async function downloadCover(imageUrl) {
    if (!imageUrl) return null;

    const res = await tauriFetch(imageUrl);
    if (!res.ok) return null;

    const data = await res.arrayBuffer();
    const buf = data instanceof Uint8Array ? data : new Uint8Array(data);

    const dataDir = await appDataDir();
    const coversDir = await join(dataDir, 'covers');
    await mkdir(coversDir, { recursive: true });

    const ext = (imageUrl.split('?')[0].match(/\.(\w{1,5})$/)?.[1] || 'jpg').toLowerCase();
    const filename = `${crypto.randomUUID()}.${ext}`;
    const destPath = await join(coversDir, filename);

    await writeFile(destPath, buf);
    return destPath;
}

/** Loose normalization for comparing VNDB titles against local media names. */
/** @param {string} s */
export function normalizeTitle(s) {
    return (s ?? '')
        .toLowerCase()
        .replace(/[~!:().,_'"\-\s]+/g, ' ')
        .trim();
}

/**
 * Find a VN whose title matches the given name 1:1 (case/punctuation-insensitive).
 * @param {Array<Record<string, any>>} vns
 * @param {string} title
 * @returns {Record<string, any> | null}
 */
export function findExactVn(vns, title) {
    const target = normalizeTitle(title);
    if (!target) return null;
    return vns.find((v) => normalizeTitle(v.title) === target) ?? null;
}

/**
 * Pick the VN title according to the user's preference.
 * - "romaji"  -> the site's main (romanized) title, e.g. "Monobeno"
 * - "japanese"-> the original-script title, e.g. "ものべの"
 * @param {Record<string, any> | null} vn
 * @param {string} pref
 * @returns {string}
 */
export function preferredTitle(vn, pref) {
    if (!vn) return '';
    if (pref === 'japanese') {
        const original =
            (vn.titles ?? []).find((/** @type {any} */ t) => t.main)?.title || vn.alttitle || vn.title || '';
        return original.trim() || vn.title;
    }
    return vn.title ?? '';
}

const LENGTH_LABELS = ['Very short', 'Short', 'Medium', 'Long', 'Very long'];

/**
 * Format a VN's play time as e.g. "Long (46h)".
 *
 * The site derives the label from length_minutes (2h / 10h / 30h / 50h
 * boundaries) and only falls back to the coarse `length` field (1-5) when
 * there are no length votes.
 * @param {Record<string, any> | null} vn
 * @returns {string}
 */
export function formatVnLength(vn) {
    if (!vn) return 'Unknown';

    let label = null;
    let hours = null;

    if (vn.length_minutes != null) {
        const minutes = vn.length_minutes;
        const idx = minutes <= 120 ? 0 : minutes <= 600 ? 1 : minutes <= 1800 ? 2 : minutes <= 3000 ? 3 : 4;
        label = LENGTH_LABELS[idx];
        hours = Math.round(minutes / 60);
    } else if (vn.length != null) {
        label = LENGTH_LABELS[vn.length - 1] ?? null;
    }

    if (label && hours != null) return `${label} (${hours}h)`;
    if (label) return label;
    if (hours != null) return `${hours}h`;
    return 'Unknown';
}

/**
 * Format a VN's Bayesian rating (10-100) as e.g. "8.5", or '—' when unrated.
 * @param {Record<string, any> | null} vn
 * @returns {string}
 */
export function formatVnRating(vn) {
    const rating = vn?.rating;
    if (rating == null) return '—';
    return (rating / 10).toFixed(1);
}

/** Strip a leading "v" prefix and validate the rest is a number. */
/** @param {string} value */
export function parseVndbId(value) {
    const m = String(value ?? '').trim().match(/^v?(\d+)$/i);
    return m ? `v${m[1]}` : null;
}

/** Build a vndb.org page URL for an id (accepts "17" or "v17"). */
/** @param {string} id */
export function vndbUrl(id) {
    const parsed = parseVndbId(id);
    return parsed ? `https://vndb.org/${parsed}` : null;
}
