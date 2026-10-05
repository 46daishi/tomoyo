use crate::normalize;
use crate::types::{DictEntry, MatchKind};

pub(crate) fn is_bound_only(entry: &DictEntry) -> bool {
    !entry.pos.is_empty()
        && entry.pos.iter().all(|p| p.eq_ignore_ascii_case("suffix") || p.eq_ignore_ascii_case("prefix"))
}

pub(crate) fn priority_score(entry: &DictEntry) -> u16 {
    let base = entry.priority.iter().map(|t| match t.as_str() {
        "ichi1" => 950,
        "news1" => 900,
        "gai1"  => 850,
        "spec1" => 800, // common but not corpus-measured — treat as mid-tier by default
        "spec2" | "ichi2" | "news2" | "gai2" => 500,
        t if t.starts_with("nf") => {
            let n: u16 = t[2..].parse().unwrap_or(48);
            1000 - n * 10
        }
        _ => 0,
    }).max().unwrap_or(0);

    let is_particle = entry.pos.iter().any(|p| p.eq_ignore_ascii_case("particle"));
    if is_particle { base + 200 } else { base }
}

/// Coarse part-of-speech classes for same-lemma comparison: inflected
/// forms of one lexeme (直ぐ keiyodoshi vs 直ぐに adverb) must count as
/// the same, while verb/noun/numeric/suffix stay distinct (有る vs
/// アルト, 三 vs 山河 still split). Word-split, not substring, so
/// "adverb" never counts as "verb".
pub(crate) fn pos_class(p: &str) -> &str {
    let words: Vec<&str> = p.split(|c: char| !c.is_alphabetic()).collect();
    if words.contains(&"verb") {
        "verb"
    } else if words.contains(&"adverb")
        || words.contains(&"keiyodoshi")
        || words.contains(&"adjectival")
    {
        "adverbial"
    } else {
        p
    }
}

/// A direct spelling match outranks a homophone reached only through an
/// alternate spelling or a reading — e.g. 前(まえ) beats 先(さき) when both
/// match the surface 前, since 先 merely lists 前 as a secondary spelling.
pub(crate) fn match_kind(entry: &DictEntry, key: &str) -> MatchKind {
    // Compare against every chouonpu variant so セーソー entries still count
    // as spelling matches for a せいそう query and vice versa.
    let spellings: Vec<String> = entry
        .spellings
        .iter()
        .flat_map(|s| normalize::normalize_variants(s))
        .collect();
    let readings: Vec<String> = entry
        .readings
        .iter()
        .flat_map(|s| normalize::normalize_variants(s))
        .collect();

    // Primary means the key matches a variant of the first spelling; the
    // first spelling's own variants are checked, not just its raw form.
    let primary_variants = entry
        .spellings
        .first()
        .map(|s| normalize::normalize_variants(s))
        .unwrap_or_default();
    if primary_variants.iter().any(|s| s == key) {
        MatchKind::PrimarySpelling
    } else if spellings.iter().any(|s| s == key) {
        MatchKind::Spelling
    } else if readings.iter().any(|s| s == key) {
        MatchKind::Reading
    } else {
        // A literal match must have come from a spelling or reading, so this
        // arm only fires for deconjugation-reached entries in practice.
        MatchKind::Deconjugated
    }
}

/// Does any of this entry's readings match the reading the tokenizer assigned
/// to the surface in context (e.g. 前 read まえ in 前にある vs ぜん in 午前)?
pub(crate) fn reading_matches_context(entry: &DictEntry, context_reading: &str) -> bool {
    let ctx_variants = normalize::normalize_variants(context_reading);
    entry.readings.iter().any(|r| {
        let entry_variants = normalize::normalize_variants(r);
        ctx_variants
            .iter()
            .any(|c| entry_variants.iter().any(|e| e == c))
    })
}
