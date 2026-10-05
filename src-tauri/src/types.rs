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
    // which case the priority tiers decide as before.
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
    Deconjugated,    // reached by deconjugating a conjugated surface
}
