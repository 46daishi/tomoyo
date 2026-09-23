import { getDb } from './db';

/** @param {{ mediaId?: number | null }} [args] */
export async function getNames({ mediaId = null } = {}) {
    const db = await getDb();
    if (mediaId == null) {
        return db.select(
            `SELECT n.*, COALESCE(m.tag, m.title) as media_label
             FROM names n
             LEFT JOIN media m ON m.id = n.media_id
             ORDER BY n.created_at DESC`
        );
    }
    return db.select('SELECT * FROM names WHERE media_id = $1 ORDER BY created_at DESC', [mediaId]);
}

/** @param {{ mediaId: number, name: string, reading?: string }} args */
export async function saveName({ mediaId, name, reading = '' }) {
    const db = await getDb();
    const cleanName = (name ?? '').trim();
    const cleanReading = (reading ?? '').trim();
    if (!cleanName) throw new Error('Name is empty.');
    await db.execute(
        `INSERT INTO names (media_id, name, reading)
         VALUES ($1, $2, $3)
         ON CONFLICT(media_id, name) DO UPDATE SET reading = excluded.reading, updated_at = unixepoch()`,
        [mediaId, cleanName, cleanReading]
    );
    const rows = await db.select('SELECT id FROM names WHERE media_id = $1 AND name = $2', [
        mediaId,
        cleanName,
    ]);
    return rows[0]?.id ?? null;
}

/** @param {{ id: number, name: string, reading: string }} args */
export async function updateName({ id, name, reading }) {
    const db = await getDb();
    const cleanName = (name ?? '').trim();
    const cleanReading = (reading ?? '').trim();
    if (!cleanName) throw new Error('Name is empty.');
    await db.execute('UPDATE names SET name = $1, reading = $2, updated_at = unixepoch() WHERE id = $3', [
        cleanName,
        cleanReading,
        id,
    ]);
}

/** @param {{ id: number }} args */
export async function deleteName({ id }) {
    const db = await getDb();
    await db.execute('DELETE FROM names WHERE id = $1', [id]);
}

// Longest saved name containing character `index` (so hovering anywhere
// inside a name still resolves the whole name). Returns
// { row, start, end } or null. All offsets are character (code point)
// indices, matching the lookup engine.
/**
 * @param {Array<Record<string, any>> | null | undefined} names
 * @param {string} text
 * @param {number} index
 */
export function findNameAt(names, text, index) {
    if (!text || !names?.length) return null;
    const chars = [...text];
    if (index < 0 || index >= chars.length) return null;
    let best = null;
    for (const row of names) {
        if (!row?.name) continue;
        const needle = [...row.name];
        const from = Math.max(0, index - needle.length + 1);
        for (let start = from; start <= index; start++) {
            if (start + needle.length > chars.length) continue;
            let ok = true;
            for (let i = 0; i < needle.length; i++) {
                if (chars[start + i] !== needle[i]) {
                    ok = false;
                    break;
                }
            }
            if (ok) {
                if (!best || needle.length > best.len) {
                    best = { row, start, end: start + needle.length, len: needle.length };
                }
                break;
            }
        }
    }
    return best;
}

// Greedy longest-match spans for underlining, mirroring scan_sentence:
// advance past each match, one char past non-matches.
/**
 * @param {string} text
 * @param {Array<Record<string, any>> | null | undefined} names
 */
export function scanNameSpans(text, names) {
    /** @type {Array<{ start: number, end: number, nameId: number }>} */
    const spans = [];
    if (!text || !names?.length) return spans;
    const chars = [...text];
    const needles = names
        .filter((r) => r?.name)
        .map((r) => ({ row: r, chars: [...r.name] }));
    let pos = 0;
    while (pos < chars.length) {
        let match = null;
        for (const { row, chars: n } of needles) {
            if (n.length > chars.length - pos) continue;
            let ok = true;
            for (let i = 0; i < n.length; i++) {
                if (chars[pos + i] !== n[i]) {
                    ok = false;
                    break;
                }
            }
            if (ok && (!match || n.length > match.len)) match = { row, len: n.length };
        }
        if (match) {
            spans.push({ start: pos, end: pos + match.len, nameId: match.row.id });
            pos += match.len;
        } else {
            pos += 1;
        }
    }
    return spans;
}

// A frontend-only dictionary entry for a saved name: no reading line, the
// user's reading goes in definitions, POS line always reads NAME (CUSTOM).
/** @param {Record<string, any>} row */
export function nameToEntry(row) {
    return {
        id: `name-${row.id}`,
        spellings: [row.name],
        readings: [],
        definitions: row.reading ? [row.reading] : [],
        pos: ['NAME (CUSTOM)'],
        customName: true,
        nameId: row.id,
    };
}
