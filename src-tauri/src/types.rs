use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize)]
pub(crate) struct TokenOut {
    pub(crate) surface: String,
    pub(crate) reading: String,
    pub(crate) pos: String,
    pub(crate) base_form: String,
}

/// A single morphological token (vibrato/MeCab) with the fields needed for
/// lookups: character offsets, surface, dictionary base form, POS, and the
/// normalized (hiragana) reading MeCab assigned in context.
#[derive(Clone, serde::Serialize)]
pub(crate) struct MorphToken {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) surface: String,
    pub(crate) base_form: String,
    pub(crate) pos: String,
    pub(crate) reading: String,
}

#[derive(Deserialize, Serialize, Clone)]
pub(crate) struct DictEntry {
    pub(crate) id: u32,
    pub(crate) spellings: Vec<String>,
    pub(crate) readings: Vec<String>,
    pub(crate) definitions: Vec<String>,
    pub(crate) pos: Vec<String>,
    pub(crate) priority: Vec<String>,
    // JMdict "usually written using kana alone" (&uk;) marker. Absent in
    // older JSONs — defaults to false so they keep loading.
    #[serde(default)]
    pub(crate) kana_only: bool,
    // Per-form restriction tags from JMdict (ke_inf/re_inf, e.g. "ateji",
    // "sk"), parallel to `spellings`/`readings`. Absent in older JSONs.
    // Not enforced yet (precision flip comes separately); carried so the
    // data survives a regen.
    #[serde(default)]
    pub(crate) ke_inf: Vec<Vec<String>>,
    #[serde(default)]
    pub(crate) re_inf: Vec<Vec<String>>,
    // Readings valid only for these spellings (re_restr), parallel to
    // `readings`. Empty = unrestricted.
    #[serde(default)]
    pub(crate) re_restr: Vec<Vec<String>>,
    // Reading never valid in kanji form (re_nokanji), parallel to readings.
    #[serde(default)]
    pub(crate) re_nokanji: Vec<bool>,
    // Sense-level misc tags (s_inf-adjacent, e.g. "uk", "arch").
    #[serde(default)]
    pub(crate) misc: Vec<String>,
    // Corpus frequency rank (lower is more common); 0 = unranked, in
    // which case the priority tiers decide as before. Ranks break ties
    // within equal tiers at sort time (unlisted sorts rarest).
    #[serde(default)]
    pub(crate) freq_rank: u32,
}

#[derive(serde::Serialize)]
pub(crate) struct MatchSpan {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) surface: String,
    pub(crate) entries: Vec<Arc<DictEntry>>,
    pub(crate) deconjugated_from: Option<String>,
    pub(crate) related_entries: Vec<Arc<DictEntry>>, // entries containing `surface`, excluding exact matches already in `entries`
}

/// Ordering here (lower = better) is the sort precedence.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum MatchKind {
    ExactScript, // entry lists the raw surface script itself (katakana
    // シャイ for シャイ) — outranks normalized-only homophones (謝意).
    PrimarySpelling, // normalized surface == entry's primary (first) spelling
    Spelling,        // normalized surface == some other spelling
    Morphological,   // reached via the tokenizer's base form (e.g. します -> する)
    Reading,         // normalized surface == a reading
    Deconjugated,   // reached by deconjugating a conjugated surface
}

/// Tokenizer-dictionary layout (UniDic feature columns): coarse POS =
/// fields[0], dictionary form = fields[10] (`orthBase` 書字形基本形 — the
/// kana-leaning base the engine's string checks expect, e.g. する rather
/// than the lemma's 為る), reading = fields[9] (`pron` 発音形出現形,
/// katakana, normalized to hiragana below). Short/unknown rows fall back
/// to surface/empty, mirroring the old IPAdic indices.
const POS_FIELD: usize = 0;
const BASE_FIELD: usize = 10;
const READING_FIELD: usize = 9;

/// Coarse UniDic POS names the lookup engine doesn't know, translated to
/// the IPAdic-shaped names its match sites expect. Everything else (動詞,
/// 助動詞, 助詞, 形容詞, 副詞, 名詞, ...) is identical in both dictionaries.
pub(crate) fn alias_pos(pos: &str) -> &str {
    match pos {
        // Punctuation: っ, 。, 、 ...
        "補助記号" => "記号",
        // Na-adjective stems (きれい) and そう-stems: IPAdic files these
        // as nouns, and the engine's noun paths (suru-compounds,
        // どう/そう/こう+したい, copula handling) rely on that.
        "形状詞" => "名詞",
        _ => pos,
    }
}

/// One morphological token from a raw tokenizer row (char offsets, surface,
/// comma-separated feature string).
pub(crate) fn morph_token_from(
    start: usize,
    end: usize,
    surface: String,
    feature: &str,
) -> MorphToken {
    let fields: Vec<&str> = feature.split(',').collect();
    // UniDic marks unknown readings "*" (punctuation, sokuon っ,
    // out-of-lexicon katakana): treat as empty so unknown-token guards
    // and reading-concatenation (completions match tails by reading)
    // see an honest gap, exactly like a missing field.
    let reading_raw = fields.get(READING_FIELD).unwrap_or(&"");
    let reading = if reading_raw.is_empty() || *reading_raw == "*" {
        String::new()
    } else {
        // Readings come out in katakana; normalize to hiragana so they
        // can be compared against dictionary readings, then resolve
        // chouonpu to plain vowels for the deconjugation engine.
        crate::normalize::resolve_chouonpu(&crate::normalize::normalize_text(reading_raw))
    };
    MorphToken {
        start,
        end,
        surface: surface.clone(),
        base_form: fields
            .get(BASE_FIELD)
            .map(|s| s.to_string())
            .unwrap_or(surface),
        pos: alias_pos(fields.get(POS_FIELD).unwrap_or(&"")).to_string(),
        reading,
    }
}

/// One frontend-facing token from a raw tokenizer row (no offsets, reading
/// left raw — matches the old `tokenize_text` behavior field for field).
pub(crate) fn token_out_from(surface: String, feature: &str) -> TokenOut {
    let fields: Vec<&str> = feature.split(',').collect();
    TokenOut {
        surface: surface.clone(),
        reading: fields.get(READING_FIELD).unwrap_or(&"").to_string(),
        pos: alias_pos(fields.get(POS_FIELD).unwrap_or(&"")).to_string(),
        base_form: fields
            .get(BASE_FIELD)
            .map(|s| s.to_string())
            .unwrap_or(surface),
    }
}
