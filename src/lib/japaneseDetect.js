// Ranges: Hiragana, Katakana, CJK Unified Ideographs (kanji), CJK punctuation, Half-width Katakana & Punctuation
const JAPANESE_CHAR_REGEX = /[\u3040-\u309F\u30A0-\u30FF\u4E00-\u9FFF\u3000-\u303F\uFF61-\uFF9F]/g;

/**
 * Returns true if at least `threshold` fraction of non-whitespace
 * characters in the text are Japanese script.
 */
export function isMostlyJapanese(text, threshold = 0.3) {
    const stripped = text.replace(/\s/g, '');
    if (stripped.length === 0) return false;

    const matches = stripped.match(JAPANESE_CHAR_REGEX);
    const jpCount = matches ? matches.length : 0;

    return jpCount / stripped.length >= threshold;
}