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

export async function scanSentenceSpans(text) {
    if (!text) return [];
    const chars = [...text];
    const spans = [];
    let pos = 0;

    while (pos < chars.length) {
        const result = await lookupAtPosition(text, pos, 0);
        if (!result) { pos += 1; continue; }
        spans.push({
            start: result.start,
            end: result.end,
            wordId: result.entries.length > 0 ? result.entries[0].id : null,
        });
        pos = result.end;
    }
    return spans;
}

export async function findKnownWordSpans(text, knownWordsMap) {
    const allSpans = await scanSentenceSpans(text);
    return allSpans
        .filter((s) => s.wordId !== null && knownWordsMap.has(s.wordId))
        .map((s) => ({ ...s, status: knownWordsMap.get(s.wordId) }));
}