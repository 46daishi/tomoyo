/**
 * Frequency display helpers for dictionary entries, mirroring how
 * Yomitan-style dictionaries surface word commonness.
 *
 * `freq_rank` is a per-ENTRY rank from the Jiten Visual Novel frequency
 * list (scripts of 2,081 VNs, CC BY-SA 4.0): best rank across the entry's
 * (spelling, reading) pairs — readings alone for usually-kana words, the
 * kana string itself when a word is never seen in kanji. 0 = unlisted.
 * Archaic entries never inherit a living kana string's rank.
 */

/**
 * Exact corpus rank for display, or null when the entry is unlisted.
 * @param {{ freq_rank?: number } | null | undefined} entry
 * @returns {number | null} the rank, or null when unranked (0/missing).
 */
export function frequencyRank(entry) {
    const rank = entry?.freq_rank ?? 0;
    return rank > 0 ? rank : null;
}

/**
 * Frequency tier for display: 'common', 'uncommon', or 'rare', cut from
 * the same VN rank — so "Common" means common in visual novels, not in
 * newspapers. Cutoffs validated against tier members (dialogue staples
 * land ≤20k: 食べる #176, ぶっ殺す #11229; literary/technical words land
 * beyond 60k: 侯 #67652, 稿 #61469).
 *
 * - common: rank 1-20,000.
 * - uncommon: rank 20,001-60,000 (仮名 #45036 lives here).
 * - rare: rank 60,000+ or unlisted (79% of entries — dictionaries are
 *   mostly rare words). Read as "outside the VN top 60k" rather than a
 *   proven rarity verdict.
 * @param {{ freq_rank?: number } | null | undefined} entry
 * @returns {'common' | 'uncommon' | 'rare'}
 */
export function frequencyTier(entry) {
    const rank = frequencyRank(entry);
    if (rank === null) return 'rare';
    if (rank <= 20000) return 'common';
    if (rank <= 60000) return 'uncommon';
    return 'rare';
}
