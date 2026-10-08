/**
 * Short display labels for JMdict part-of-speech entity names.
 *
 * The raw entity names are glossary-verbose ("noun (common)
 * (futsuumeishi)" on 189,600 entries) and actively misleading next to the
 * frequency pills: "common" there is the common-vs-proper-noun
 * distinction, not word frequency. These labels keep the linguistic
 * meaning while fitting a tooltip line.
 */

/** @type {Object<string, string>} */
const SHORT_LABELS = {
    'noun (common) (futsuumeishi)': 'noun',
    'noun or participle which takes the aux. verb suru': 'suru verb',
    'suru verb - special class': 'suru verb (special)',
    'suru verb - included': 'suru verb',
    'su verb - precursor to the modern suru': 'su verb (precursor)',
    'Godan verb - Iku/Yuku special class': 'godan verb (iku/yuku)',
    "Godan verb with 'ru' ending (irregular verb)": 'godan verb (ru, irregular)',
    'Ichidan verb': 'ichidan verb',
    'Ichidan verb - zuru verb (alternative form of -jiru verbs)': 'ichidan verb (zuru)',
    'expressions (phrases, clauses, etc.)': 'expression',
    "nouns which may take the genitive case particle 'no'": 'の-adjective',
    'noun or verb acting prenominally': 'prenominal',
    'noun, used as a suffix': 'noun suffix',
    'noun, used as a prefix': 'noun prefix',
    'pre-noun adjectival (rentaishi)': 'pre-noun adjectival',
    "'taru' adjective": 'taru-adjective',
    'adjectival nouns or quasi-adjectives (keiyodoshi)': 'na-adjective',
    'adjective (keiyoushi)': 'i-adjective',
    'adjective (keiyoushi) - yoi/ii class': 'i-adjective (yoi/ii)',
    'adverb (fukushi)': 'adverb',
    "adverb taking the 'to' particle": 'adverb (と)',
    'interjection (kandoushi)': 'interjection'
};

const GODAN_ENDING = /^Godan verb with '([a-z]+)' ending$/;

/**
 * @param {string} tag a raw JMdict pos entity name
 * @returns {string} the short display label (raw tag when unmapped)
 */
export function shortPosTag(tag) {
    const mapped = SHORT_LABELS[tag];
    if (mapped !== undefined) return mapped;
    const godan = GODAN_ENDING.exec(tag);
    if (godan !== null) return `godan verb (${godan[1]})`;
    return tag;
}

/**
 * @param {string[] | null | undefined} tags raw JMdict pos tags
 * @returns {string} comma-joined short labels ('' when none)
 */
export function formatPosTags(tags) {
    return (tags ?? []).map(shortPosTag).join(', ');
}
