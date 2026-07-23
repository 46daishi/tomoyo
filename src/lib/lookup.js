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