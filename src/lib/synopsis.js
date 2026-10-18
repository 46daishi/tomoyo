import { getDb } from '$lib/db';

/**
 * VNDB descriptions carry light markup ([url=…]…[/url], [spoiler], [b], …).
 * For the plain-text synopsis shown on the media page the tags are dropped
 * and the wrapped text is kept.
 * @param {string | null | undefined} text
 * @returns {string}
 */
export function stripVndbMarkup(text) {
    return (text ?? '')
        .replace(/\[url=[^\]]*\]([\s\S]*?)\[\/url\]/gi, '$1')
        .replace(/\[\/?(?:b|i|u|s|spoiler|quote|code)\]/gi, '')
        .trim();
}

/**
 * Persist the synopsis for a media entry. `updated_at` is deliberately left
 * alone: the automatic VNDB fetch would otherwise silently reshuffle the
 * home page's updated_at ordering just from opening a page.
 * @param {number} mediaId
 * @param {string | null} synopsis
 */
export async function saveSynopsis(mediaId, synopsis) {
    const db = await getDb();
    const text = synopsis?.trim();
    await db.execute('UPDATE media SET synopsis = $1 WHERE id = $2', [text || null, mediaId]);
}
