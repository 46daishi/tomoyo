/**
 * Frequency display helpers for dictionary entries, mirroring how
 * Yomitan-style dictionaries surface word commonness.
 *
 * `freq_rank` is a per-SPELLING rank (best wordfreq-ja rank across the
 * entry's written forms; 0 = unlisted). Readings are shared by many words,
 * so they are deliberately never used as lookup keys.
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
 * Frequency tier for display: 'common', 'uncommon', or 'rare'. The tiers
 * are built on the per-entry JMdict priority tags — editorial judgments
 * in the Jisho/Takoboto "Common word" spirit — while `frequencyRank`
 * above carries the exact per-spelling corpus number for entries that
 * have one.
 *
 * - common: ichi1/news1 (top newspaper lists), nf01-nf12 (roughly the top
 *   6k), spec1/gai1 (editors' explicit "common" flags).
 * - uncommon: the remaining measured bands (nf13-nf48, ichi2/news2,
 *   spec2/gai2).
 * - rare: no frequency data at all (86% of entries). Overwhelmingly
 *   specialized, literary, or archaic words — though a few ordinary words
 *   also lack tags, so read it as "outside the ~30k listed words" rather
 *   than a proven rarity verdict.
 * @param {{ priority?: string[] } | null | undefined} entry
 * @returns {'common' | 'uncommon' | 'rare'}
 */
export function frequencyTier(entry) {
    const prio = entry?.priority ?? [];
    /** @param {...string} tags */
    const has = (...tags) => tags.some((t) => prio.includes(t));
    const nfBands = prio
        .filter((t) => /^nf\d+$/.test(t))
        .map((t) => parseInt(t.slice(2), 10));
    const bestNf = nfBands.length > 0 ? Math.min(...nfBands) : Infinity;
    if (has('ichi1', 'news1') || bestNf <= 12 || has('spec1', 'gai1')) return 'common';
    if (bestNf <= 48 || has('ichi2', 'news2', 'spec2', 'gai2')) return 'uncommon';
    return 'rare';
}
