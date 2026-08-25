import { invoke } from '@tauri-apps/api/core';

/**
 * Resolves whatever word/phrase starts at `position` (a character index
 * into `text`), the same way JL resolves a click/cursor point: longest
 * dictionary/deconjugation match starting exactly there, nothing else in
 * the text is touched or pre-computed.
 *
 * `skip` selects which match to return counting from longest (0) downward
 * — pass 1, 2, ... to reach shorter candidates a longer match "swallows"
 * (e.g. 今日 or いい when 今日は / いい天気 are found first). Returns
 * `null` if `position` is out of bounds, or if there's no candidate at
 * that skip depth (caller should wrap back to skip = 0).
 *
 * Returns a MatchSpan `{ start, end, surface, entries, deconjugated_from }`.
 */
export async function lookupAtPosition(text, position, skip = 0) {
    return await invoke('lookup_at_position', { text, position, skip });
}

/**
 * Morphological tokens (MeCab) for a whole sentence: { start, end, surface,
 * base_form, pos, reading } with char offsets.
 */
export async function tokenizeSentence(text) {
    return await invoke('tokenize_sentence', { text });
}

export async function scanSentenceSpans(text) {
    if (!text) return [];
    // Server-side greedy longest-match in one IPC round-trip (shares a single
    // tokenization across the whole scan, unlike per-character invokes).
    const spans = await invoke('scan_sentence', { text });
    return spans.map((s) => ({
        start: s.start,
        end: s.end,
        // All dictionary entries this span resolves to (may be several
        // homophones). Kept alongside `wordId` (the top-ranked entry) so
        // callers can check whether *any* of them is mined/known.
        entryIds: (s.entries ?? []).map((e) => e.id),
        wordId: s.entries.length > 0 ? s.entries[0].id : null,
    }));
}

export async function findKnownWordSpans(text, knownWordsMap) {
    const allSpans = await scanSentenceSpans(text);
    const spans = [];
    for (const s of allSpans) {
        // Highlight the span if *any* of its entries is a known word, not
        // just the top-ranked one — otherwise mining 前(まえ) while 先(さき)
        // ranks first would leave the word unhighlighted.
        const knownId = (s.entryIds ?? []).find((id) => knownWordsMap.has(id));
        if (knownId !== undefined) {
            spans.push({ ...s, wordId: knownId, status: knownWordsMap.get(knownId) });
        }
    }
    return spans;
}

/**
 * Find word spans to underline based on highlight mode.
 * - "none": return nothing
 * - "known": underline if any entry is in the dictionary (existing behavior)
 * - "unknown": underline only if ALL entries are unknown
 *   (not in dictionary, or in dictionary with status 0 when treatNewAsUnknown)
 */
export async function findHighlightedWordSpans(text, knownWordsMap, mode, treatNewAsUnknown) {
    if (mode === 'none' || !text) return [];
    if (mode === 'known') return findKnownWordSpans(text, knownWordsMap);

    // unknown mode
    const allSpans = await scanSentenceSpans(text);
    const spans = [];
    for (const s of allSpans) {
        const entryIds = s.entryIds ?? [];
        if (entryIds.length === 0) {
            // No dictionary entries at all — truly unknown
            spans.push({ ...s, wordId: null, status: null });
            continue;
        }
        const allUnknown = entryIds.every((id) => {
            const status = knownWordsMap.get(id);
            if (status === undefined) return true;           // not in dict
            if (treatNewAsUnknown && status === 0) return true; // status NEW
            return false;
        });
        if (allUnknown) {
            spans.push({ ...s, wordId: entryIds[0], status: knownWordsMap.get(entryIds[0]) ?? null });
        }
    }
    return spans;
}