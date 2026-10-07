/**
 * Splits a dictionary headword into segments for furigana (<ruby>) display.
 *
 * Characters the spelling and reading share (usually okurigana and kana
 * prefixes) render as plain text; each kanji run takes the reading between
 * the surrounding matches, e.g. 食べる / たべる -> 食(た) + べる.
 * Anything ambiguous falls back to a single whole-word segment so a wrong
 * alignment can never render.
 *
 * @param {string} spelling e.g. "食べる"
 * @param {string} reading e.g. "たべる"
 * @returns {{ text: string, reading: string }[]} segments; segments with an
 *   empty `reading` render as plain text.
 */
export function distributeFurigana(spelling, reading) {
    // No furigana to show at all.
    const asIs = [{ text: spelling, reading: '' }];
    // Alignment failed: show the whole reading over the whole spelling
    // rather than a wrong split (or nothing).
    const whole = [{ text: spelling, reading }];
    if (!spelling || !reading) return asIs;
    // Fold full-width katakana to hiragana for COMPARISON only. The mapping
    // is 1:1 so indices stay valid for slicing the originals — this is what
    // lets hiragana okurigana match katakana readings (かっこ vs カッコ)
    // instead of bailing to whole-word ruby.
    /** @param {string} c @returns {string} */
    const foldKana = (c) => {
        const cp = c.codePointAt(0) ?? 0;
        return cp >= 0x30a1 && cp <= 0x30f6 ? String.fromCodePoint(cp - 0x60) : c;
    };
    /** @param {string} s @returns {string} */
    const foldStr = (s) => [...s].map(foldKana).join('');
    if (foldStr(spelling) === foldStr(reading)) return asIs;

    // Iteration marks repeat the previous character, which in headwords
    // is essentially always kanji (島々, 日々) — group them with kanji so
    // they take whole-word furigana instead of failing alignment.
    /** @param {string} c @returns {boolean} */
    const isKanji = (c) => /[\u3400-\u4DBF\u4E00-\u9FFF\uF900-\uFAFF\u3005\u300B]/u.test(c);

    // Maximal runs of kanji vs non-kanji. A spelling with no kanji never
    // needs furigana (if it differed from the reading we'd only be
    // restating katakana/half-width variants).
    /** @type {{ text: string, kanji: boolean }[]} */
    const runs = [];
    for (const c of spelling) {
        const kanji = isKanji(c);
        const lastRun = runs[runs.length - 1];
        if (lastRun && lastRun.kanji === kanji) {
            lastRun.text += c;
        } else {
            runs.push({ text: c, kanji });
        }
    }
    if (!runs.some((r) => r.kanji)) return whole;

    const readingChars = [...reading];
    const foldedReading = readingChars.map(foldKana);
    const segments = [];
    let pos = 0; // code-point cursor into readingChars
    for (let i = 0; i < runs.length; i++) {
        const run = runs[i];
        const runChars = [...run.text];
        const foldedRun = runChars.map(foldKana);
        if (!run.kanji) {
            // Kana run: must match the reading verbatim at this position
            // (compared script-folded, so hiragana/katakana spellings align).
            const slice = foldedReading.slice(pos, pos + runChars.length);
            if (slice.join('') !== foldedRun.join('')) return whole;
            segments.push({ text: run.text, reading: '' });
            pos += runChars.length;
        } else if (i === runs.length - 1) {
            // Last run takes whatever reading is left.
            segments.push({ text: run.text, reading: readingChars.slice(pos).join('') });
            pos = readingChars.length;
        } else {
            // Kanji run takes the reading up to the next kana run.
            const next = [...runs[i + 1].text].map(foldKana);
            let k = -1;
            outer: for (let j = pos; j + next.length <= foldedReading.length; j++) {
                for (let t = 0; t < next.length; t++) {
                    if (foldedReading[j + t] !== next[t]) continue outer;
                }
                k = j;
                break;
            }
            if (k === -1) return whole;
            segments.push({ text: run.text, reading: readingChars.slice(pos, k).join('') });
            pos = k;
        }
    }
    if (pos !== readingChars.length) return whole;
    // A kanji run that took nothing (or a trailing run left with nothing
    // because earlier runs over-consumed) means the greedy split lost the
    // true morpheme boundaries — e.g. agreeing on an anchor kana that
    // belongs to the next kanji's reading. Fall back to whole-word
    // furigana rather than render missing/shifted readings.
    if (segments.some((s, i) => s.text && !s.reading && runs[i] && runs[i].kanji)) {
        return whole;
    }
    return segments;
}
