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
