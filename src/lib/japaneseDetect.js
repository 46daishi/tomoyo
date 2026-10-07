// Ranges: Hiragana, Katakana, CJK Unified Ideographs (kanji), CJK punctuation, Half-width Katakana & Punctuation
const JAPANESE_CHAR_REGEX = /[\u3040-\u309F\u30A0-\u30FF\u4E00-\u9FFF\u3000-\u303F\uFF61-\uFF9F]/g;

// Punctuation (all scripts, incl. ASCII dots VNs pad lines with) and
// symbols (♪☆♥♪, emoji…) carry no language signal either way.
const IGNORED_REGEX = /[\s\p{P}\p{S}]/gu;

/**
 * Returns true if at least `threshold` fraction of the content
 * characters in the text are Japanese script. Whitespace, punctuation
 * and symbols are ignored entirely, so a line like "えっと........."
 * still counts as Japanese instead of being drowned by dots.
 */
export function isMostlyJapanese(text, threshold = 0.3) {
    const stripped = text.replace(IGNORED_REGEX, '');
    if (stripped.length === 0) return false;

    const matches = stripped.match(JAPANESE_CHAR_REGEX);
    const jpCount = matches ? matches.length : 0;

    return jpCount / stripped.length >= threshold;
}