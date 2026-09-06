mod deconjugate;
mod normalize;
mod discord_rpc;
mod settings;

use settings::{get_settings, save_settings, SettingsState};
use deconjugate::{Deconjugator, DeconjugatedForm};
use normalize::normalize_variants;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tauri::Manager;
use vibrato::{Dictionary, Tokenizer};
use serde::Serialize;
use tauri_plugin_sql::{Migration, MigrationKind};
use zstd::Decoder;
use serde::Deserialize;
use std::collections::HashSet;
use discord_rpc::DiscordState;

#[derive(Serialize)]
struct TokenOut {
    surface: String,
    reading: String,
    pos: String,
    base_form: String,
}

struct TokenizerState(Mutex<Tokenizer>);

/// A single morphological token (vibrato/MeCab) with the fields needed for
/// lookups: character offsets, surface, dictionary base form, POS, and the
/// normalized (hiragana) reading MeCab assigned in context.
#[derive(Clone, serde::Serialize)]
struct MorphToken {
    start: usize,
    end: usize,
    surface: String,
    base_form: String,
    pos: String,
    reading: String,
}

/// Cache of sentence -> morphological tokens, so repeated lookups against the
/// same sentence (hover, cycle, scan) don't re-run the tokenizer.
struct MorphCacheState(Mutex<HashMap<String, Vec<MorphToken>>>);

#[derive(Deserialize, Serialize, Clone)]
struct DictEntry {
    id: u32,
    spellings: Vec<String>,
    readings: Vec<String>,
    definitions: Vec<String>,
    pos: Vec<String>,
    priority: Vec<String>,
}

struct DictionaryIndex {
    by_text: HashMap<String, Vec<Arc<DictEntry>>>,
    by_id: HashMap<u32, Arc<DictEntry>>,
    // Maps each bigram (and, for single-character text, each unigram) to
    // the set of entry ids that contain it somewhere in a spelling or
    // reading. Used to narrow "contains" searches to a small candidate
    // set instead of scanning every entry.
    by_bigram: HashMap<String, HashSet<u32>>,
}

fn bigrams(text: &str) -> Vec<String> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 2 {
        // single-character text: index the character itself so short
        // queries/spellings are still reachable
        return vec![chars.iter().collect()];
    }
    chars.windows(2).map(|w| w.iter().collect()).collect()
}

impl DictionaryIndex {
    fn build(entries: Vec<DictEntry>) -> Self {
        let mut by_text: HashMap<String, Vec<Arc<DictEntry>>> = HashMap::new();
        let mut by_id: HashMap<u32, Arc<DictEntry>> = HashMap::new();
        let mut by_bigram: HashMap<String, HashSet<u32>> = HashMap::new();

        for entry in entries {
            let entry = Arc::new(entry);

            for spelling in entry.spellings.iter().chain(entry.readings.iter()) {
                let key = normalize::normalize_text(spelling);
                by_text.entry(key.clone()).or_default().push(Arc::clone(&entry));

                for gram in bigrams(&key) {
                    by_bigram.entry(gram).or_default().insert(entry.id);
                }
            }

            by_id.insert(entry.id, Arc::clone(&entry));
        }

        Self { by_text, by_id, by_bigram }
    }
}

fn is_bound_only(entry: &DictEntry) -> bool {
    !entry.pos.is_empty()
        && entry.pos.iter().all(|p| p.eq_ignore_ascii_case("suffix") || p.eq_ignore_ascii_case("prefix"))
}

/// True for characters that should never take part in a looked-up span:
/// Japanese and ASCII punctuation plus whitespace. Excludes the katakana
/// chouonpu ー and the nakaguro ・, which are word-internal (コーヒー,
/// オブジェクト・指向). Used both to skip a cursor that lands on or after
/// leading punctuation (e.g. the ".." in "..苦労") and to trim trailing
/// punctuation from candidate spans.
fn is_punct_char(c: char) -> bool {
    matches!(
        c,
        '。' | '、' | '，' | '．' | '：' | '；' | '！' | '？' | '…' | '‥' | '〜' | '～'
            | '「' | '」' | '『' | '』' | '【' | '】' | '（' | '）' | '〔' | '〕'
            | '〈' | '〉' | '《' | '》' | '＝' | '＊' | '　'
            | ' ' | '\t' | '\n' | '\r'
            | ',' | '.' | ';' | ':' | '!' | '?' | '(' | ')' | '[' | ']' | '{' | '}'
            | '<' | '>' | '"' | '\'' | '`' | '~' | '^' | '*' | '-' | '_' | '+' | '='
            | '/' | '\\' | '|' | '@' | '#' | '$' | '%' | '&'
    )
}

fn priority_score(entry: &DictEntry) -> u16 {
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

fn find_containing(query: &str, index: &DictionaryIndex, limit: usize) -> Vec<Arc<DictEntry>> {
    let normalized = normalize::normalize_text(query);
    if normalized.is_empty() {
        return Vec::new();
    }

    let grams = bigrams(&normalized);

    // Intersect posting lists, starting from the smallest to minimize work.
    let mut posting_sets: Vec<&HashSet<u32>> = grams
        .iter()
        .filter_map(|g| index.by_bigram.get(g))
        .collect();

    if posting_sets.len() < grams.len() {
        // at least one bigram in the query doesn't exist anywhere in the
        // dictionary at all, so no entry can possibly contain the query
        return Vec::new();
    }

    posting_sets.sort_by_key(|s| s.len());

    let mut candidates: HashSet<u32> = posting_sets[0].clone();
    for set in &posting_sets[1..] {
        candidates.retain(|id| set.contains(id));
        if candidates.is_empty() {
            return Vec::new();
        }
    }

    let mut results: Vec<(Arc<DictEntry>, bool)> = Vec::new();
    for id in candidates {
        if let Some(entry) = index.by_id.get(&id) {
            let forms: Vec<String> = entry.spellings.iter().chain(entry.readings.iter())
                .map(|s| normalize::normalize_text(s))
                .collect();
    
            let is_exact = forms.iter().any(|f| f == &normalized);
            let actually_contains = is_exact || forms.iter().any(|f| f.contains(&normalized));
    
            if actually_contains {
                results.push((Arc::clone(entry), is_exact));
            }
        }
    }
    
    results.sort_by(|(a, a_exact), (b, b_exact)| {
        b_exact.cmp(a_exact)
            .then(priority_score(b).cmp(&priority_score(a)))
    });
    results.truncate(limit);
    
    results.into_iter().map(|(e, _)| e).collect()
}

struct DeconjRulesState(Deconjugator);

#[derive(serde::Serialize)]
struct MatchSpan {
    start: usize,
    end: usize,
    surface: String,
    entries: Vec<Arc<DictEntry>>,
    deconjugated_from: Option<String>,
    related_entries: Vec<Arc<DictEntry>>, // entries containing `surface`, excluding exact matches already in `entries`
}

// Character count (not morpheme count) a phrase match can span. This is
// purely a performance/sanity cap on how far the longest-match scan looks
// ahead from a given position — it is NOT a linguistic boundary. JL does
// not use POS tagging or any tokenizer to decide where a match is allowed
// to end; the dictionary (plus deconjugation) is the only thing that
// decides that. Whatever doesn't resolve to a real entry at a given
// length just falls through to a shorter candidate at the same position.
const MAX_CHARS_COMBINED: usize = 16;

/// Mirrors JL's actual interaction model: JL does not pre-segment or
/// pre-highlight a whole sentence. It resolves exactly one match, starting
/// at the exact character position the user is pointing at (mouse
/// position / cursor / click), by trying the longest candidate substring
/// first and shrinking one character at a time until something resolves
/// against the dictionary (literally or via deconjugation). Nothing is
/// computed for the rest of the text — if the guess is wrong, the user
/// just points one character over and a fresh lookup runs from there.
///
/// `skip` selects which successful match to return, counting from longest
/// (skip = 0) downward — e.g. if 今日は, 今日, and 今 are all separately
/// in the dictionary, skip=1 returns 今日 and skip=2 returns 今, letting
/// a shorter word that a longer match "swallows" still be reached from
/// the same starting character (JL/Yomitan expose this as a
/// cycle-to-shorter-candidate hotkey rather than making longest-match
/// smarter, since there's no general way to know which length the user
/// actually wants).
///
/// Returns `None` if `position` is out of bounds, or if `skip` asks for
/// more candidates than exist at this position (the caller should treat
/// that as "wrap back to skip = 0"). A position with no dictionary/
/// deconjugation match at all still returns `Some` at skip = 0, as a
/// one-character span with empty `entries`.
fn lookup_from_position(
    text: &str,
    mut position: usize,
    skip: usize,
    index: &DictionaryIndex,
    decon: &Deconjugator,
    tokens: &[MorphToken],
) -> Option<MatchSpan> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    if position >= len {
        return None;
    }

    // Punctuation never participates in a span. If the cursor lands on or
    // after leading punctuation (e.g. the ".." in "..苦労", a comma, an
    // opening bracket), skip forward to the first content character so the
    // span matches only the actual word.
    while position < len && is_punct_char(chars[position]) {
        position += 1;
    }
    if position >= len {
        return None;
    }

    // The token at (or containing) the cursor gives the in-context reading,
    // used to order kanji homographs (前 -> まえ in 前にある, ぜん inside 午前).
    // The base form is only used when the cursor is at the very start of a
    // verb token, since that's when the whole token's conjugation is what the
    // user is looking at (e.g. the し of します -> する).
    let token_at_pos = tokens.iter().find(|t| position >= t.start && position < t.end);
    let context_reading = token_at_pos
        .and_then(|t| {
            if t.reading.is_empty() {
                None
            } else {
                Some(t.reading.as_str())
            }
        });
    let morph_base = tokens
        .iter()
        .find(|t| t.start == position)
        .and_then(|t| {
            if t.pos == "動詞" && t.base_form != t.surface {
                Some(t.base_form.as_str())
            } else {
                None
            }
        });

    // Punctuation never forms a span.
    if let Some(t) = token_at_pos {
        if t.pos == "記号" {
            return None;
        }
    }

    // Function words (particles が/を/は/も/に/の/と/で, auxiliaries
    // ます/た/だ/ん/たい, conjunctions) are themselves dictionary entries
    // (が -> 蛾, ます -> 鱒) and so stay lookup-able — but only as their own
    // single token. They must never extend into the next word, which is what
    // produced があ, をして, もできる, はしません, にさせたい, もない,
    // はよ, のこと and なんだ -> 涙.
    let function_word = token_at_pos
        .map(|t| matches!(t.pos.as_str(), "助詞" | "助動詞" | "接続詞"))
        .unwrap_or(false);

    // Candidate spans are token-aligned: sub-spans inside the token at the
    // cursor (so 今 can still be reached inside 今日 for the skip feature)
    // plus whole-token extensions across following tokens. Spans never end
    // mid-way through a later token, which is what produced があ / はよ /
    // のこ. Function words never extend beyond their own token.
    let mut ends: Vec<usize> = Vec::new();
    if let Some(t) = token_at_pos {
        for e in (position + 1)..=t.end.min(len) {
            ends.push(e);
        }
        // If the cursor is on an unknown token (empty reading — katakana slang
        // like マズ), allow the span to continue character-by-character into the
        // next token so a real literal word can still form across the false
        // boundary: マズ + い -> マズい (まずい). Normally spans never end
        // mid-token, but an unknown cursor token is a safe exception: the
        // empty-reading fallback already routes resolution through surface
        // deconjugation, so no function-word merge (があ / はよ / のこ) can appear.
        if t.reading.is_empty() {
            if let Some(next) = tokens.iter().find(|tok| tok.start > position) {
                for e in (t.end + 1)..=next.end.min(len) {
                    ends.push(e);
                }
            }
        }
        if !function_word {
            for tok in tokens.iter().filter(|tok| tok.start > position) {
                // Trailing punctuation and unknown tokens (emphatic kana
                // like ぅぇぁ, stray ー, dot runs, etc. — MeCab tags them
                // base "*" with an empty reading) never extend a span; they
                // only produced 疲れるぅ -> つく and 苦労…… -> 繰る.
                if tok.pos == "記号" || tok.base_form == "*" {
                    break;
                }
                let e = tok.end.min(len);
                if e > position {
                    ends.push(e);
                }
            }
        }
    } else {
        for e in (position + 1)..=len {
            ends.push(e);
        }
    }
    ends.sort_unstable();
    ends.dedup();
    ends.retain(|e| *e <= position + MAX_CHARS_COMBINED);

    let mut found = 0usize;
    for &end in ends.iter().rev() {
        // Trim trailing punctuation so a candidate never ends in 記号/UNK
        // characters absorbed into the cursor token (e.g. 苦労。 when MeCab
        // merges them). This pairs with the forward-extension break above.
        let mut eff_end = end;
        while eff_end > position && is_punct_char(chars[eff_end - 1]) {
            eff_end -= 1;
        }
        if eff_end <= position {
            continue;
        }
        let candidate: String = chars[position..eff_end].iter().collect();
        if let Some((entries, deconj_info)) =
            lookup_candidate(&candidate, index, decon, context_reading, morph_base, tokens, position)
        {
            if found == skip {
                // ── NEW: compute related entries before returning ──
                let exact_ids: HashSet<u32> = entries.iter().map(|e| e.id).collect();
                let related = find_containing(&candidate, index, 20)
                    .into_iter()
                    .filter(|e| !exact_ids.contains(&e.id))
                    .collect();

                return Some(MatchSpan {
                    start: position,
                    end: eff_end,
                    surface: candidate,
                    entries,
                    deconjugated_from: deconj_info,
                    related_entries: related, // NEW field
                });
            }
            found += 1;
        }
    }

    if skip == 0 {
        let surface: String = chars[position..position + 1].iter().collect();

        // ── NEW: related entries for the no-match fallback too ──
        let related = find_containing(&surface, index, 20);

        return Some(MatchSpan {
            start: position,
            end: position + 1,
            surface,
            entries: vec![],
            deconjugated_from: None,
            related_entries: related, // NEW field
        });
    }

    None
}



/// Tries every normalized variant of `candidate` (there can be more than
/// one due to chouonpu ambiguity — see normalize::chouonpu_variants)
/// against the dictionary index: literal match first, then deconjugation.
/// Deconjugation uses the JL/Nazeka engine, which records each resolved
/// form with the fewest proper rule steps per (text, word class), and the
/// resulting word class is validated against each entry's POS (JL's
/// GetValidDeconjugatedResults) so a coincidental conjugation can't surface
/// a wrong homograph.

/// How an entry was reached for a given surface. Used as a tie-breaker so a
/// direct spelling match outranks a homophone reached only through an
/// alternate spelling or a reading — e.g. 前(まえ) beats 先(さき) when both
/// match the surface 前, since 先 merely lists 前 as a secondary spelling.
/// Ordering here (lower = better) is the sort precedence.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum MatchKind {
    PrimarySpelling, // normalized surface == entry's primary (first) spelling
    Spelling,        // normalized surface == some other spelling
    Morphological,   // reached via the tokenizer's base form (e.g. します -> する)
    Reading,         // normalized surface == a reading
    Deconjugated,    // reached by deconjugating a conjugated surface
}

fn match_kind(entry: &DictEntry, key: &str) -> MatchKind {
    let spellings: Vec<String> =
        entry.spellings.iter().map(|s| normalize::normalize_text(s)).collect();
    let readings: Vec<String> =
        entry.readings.iter().map(|s| normalize::normalize_text(s)).collect();

    if spellings.first().map(String::as_str) == Some(key) {
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
fn reading_matches_context(entry: &DictEntry, context_reading: &str) -> bool {
    entry
        .readings
        .iter()
        .any(|r| normalize::normalize_text(r) == context_reading)
}

/// Maps a JL/Nazeka deconjugation tag to the JMdict English POS labels an
/// entry must carry for the deconjugated result to be valid (JL's
/// GetValidDeconjugatedResults). "any" (tomoyo's supplementary rules) and
/// unknown tags are POS-unrestricted.
fn deconj_tag_to_dict_pos(tag: &str) -> &'static [&'static str] {
    match tag {
        "v1" => &["Ichidan verb"],
        "v1-s" => &["Ichidan verb - kureru special class"],
        "v4r" => &["Yodan verb with 'ru' ending (archaic)"],
        "v5aru" => &["Godan verb - -aru special class"],
        "v5b" => &["Godan verb with 'bu' ending"],
        "v5g" => &["Godan verb with 'gu' ending"],
        "v5k" => &["Godan verb with 'ku' ending"],
        "v5k-s" => &["Godan verb - Iku/Yuku special class"],
        "v5m" => &["Godan verb with 'mu' ending"],
        "v5n" => &["Godan verb with 'nu' ending"],
        "v5r" => &["Godan verb with 'ru' ending"],
        "v5r-i" => &["Godan verb with 'ru' ending (irregular verb)"],
        "v5s" => &["Godan verb with 'su' ending"],
        "v5t" => &["Godan verb with 'tsu' ending"],
        "v5u" => &["Godan verb with 'u' ending"],
        "v5u-s" => &["Godan verb with 'u' ending (special class)"],
        "vk" => &["Kuru verb - special class"],
        "vs-c" => &["su verb - precursor to the modern suru"],
        "vs-i" => &["suru verb - included"],
        "vs-s" => &["suru verb - special class"],
        "vz" => &["Ichidan verb - zuru verb (alternative form of -jiru verbs)"],
        "adj-i" => &["adjective (keiyoushi)"],
        "adj-ix" => &["'ku' adjective (archaic)", "'shiku' adjective (archaic)"],
        "cop" => &["copula"],
        _ => &[],
    }
}

fn deconj_tag_matches_entry(entry_pos: &[String], tag: &str) -> bool {
    let allowed = deconj_tag_to_dict_pos(tag);
    allowed.is_empty() || entry_pos.iter().any(|p| allowed.contains(&p.as_str()))
}

/// Verb word classes the deconjugation rules can claim. Deconjugation
/// results in one of these are only trusted when the span actually contains
/// a 動詞 token — otherwise a noun/na-adjective + な (好きな) deconjugates
/// through the imperative な rule into a coincidental verb (好く).
fn is_verb_class(tag: &str) -> bool {
    matches!(
        tag,
        "v1" | "v1-s"
            | "v4r"
            | "v5aru"
            | "v5b"
            | "v5g"
            | "v5k"
            | "v5k-s"
            | "v5m"
            | "v5n"
            | "v5r"
            | "v5r-i"
            | "v5s"
            | "v5t"
            | "v5u"
            | "v5u-s"
            | "vk"
            | "vs-c"
            | "vs-i"
            | "vs-s"
            | "vz"
    )
}

/// Jargon-y JL rule names for sound changes and auxiliary helpers that say
/// nothing about the surface form — excluded from combined labels so
/// してくれました reads "polite past" rather than "polite past +
/// statement/request + unstressed infinitive".
fn is_stem_jargon(detail: &str) -> bool {
    matches!(
        detail,
        // Auxiliary helpers and sound changes that say nothing about the
        // surface form.
        "statement/request"
            | "slurred"
            | "slurred negative"
            | "rough casual"
            | "ksb"
            | "contracted"
            // JL's parenthetical stem notes, which chain_description has
            // already stripped of their parentheses.
            | "masu stem"
            | "unstressed infinitive"
            | "stem"
            | "adverbial stem"
            | "izenkei"
            | "ka stem"
            | "ke stem"
            | "mizenkei"
            | "'a' stem"
    )
}

/// Shorter, plainer names for verbose JL rule details.
fn curated_name(detail: &str) -> &str {
    match detail {
        "finish/completely/end up" => "ended up",
        "passive/potential/honorific" | "passive/potential" => "potential",
        "toku (for now)" => "in advance (casual)",
        other => other,
    }
}

/// Names the surface's conjugation from a deconjugation rule chain by
/// combining every meaningful rule that applied, outermost first — e.g.
/// 住んでいた -> "past + teiru", 忘れてしまった -> "past + ended up".
/// Parenthetical stem notes ("(masu stem)") are skipped, except a leading
/// one like (te) in 飲んで, which is the surface conjugation itself.
fn combined_label(chain: &str) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for (i, detail) in chain.split('→').enumerate() {
        if detail.is_empty() {
            continue;
        }
        if detail.starts_with('(') {
            if i == 0 && detail.len() >= 2 && detail.ends_with(')') {
                parts.push(detail[1..detail.len() - 1].to_string());
            }
            continue;
        }
        if is_stem_jargon(detail) {
            continue;
        }
        parts.push(curated_name(detail).to_string());
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" + "))
    }
}

fn lookup_candidate(
    candidate: &str,
    index: &DictionaryIndex,
    decon: &Deconjugator,
    context_reading: Option<&str>,
    morph_base: Option<&str>,
    tokens: &[MorphToken],
    position: usize,
) -> Option<(Vec<Arc<DictEntry>>, Option<String>)> {
    let variants = normalize_variants(candidate);
    let span_len = candidate.chars().count();

    // The candidate's reading (concatenated token readings) when it is
    // token-aligned. JL deconjugates the reading, not the surface, which is
    // what keeps 入ってこない -> 入る resolving to the はいる reading instead
    // of matching both homograph readings of 入る. The reading is only used
    // when a token starts exactly at the cursor: a span that begins
    // mid-token (e.g. the き of とき inside the single token とき) has no
    // honest reading — deconjugating its tokens produced きました -> ます
    // (増す) when the surface きました correctly resolves to くる. Sub-span
    // candidates therefore fall back to surface deconjugation.
    let span_reading: Option<String> = {
        let starts_at_position = tokens.iter().any(|t| t.start == position);
        if !starts_at_position {
            None
        } else {
            // If the token at the cursor is unknown (MeCab gives it no reading
            // because it isn't in its dictionary — katakana names/slang like
            // ジイ or マズ), the concatenated span reading silently drops that
            // prefix and deconjugates just the tail, producing dishonest
            // results (ジイさんじゃない -> さん, マズいんだ -> いぬ). Falls back to
            // surface deconjugation on the full normalized form instead, which
            // resolves じいさん (爺さん) / まずい (不味い) correctly.
            let cursor_tok = tokens.iter().find(|t| t.start == position);
            if cursor_tok.map_or(false, |t| t.reading.is_empty()) {
                None
            } else {
                let in_span: Vec<&MorphToken> = tokens
                    .iter()
                    .filter(|t| t.start >= position && t.end <= position + span_len)
                    .collect();
                if in_span.is_empty() {
                    None
                } else {
                    Some(in_span.iter().map(|t| t.reading.as_str()).collect())
                }
            }
        }
    };

    // Rule-based deconjugation forms for this surface, computed once so the
    // morphological candidate below can check whether the surface actually
    // conjugates.
    let deconj_forms: Vec<DeconjugatedForm> = match &span_reading {
        Some(reading) => decon.deconjugate(reading),
        None => variants.iter().flat_map(|key| decon.deconjugate(key)).collect(),
    };

    // (entry, chain_len, chain_description, kind, context_match)
    let mut candidates: Vec<(Arc<DictEntry>, usize, Option<String>, MatchKind, bool)> = Vec::new();
    let mut seen_ids: HashSet<u32> = HashSet::new();

    // Literal matches first — inserted with chain_len 0, so they win ties
    // against equal-priority deconjugated results, same as before.
    for key in &variants {
        if let Some(entries) = index.by_text.get(key) {
            for e in entries {
                if seen_ids.insert(e.id) {
                    let kind = match_kind(e, key);
                    let ctx = context_reading.map_or(false, |r| reading_matches_context(e, r));
                    candidates.push((Arc::clone(e), 0, None, kind, ctx));
                }
            }
        }
    }

    // Morphological matches — the tokenizer's base form for the verb at the
    // cursor. High confidence because MeCab resolved the actual conjugation
    // (します -> し -> する), so it outranks rule-based deconjugation, which can
    // guess a wrong coincidental form (しる/知る for します). Two gates keep it
    // from absorbing unrelated morphemes:
    //   1. the base form is already a deconjugation result for this surface
    //      (fixes kana きませんでした -> くる);
    //   2. or the candidate is the verb token followed only by
    //      auxiliary/particle tokens (fixes 食べられます -> 食べる).
    if let Some(base) = morph_base {
        let base_norm = normalize::normalize_text(base);
        let via_deconj = deconj_forms.iter().any(|f| normalize::normalize_text(&f.text) == base_norm);
        let via_aux_tail = tokens
            .iter()
            .filter(|t| t.start >= position && t.start < position + span_len && t.start != position)
            .all(|t| matches!(t.pos.as_str(), "助動詞" | "助詞" | "記号" | "接頭辞" | "接尾辞"));
        if via_deconj || via_aux_tail {
            // Name the deconjugation (e.g. した -> "past") rather than the bare
            // base form in the tooltip. The deconjugation forms are kana while
            // the base form may be kanji (のむ vs 飲む), so they are matched by
            // the dictionary entry they resolve to, not by raw text.
            let base_ids: HashSet<u32> = index
                .by_text
                .get(&base_norm)
                .map(|es| es.iter().map(|e| e.id).collect())
                .unwrap_or_default();
            let resolves_to_base = |f: &DeconjugatedForm| {
                index
                    .by_text
                    .get(&normalize::normalize_text(&f.text))
                    .map_or(false, |es| es.iter().any(|e| base_ids.contains(&e.id)))
            };
            // The span reading, then the same reading with trailing particle
            // tokens dropped (から in 飲んでから) so the te-form is still
            // reachable for the label.
            let mut label_readings: Vec<String> = Vec::new();
            if let Some(reading) = &span_reading {
                label_readings.push(reading.clone());
                let mut in_span: Vec<&MorphToken> = tokens
                    .iter()
                    .filter(|t| t.start >= position && t.end <= position + span_len)
                    .collect();
                while let Some(&last) = in_span.last() {
                    if last.pos != "助詞" {
                        break;
                    }
                    // A 助詞 directly after a 動詞 token is the te-form て/で,
                    // part of the conjugation — not a particle to strip.
                    if in_span.iter().rev().skip(1).next().map_or(false, |t| t.pos == "動詞") {
                        break;
                    }
                    in_span.pop();
                }
                let stripped: String = in_span.iter().map(|t| t.reading.as_str()).collect();
                if stripped != *reading {
                    label_readings.push(stripped);
                }
            }
            let mut label: Option<String> = None;
            for reading in &label_readings {
                if let Some(chain) = decon
                    .deconjugate(reading)
                    .iter()
                    .find(|f| resolves_to_base(f))
                    .and_then(|f| f.rule_chain.as_deref())
                {
                    label = combined_label(chain);
                    if label.is_some() {
                        break;
                    }
                }
            }
            // てから / でから (te-form + the particle から) is a grammatical
            // rule meaning "after doing" — it outranks the plain te-form name.
            let te_kara = span_reading
                .as_deref()
                .map_or(false, |r| r.ends_with("てから") || r.ends_with("でから"));
            let label = if te_kara {
                "after doing".to_string()
            } else {
                // When no rule chain reaches the base form (はじめます ->
                // はじめる dead-ends at the unrecordable ます-stem), still
                // name the surface conjugation from the first rule applied
                // (polite).
                label
                    .or_else(|| label_readings.last().and_then(|r| decon.first_rule(r)))
                    .unwrap_or_else(|| base.to_string())
            };
            if let Some(entries) = index.by_text.get(&base_norm) {
                for e in entries {
                    if seen_ids.insert(e.id) {
                        let ctx = context_reading.map_or(false, |r| reading_matches_context(e, r));
                        candidates.push((Arc::clone(e), 1, Some(label.clone()), MatchKind::Morphological, ctx));
                    }
                }
            }
        }
    }

    // Suru-verb nouns — a 名詞 (調査) followed by the suru verb (する/し/して/
    // している) and then only auxiliaries/particles. The dictionary headword
    // is the noun itself ("調査" carries "noun or participle which takes the
    // aux. verb suru"), so 調査している must resolve to 調査 rather than a
    // coincidental rule-based deconjugation of the whole katakana reading —
    // ちょうさしている also yields ちょうする (弔する/徴する), which is wrong.
    // The noun's own POS validates the suru construction, mirroring how the
    // verb case above trusts the tokenizer's base form.
    let noun_pos_entries: Vec<Arc<DictEntry>> = {
        let noun_tok = tokens.iter().find(|t| t.start == position);
        match noun_tok {
            Some(noun) if noun.pos == "名詞" => {
                let noun_norm = normalize::normalize_text(&noun.surface);
                let entries = index.by_text.get(&noun_norm).cloned().unwrap_or_default();
                if entries.iter().any(|e| e.pos.iter().any(|p| p.contains("takes the aux. verb suru") || p.contains("suru verb"))) {
                    let suru_start = noun.end;
                    let suru = tokens.iter().find(|t| t.start == suru_start);
                    match suru {
                        Some(s) if s.base_form == "する" => {
                            let tail_ok = tokens
                                .iter()
                                .filter(|t| t.start > suru_start && t.start < position + span_len)
                                .all(|t| matches!(t.pos.as_str(), "助動詞" | "助詞" | "記号" | "接頭辞" | "接尾辞" | "動詞"));
                            if tail_ok { entries } else { Vec::new() }
                        }
                        _ => Vec::new(),
                    }
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    };
    if !noun_pos_entries.is_empty() {
        // Label the suru construction by deconjugating the reading from the
        // suru token onward (し+て+いる -> "teiru", し+た -> "past", ...),
        // excluding the leading noun token itself.
        let suru_reading: String = tokens
            .iter()
            .filter(|t| t.start >= position && t.end <= position + span_len)
            .skip_while(|t| t.base_form != "する")
            .map(|t| t.reading.as_str())
            .collect();
        let label = decon
            .deconjugate(&suru_reading)
            .iter()
            .find(|f| normalize::normalize_text(&f.text) == "する")
            .and_then(|f| f.rule_chain.as_deref())
            .and_then(combined_label)
            .unwrap_or_else(|| "suru".to_string());
        for e in noun_pos_entries {
            if seen_ids.insert(e.id) {
                let ctx = context_reading.map_or(false, |r| reading_matches_context(&e, r));
                candidates.push((Arc::clone(&e), 1, Some(label.clone()), MatchKind::Morphological, ctx));
            }
        }
    }

    // Rule-based deconjugation — fallback beneath morphology. Entries already
    // found via literal/morphological paths are skipped via seen_ids, so a
    // word never appears twice just because both paths resolved to it. Each
    // deconjugation result is also validated against the entry's POS (the
    // rule's word class must appear among the entry's parts of speech), so a
    // coincidental conjugation like しる -> 知る (v5r) never surfaces.
    let starts_at_position = tokens.iter().any(|t| t.start == position);
    let span_has_verb_token = tokens
        .iter()
        .any(|t| t.start >= position && t.start < position + span_len && t.pos == "動詞");
    for form in &deconj_forms {
        // Verb-class results need a real verb token backing them (see
        // is_verb_class) — otherwise 好きな (名詞+な) resolves to 好く via the
        // imperative な rule. Sub-span candidates (cursor mid-token, e.g. the
        // き of きませんでした) are exempt: their tokens aren't aligned with
        // the conjugation, so the deconjugation itself is the best evidence.
        if starts_at_position && is_verb_class(&form.tag) && !span_has_verb_token {
            continue;
        }
        let key = normalize::normalize_text(&form.text);
        if let Some(entries) = index.by_text.get(&key) {
            let chain_desc = form.rule_chain.as_deref().and_then(combined_label);
            for e in entries {
                if deconj_tag_matches_entry(&e.pos, &form.tag) && seen_ids.insert(e.id) {
                    let ctx = context_reading.map_or(false, |r| reading_matches_context(e, r));
                    candidates.push((
                        Arc::clone(e),
                        form.proper_steps,
                        chain_desc.clone(),
                        MatchKind::Deconjugated,
                        ctx,
                    ));
                }
            }
        }
    }

    if candidates.is_empty() {
        return None;
    }

    // Sort: how the entry was reached (literal spelling > morphological
    // base form > reading > rule deconjugation) first, then whether its
    // reading matches the in-context reading. Among the remaining ties,
    // entries with no dictionary priority at all ("orphans" like the
    // intermediate potential 食べられる or the causative homophone 知らす)
    // never beat a common word — they're coincidental deconjugation results,
    // so 食べられます -> 食べる and 知らされなかった -> 知る keep winning by
    // frequency. Among common words, fewest deconjugation steps decides (JL
    // ranks MinDeconjugationProcessStepCount before frequency), which is what
    // gives 惹かれております -> 惹かれる over the higher-frequency 光る/引く
    // (ひかれる 3 steps vs ひかる/ひく 4). Then priority, then whether the
    // entry's spelling matches the tokenizer's base form (行かせられなかった
    // -> 行く, where 行く and 生かす tie on steps and priority), then
    // non-bound entries.
    candidates.sort_by(|a, b| {
        let a_prio = priority_score(&a.0);
        let b_prio = priority_score(&b.0);
        let a_orphan = a_prio == 0;
        let b_orphan = b_prio == 0;
        let a_base_match = match morph_base {
            Some(base) => a.0.spellings.iter().any(|s| normalize::normalize_text(s) == base),
            None => false,
        };
        let b_base_match = match morph_base {
            Some(base) => b.0.spellings.iter().any(|s| normalize::normalize_text(s) == base),
            None => false,
        };
        a.3.cmp(&b.3)
            .then(b.4.cmp(&a.4)) // context-match: true first
            .then(a_orphan.cmp(&b_orphan)) // common word first
            .then(a.1.cmp(&b.1)) // fewest deconj steps first
            .then(b_prio.cmp(&a_prio))
            .then(b_base_match.cmp(&a_base_match)) // morph-base spelling: true first
            .then(is_bound_only(&a.0).cmp(&is_bound_only(&b.0))) // false (not bound) sorts before true
    });

    let deconjugated_from = candidates[0].2.clone();
    let entries: Vec<Arc<DictEntry>> = candidates.into_iter().map(|(e, _, _, _, _)| e).collect();

    Some((entries, deconjugated_from))
}

struct DictState(DictionaryIndex);

fn tokenize_tokens(tokenizer_mutex: &Mutex<Tokenizer>, text: &str) -> Vec<MorphToken> {
    let tokenizer = tokenizer_mutex.lock().unwrap();
    let mut worker = tokenizer.new_worker();
    worker.reset_sentence(text);
    worker.tokenize();

    worker
        .token_iter()
        .map(|t| {
            let range = t.range_char();
            let feature = t.feature(); // comma-separated MeCab features
            let fields: Vec<&str> = feature.split(',').collect();
            MorphToken {
                start: range.start,
                end: range.end,
                surface: t.surface().to_string(),
                base_form: fields.get(6).map(|s| s.to_string()).unwrap_or_else(|| t.surface().to_string()),
                pos: fields.get(0).unwrap_or(&"").to_string(),
                // readings come out in katakana; normalize to hiragana so they
                // can be compared against dictionary readings.
                reading: normalize::normalize_text(fields.get(7).unwrap_or(&"")),
            }
        })
        .collect()
}

fn tokenize_cached(
    cache_state: &Mutex<HashMap<String, Vec<MorphToken>>>,
    tokenizer_state: &Mutex<Tokenizer>,
    text: &str,
) -> Vec<MorphToken> {
    if let Some(tokens) = cache_state.lock().unwrap().get(text) {
        return tokens.clone();
    }

    let tokens = tokenize_tokens(tokenizer_state, text);

    let mut cache = cache_state.lock().unwrap();
    if cache.len() > 200 {
        cache.clear();
    }
    cache.insert(text.to_string(), tokens.clone());
    tokens
}

#[tauri::command]
fn lookup_at_position(
    dict_state: tauri::State<DictState>,
    decon_state: tauri::State<DeconjRulesState>,
    morph_cache: tauri::State<MorphCacheState>,
    tokenizer_state: tauri::State<TokenizerState>,
    text: String,
    position: usize,
    skip: usize,
) -> Option<MatchSpan> {
    let tokens = tokenize_cached(&morph_cache.0, &tokenizer_state.0, &text);
    lookup_from_position(&text, position, skip, &dict_state.0, &decon_state.0, &tokens)
}

/// Morphological tokens for a whole sentence (with char offsets), for
/// frontend consumers that need grammar-aware segmentation.
#[tauri::command]
fn tokenize_sentence(
    morph_cache: tauri::State<MorphCacheState>,
    tokenizer_state: tauri::State<TokenizerState>,
    text: String,
) -> Vec<MorphToken> {
    tokenize_cached(&morph_cache.0, &tokenizer_state.0, &text)
}

/// Scans a whole sentence into dictionary/deconjugation spans in one IPC
/// round-trip, using the same longest-match resolution as hover but sharing a
/// single tokenization. Replaces the frontend's per-character lookup loop.
#[tauri::command]
fn scan_sentence(
    dict_state: tauri::State<DictState>,
    decon_state: tauri::State<DeconjRulesState>,
    morph_cache: tauri::State<MorphCacheState>,
    tokenizer_state: tauri::State<TokenizerState>,
    text: String,
) -> Vec<MatchSpan> {
    let tokens = tokenize_cached(&morph_cache.0, &tokenizer_state.0, &text);
    let chars: Vec<char> = text.chars().collect();
    let mut spans = Vec::new();
    let mut pos = 0usize;
    while pos < chars.len() {
        if let Some(span) = lookup_from_position(&text, pos, 0, &dict_state.0, &decon_state.0, &tokens) {
            if !span.entries.is_empty() {
                let end = span.end;
                spans.push(span);
                pos = end.max(pos + 1);
                continue;
            }
        }
        // No useful span at this position (punctuation, or a no-match
        // placeholder) — jump to the end of the token covering `pos` so a
        // skipped function word never leaves a dangling mid-token cursor.
        let next = tokens
            .iter()
            .find(|t| t.start <= pos && pos < t.end)
            .or_else(|| tokens.iter().find(|t| t.start >= pos))
            .map(|t| t.end);
        pos = next.map(|e| e.max(pos + 1)).unwrap_or(pos + 1);
    }
    spans
}

#[tauri::command]
fn tokenize_text(state: tauri::State<TokenizerState>, text: String) -> Vec<TokenOut> {
    let tokenizer = state.0.lock().unwrap();
    let mut worker = tokenizer.new_worker();
    worker.reset_sentence(&text);
    worker.tokenize();

    worker
        .token_iter()
        .map(|t| {
            let feature = t.feature(); // comma-separated MeCab features
            let fields: Vec<&str> = feature.split(',').collect();
            TokenOut {
                surface: t.surface().to_string(),
                reading: fields.get(7).unwrap_or(&"").to_string(), // reading field position varies by dict
                pos: fields.get(0).unwrap_or(&"").to_string(),
                base_form: fields.get(6).unwrap_or(&t.surface()).to_string(),
            }
        })
        .collect()
}

#[tauri::command]
fn export_database(app: tauri::AppHandle, dest: String) -> Result<(), String> {
    let db_path = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("immersion.db");
    std::fs::copy(&db_path, &dest).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn import_database(app: tauri::AppHandle, source: String) -> Result<(), String> {
    let header = std::fs::read(&source).map_err(|e| e.to_string())?;
    if header.len() < 16 || &header[..16] != b"SQLite format 3\0" {
        return Err("Selected file is not a valid SQLite database".into());
    }

    let db_path = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("immersion.db");

    // Remove sidecar files left behind by a previous connection so the
    // fresh copy starts clean (especially after a crash).
    for suffix in ["-journal", "-wal", "-shm"] {
        let _ = std::fs::remove_file(format!("{}{}", db_path.display(), suffix));
    }

    std::fs::copy(&source, &db_path).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn restart_app(app: tauri::AppHandle) {
    app.restart();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let migrations = vec![
        Migration {
            version: 1,
            description: "create_media_table",
            sql: include_str!("../migrations/0001_media.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 2,
            description: "words_and_sentences",
            sql: include_str!("../migrations/0002_words_and_sentences.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 3,
            description: "events_and_sessions",
            sql: include_str!("../migrations/0003_events_and_sessions.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 4,
            description: "sessions_last_updated",
            sql: include_str!("../migrations/0004_sessions_last_updated.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 5,
            description: "lookup_events_new",
            sql: include_str!("../migrations/0005_lookup_events_new.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 6,
            description: "word_status",
            sql: include_str!("../migrations/0006_word_status.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 7,
            description: "dismissed_unknown_words",
            sql: include_str!("../migrations/0007_dismissed_unknown_words.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 8,
            description: "only_media_tag",
            sql: include_str!("../migrations/0008_only_media_tag.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 9,
            description: "tag_rewrite",
            sql: include_str!("../migrations/0009_tag_rewrite.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 10,
            description: "reviews",
            sql: include_str!("../migrations/0010_reviews.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 11,
            description: "sentences_read_events",
            sql: include_str!("../migrations/0011_sentences_read_events.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 12,
            description: "vndb_id",
            sql: include_str!("../migrations/0012_vndb_id.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 13,
            description: "session_links",
            sql: include_str!("../migrations/0013_session_links.sql"),
            kind: MigrationKind::Up,
        },
    ];

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_http::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations("sqlite:immersion.db", migrations)
                .build(),
        )
        .manage(DiscordState::new())
        .manage(MorphCacheState(Mutex::new(HashMap::new())))
        .invoke_handler(tauri::generate_handler![
            discord_rpc::connect_discord,
            discord_rpc::update_discord_presence,
            discord_rpc::disconnect_discord,
            tokenize_text, tokenize_sentence, scan_sentence, lookup_at_position,
            get_settings, save_settings,
            export_database, import_database, restart_app,
        ])
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();

            #[cfg(target_os = "windows")]
            window.set_decorations(true)?;

            #[cfg(target_os = "linux")]
            window.set_decorations(false)?;

            let main_window = app.get_webview_window("main").unwrap();
                let app_handle = app.handle().clone();
            
                main_window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { .. } = event {
                        if let Some(discord_state) = app_handle.try_state::<discord_rpc::DiscordState>() {
                            let _ = discord_state.disconnect();
                        }
                        
                        std::thread::sleep(std::time::Duration::from_millis(100));
                        app_handle.exit(0);
                    }
                });

            // ── Tokenizer (Vibrato) — used by tokenize_text/tokenize_sentence
            // and (via the cached tokens + base-form/context-reading info)
            // by lookup_at_position and scan_sentence. lookup_at_position
            // still resolves spans from the dictionary index, but morphology
            // informs the reading and base-form candidates. ──
            let resource_path = app
                .path()
                .resolve("resources/ipadic-mecab.dic.zst", tauri::path::BaseDirectory::Resource)?;

            let reader = Decoder::new(std::fs::File::open(resource_path)?)?;
            let dict = Dictionary::read(reader)?;
            let tokenizer = Tokenizer::new(dict);
            app.manage(TokenizerState(Mutex::new(tokenizer)));

            // ── Dictionary index (JMdict) ──
            let jmdict_path = app
                .path()
                .resolve("resources/jmdict.json", tauri::path::BaseDirectory::Resource)?;

            let jmdict_json = std::fs::read_to_string(jmdict_path)?;
            let entries: Vec<DictEntry> = serde_json::from_str(&jmdict_json)?;
            let dictionary_index = DictionaryIndex::build(entries);
            app.manage(DictState(dictionary_index));
            app.manage(DeconjRulesState(Deconjugator::build(include_str!(
                "../resources/deconjugation_rules.json"
            ))));

            let initial_settings = settings::load_settings_from_disk(&app.handle());
            app.manage(SettingsState(Mutex::new(initial_settings)));

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod lookup_tests {
    use super::*;
    use std::io::Read;
    use vibrato::{Dictionary, Tokenizer};
    use zstd::Decoder;

    pub(crate) struct Harness {
        pub(crate) index: DictionaryIndex,
        pub(crate) decon: Deconjugator,
        pub(crate) tokenizer: Tokenizer,
    }

    impl Harness {
        pub(crate) fn new() -> Self {
            let file = std::fs::File::open("resources/ipadic-mecab.dic.zst").unwrap();
            let mut reader = Decoder::new(file).unwrap();
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf).unwrap();
            let dict = Dictionary::read(&buf[..]).unwrap();
            let tokenizer = Tokenizer::new(dict);

            let jmdict_json = std::fs::read_to_string("resources/jmdict.json").unwrap();
            let entries: Vec<DictEntry> = serde_json::from_str(&jmdict_json).unwrap();
            let index = DictionaryIndex::build(entries);
            let decon = Deconjugator::build(include_str!("../resources/deconjugation_rules.json"));
            Self { index, decon, tokenizer }
        }

        pub(crate) fn tokens(&self, text: &str) -> Vec<MorphToken> {
            let mut worker = self.tokenizer.new_worker();
            worker.reset_sentence(text);
            worker.tokenize();
            worker
                .token_iter()
                .map(|t| {
                    let range = t.range_char();
                    let fields: Vec<&str> = t.feature().split(',').collect();
                    MorphToken {
                        start: range.start,
                        end: range.end,
                        surface: t.surface().to_string(),
                        base_form: fields.get(6).map(|s| s.to_string()).unwrap_or_else(|| t.surface().to_string()),
                        pos: fields.get(0).unwrap_or(&"").to_string(),
                        reading: normalize::normalize_text(fields.get(7).unwrap_or(&"")),
                    }
                })
                .collect()
        }

        fn lookup(&self, text: &str, position: usize) -> MatchSpan {
            let tokens = self.tokens(text);
            lookup_from_position(text, position, 0, &self.index, &self.decon, &tokens).unwrap()
        }
    }

    fn top_reading(span: &MatchSpan) -> String {
        span.entries[0].readings[0].clone()
    }

    #[test]
    fn shimasu_resolves_to_suru_not_shiru() {
        let h = Harness::new();
        let span = h.lookup("連絡します", 2); // position of し
        assert_eq!(span.surface, "します");
        assert_eq!(top_reading(&span), "する",
            "first entry should read する (suru), got {:?}",
            span.entries.iter().map(|e| &e.readings[0]).collect::<Vec<_>>());
    }

    #[test]
    fn mae_context_orders_mae_before_zen() {
        let h = Harness::new();
        // 前 in 前にある is read まえ; 前(ぜん) is earlier in the dict, so
        // only the context reading can put 前(まえ) first.
        let span = h.lookup("前にある", 0);
        assert_eq!(span.surface, "前");
        assert_eq!(top_reading(&span), "まえ");
    }

    #[test]
    fn overlong_candidate_absorbs_auxiliaries_to_verb() {
        let h = Harness::new();
        // JL parity: 待ってみてください deconjugates through the ～てみる/～
        // ください auxiliaries all the way to 待つ (JL does the same), so the
        // whole phrase resolves instead of stopping at 待って.
        let span = h.lookup("待ってみてください", 0);
        assert_eq!(span.surface, "待ってみてください");
        assert_eq!(top_reading(&span), "まつ");
    }

    #[test]
    fn jl_style_auxiliary_resolution() {
        let h = Harness::new();
        // ～てくる: 入ってこない -> 入る.
        let span = h.lookup("入ってこない", 0);
        assert_eq!(span.surface, "入ってこない");
        assert_eq!(top_reading(&span), "はいる");

        // causative-passive: 知らされなかった -> 知る.
        let span = h.lookup("知らされなかった", 0);
        assert_eq!(span.surface, "知らされなかった");
        assert_eq!(top_reading(&span), "しる");

        // ～てしまう: 食べてしまった -> 食べる.
        let span = h.lookup("食べてしまった", 0);
        assert_eq!(top_reading(&span), "たべる");

        // stacked causative + passive + negative + past: 行かせられなかった -> 行く.
        let span = h.lookup("行かせられなかった", 0);
        assert_eq!(top_reading(&span), "いく");

        // contracted ～ちゃう: 言っちゃった -> 言う.
        let span = h.lookup("言っちゃった", 0);
        assert_eq!(top_reading(&span), "いう");
    }

    #[test]
    fn supplementary_must_and_copula_rules() {
        let h = Harness::new();
        // なければならない (JL's rules don't reduce this to the verb):
        // 食べなければならない -> 食べる.
        let span = h.lookup("食べなければならない", 0);
        assert_eq!(span.surface, "食べなければならない");
        assert_eq!(top_reading(&span), "たべる");

        // noun + だった -> the noun: 学生だった -> 学生.
        let span = h.lookup("学生だった", 0);
        assert_eq!(span.surface, "学生だった");
        assert_eq!(top_reading(&span), "がくせい");
    }

    #[test]
    fn taberareru_resolves_to_taberu() {
        let h = Harness::new();
        let span = h.lookup("食べられます", 0);
        assert_eq!(span.surface, "食べられます");
        assert_eq!(top_reading(&span), "たべる");
    }

    #[test]
    fn kana_kuru_negative_masu_still_resolves() {
        let h = Harness::new();
        // きませんでした: no kana deconjugation rule, but the き token's base
        // form (くる) lets morphology resolve it instead of く/きる guesses.
        let span = h.lookup("きませんでした", 0);
        assert_eq!(span.surface, "きませんでした");
        assert_eq!(top_reading(&span), "くる");
    }

    #[test]
    fn plain_kana_lookup_unaffected() {
        let h = Harness::new();
        // No verb morphology involved; should resolve via reading priority.
        let span = h.lookup("こんにちは", 0);
        assert_eq!(span.surface, "こんにちは");
        assert!(span.entries[0].readings[0] == "こんにちは");
    }

    #[test]
    fn particles_match_as_single_tokens() {
        let h = Harness::new();
        // が/を/も/に/は/の are dictionary entries (蛾, を, 藻, 二, 歯, 野)
        // and stay lookup-able, but only as their own token — never merged
        // into the next word.
        let cases = [
            ("可能性があります", 3, "が"),
            ("考え方をしている", 3, "を"),
            ("意図もない", 2, "も"),
            ("にさせたい", 0, "に"),
            ("はしません", 0, "は"),
            ("のことは", 0, "の"),
            ("はよほどいい", 0, "は"),
        ];
        for (text, pos, want) in cases {
            let span = h.lookup(text, pos);
            assert_eq!(span.surface, want, "particle at {pos} in {text}");
        }
    }

    #[test]
    fn auxiliaries_match_as_single_tokens() {
        let h = Harness::new();
        // なんだ segments as な/ん/だ — な (助動詞) is limited to its own
        // token, so なんだ never forms and 涙 (reading なんだ) is unreachable.
        let span = h.lookup("そういう相手なんだ", 6); // な
        assert_eq!(span.surface, "な");
        assert_ne!(top_reading(&span), "なみだ");

        let span = h.lookup("そういう相手なんだ", 8); // だ
        assert_eq!(span.surface, "だ");

        // ます as its own token still resolves (鱒/増す).
        let span = h.lookup("可能性があります", 6); // ます
        assert_eq!(span.surface, "ます");
    }

    #[test]
    fn verbs_after_particles_resolve_correctly() {
        let h = Harness::new();
        // はしません -> する (not 走る via ichidan ません->る).
        let span = h.lookup("はしません", 1); // し
        assert_eq!(span.surface, "しません");
        assert_eq!(top_reading(&span), "する");

        // にさせたい -> する (not にる via causative recursion).
        let span = h.lookup("にさせたい", 1); // さ
        assert_eq!(span.surface, "させたい");
        assert_eq!(top_reading(&span), "する");

        // できる限り -> 出来る限り (real phrase), not 模する from もできる.
        let span = h.lookup("できる限りのことはします", 0); // できる
        assert_eq!(span.surface, "できる限り");
        assert_eq!(top_reading(&span), "できるかぎり");

        // 意図もない -> ない is the adjective 無い, not 盛る.
        let span = h.lookup("意図もない", 3); // ない
        assert_eq!(span.surface, "ない");
        assert_eq!(top_reading(&span), "ない");

        // はよほど -> よほど is the adverb 余程, not 早よ.
        let span = h.lookup("はよほどいい", 1); // よほど
        assert_eq!(span.surface, "よほど");
        assert_eq!(top_reading(&span), "よほど");

        // のこと -> こと (事/琴), not のこ (鋸).
        let span = h.lookup("のことは", 1); // こと
        assert_eq!(span.surface, "こと");
        assert_eq!(top_reading(&span), "こと");
    }

    fn scan(h: &Harness, text: &str) -> Vec<MatchSpan> {
        let tokens = h.tokens(text);
        let chars: Vec<char> = text.chars().collect();
        let mut spans = Vec::new();
        let mut pos = 0usize;
        while pos < chars.len() {
            if let Some(span) = lookup_from_position(text, pos, 0, &h.index, &h.decon, &tokens) {
                if !span.entries.is_empty() {
                    let end = span.end;
                    spans.push(span);
                    pos = end.max(pos + 1);
                    continue;
                }
            }
            let next = tokens
                .iter()
                .find(|t| t.start <= pos && pos < t.end)
                .or_else(|| tokens.iter().find(|t| t.start >= pos))
                .map(|t| t.end);
            pos = next.map(|e| e.max(pos + 1)).unwrap_or(pos + 1);
        }
        spans
    }

    #[test]
    fn scan_respects_particle_boundaries() {
        let h = Harness::new();
        // A function-word token must never appear inside a longer span: every
        // span that starts at a 助詞/助動詞 is exactly that one token.
        for text in [
            "可能性があります",
            "考え方をしている",
            "意図もない",
            "にさせたい",
            "はしません",
            "のことは",
            "はよほどいい",
            "そういう相手なんだ",
            "できる限りのことはします",
        ] {
            let tokens = h.tokens(text);
            for span in scan(&h, text) {
                let token = tokens
                    .iter()
                    .find(|t| span.start >= t.start && span.start < t.end)
                    .unwrap();
                if matches!(token.pos.as_str(), "助詞" | "助動詞") {
                    assert_eq!(
                        span.surface.chars().count(),
                        1,
                        "function word {} merged into {:?} in {text}",
                        token.surface,
                        span.surface
                    );
                }
            }
        }
    }

    #[test]
    fn reading_deconj_beats_priority_for_homographs() {
        let h = Harness::new();
        // ひかれております is 惹かれる's te-form; JL ranks fewer deconj steps
        // before frequency, so 惹かれる (3 steps) beats 光る/引く (4 steps)
        // even though both are ichi1-frequency while 惹かれる is only spec1.
        let span = h.lookup("あなたに惹かれております", 4);
        assert_eq!(span.surface, "惹かれております");
        assert_eq!(top_reading(&span), "ひかれる");
    }

    #[test]
    fn mid_token_span_falls_back_to_surface_deconj() {
        let h = Harness::new();
        // The き of とき sits inside the single token とき (とく verb), so the
        // span きました has no honest token reading (its tokens read ました)
        // and must deconjugate the surface instead: きました -> 来る, not ます.
        let span = h.lookup("ときましたかぁ", 1);
        assert_eq!(span.surface, "きました");
        assert_eq!(top_reading(&span), "くる");
    }

    #[test]
    fn suru_contraction_beats_reading_homophones() {
        let h = Harness::new();
        // してん (する+て+ん) must resolve to する via the tokenizer's base
        // form, not to the してん reading-homophones 支店/視点.
        let span = h.lookup("顔してんの!", 1);
        assert_eq!(span.surface, "してん");
        assert_eq!(top_reading(&span), "する");
    }

    #[test]
    fn leading_and_trailing_punctuation_never_enters_span() {
        let h = Harness::new();
        // Leading dots: the cursor lands on "......" (an unknown 名詞 token),
        // but the span must snap forward to the actual word 苦労. 苦労 is a
        // suru-verb noun followed by している, so the whole economy resolves to
        // 苦労 (teiru) — exactly like 調査している -> 調査.
        let span = h.lookup("......苦労しているのも", 0);
        assert_eq!(span.surface, "苦労している");
        assert_eq!(span.start, 6);
        assert_eq!(top_reading(&span), "くろう");
        assert_eq!(span.deconjugated_from.as_deref(), Some("teiru"));

        // Trailing 記号: the span stops before 〜.
        let span = h.lookup("苦労〜", 0);
        assert_eq!(span.surface, "苦労");
        assert_eq!(top_reading(&span), "くろう");
    }

    #[test]
    fn unknown_tokens_do_not_extend_spans() {
        let h = Harness::new();
        // ぅ is an unknown token with an empty reading; without the break it
        // extended 疲れるぅ whose reading つかれる deconjugated to つく. The
        // span must stop at 疲れる so the literal spelling wins.
        let span = h.lookup("こっちの人格疲れるぅ〜......", 6);
        assert_eq!(span.surface, "疲れる");
        assert_eq!(top_reading(&span), "つかれる");
    }

    #[test]
    fn kire_resolves_to_kireru_in_context() {
        let h = Harness::new();
        // キレてる -> 切れる (the キレる slang), with 切る as the secondary
        // candidate rather than a coincidental deconj homophone outranking it.
        let span = h.lookup("キレてる", 0);
        assert_eq!(span.surface, "キレてる");
        assert_eq!(top_reading(&span), "きれる");
    }

    #[test]
    fn na_adjective_na_stays_with_the_noun() {
        let h = Harness::new();
        // The imperative な rule deconjugates 好きな -> 好く (すく), but a
        // verb-class result with no verb token in the span is coincidental.
        let span = h.lookup("好きな", 0);
        assert_eq!(span.surface, "好き");
        assert_eq!(top_reading(&span), "すき");
        let span = h.lookup("真面目な", 0);
        assert_eq!(span.surface, "真面目");
        assert_eq!(top_reading(&span), "まじめ");
    }

    #[test]
    fn deconj_labels_name_the_surface_conjugation() {
        let h = Harness::new();
        for (text, expected) in [
            ("した", "past"),
            ("高かった", "past"),
            ("面白くない", "negative"),
            ("呼んでいる", "teiru"),
            ("しなければならない", "must"),
            ("来てください", "polite request"),
        ] {
            let span = h.lookup(text, 0);
            assert_eq!(
                span.deconjugated_from.as_deref(),
                Some(expected),
                "{text} should be labeled {expected}, got {:?}",
                span.deconjugated_from
            );
        }
    }

    #[test]
    fn nai_to_ikemasen_detects_must() {
        let h = Harness::new();
        for (text, base) in [
            ("洗わないといけません", "あらう"),
            ("洗わなければなりません", "あらう"),
            ("食べないといけない", "たべる"),
            ("食べなくてはいけません", "たべる"),
            ("しないといけません", "する"),
            ("しなければいけない", "する"),
        ] {
            let span = h.lookup(text, 0);
            assert_eq!(top_reading(&span), base, "{text} should resolve to {base}");
            assert_eq!(
                span.deconjugated_from.as_deref(),
                Some("must"),
                "{text} should be labeled must, got {:?}",
                span.deconjugated_from
            );
        }
    }

    #[test]
    fn toire_sentence_spans() {
        let h = Harness::new();
        let spans = scan(&h, "トイレを使ってから、手を洗わないといけません。");
        let mut pairs: Vec<(String, String)> = spans
            .iter()
            .map(|s| (s.surface.clone(), s.deconjugated_from.clone().unwrap_or_default()))
            .collect();
        pairs.retain(|(s, _)| !s.is_empty());
        assert_eq!(
            pairs,
            vec![
                ("トイレ".to_string(), String::new()),
                ("を".to_string(), String::new()),
                ("使ってから".to_string(), "after doing".to_string()),
                ("手".to_string(), String::new()),
                ("を".to_string(), String::new()),
                ("洗わないといけません".to_string(), "must".to_string()),
            ]
        );
    }

    #[test]
    fn morph_labels_name_conjugations_for_kanji_bases() {
        let h = Harness::new();
        // The label comes from the deconjugation of the span reading, matched
        // to the base by dictionary entry (のむ == 飲む), never the bare base
        // form. から is a trailing particle, so 飲んでから still reaches the
        // te-form; 始めます dead-ends at the unrecordable ます-stem but is
        // still named by the first rule applied.
        for (text, base, expected) in [
            ("飲んで", "のむ", "te"),
            ("飲んでから", "のむ", "after doing"),
            ("食べてから", "たべる", "after doing"),
            ("飲んでいる", "のむ", "teiru"),
            ("食べています", "たべる", "polite + teiru"),
            ("始めます", "はじめる", "polite"),
            ("始めませんでした", "はじめる", "polite past negative"),
            ("言いませんでした", "いう", "polite past negative"),
        ] {
            let span = h.lookup(text, 0);
            assert_eq!(
                span.deconjugated_from.as_deref(),
                Some(expected),
                "{text} should be labeled {expected}, got {:?}",
                span.deconjugated_from
            );
            assert_eq!(top_reading(&span), base, "{text} should resolve to {base}");
        }
    }

    #[test]
    fn combined_labels_name_all_meaningful_rules() {
        let h = Harness::new();
        // Labels combine every meaningful rule applied to the surface,
        // outermost first, skipping JL's intermediate-stem jargon — so
        // 住んでいた reads "past + teiru" rather than "past" and
        // 忘れてしまった reads "past + ended up" rather than "past".
        for (text, base, expected) in [
            ("住んでいた", "すむ", "past + teiru"),
            ("食べています", "たべる", "polite + teiru"),
            ("忘れてしまった", "わすれる", "past + ended up"),
            ("食べてしまった", "たべる", "past + ended up"),
            ("食べさせられてしまった", "たべる", "past + ended up + potential + causative"),
            ("食べられます", "たべる", "polite + potential"),
            ("しておきました", "する", "polite past + for now"),
            ("しておく", "する", "for now"),
            ("してくれました", "する", "polite past"),
            ("知らされなかった", "しる", "past + negative + causative passive"),
            ("言っちゃった", "いう", "past + ended up"),
        ] {
            let span = h.lookup(text, 0);
            assert_eq!(
                span.deconjugated_from.as_deref(),
                Some(expected),
                "{text} should be labeled {expected}, got {:?}",
                span.deconjugated_from
            );
            assert_eq!(top_reading(&span), base, "{text} should resolve to {base}");
        }
    }

    #[test]
    fn suru_noun_phrases_resolve_to_the_noun() {
        let h = Harness::new();
        // 調査 + している is the suru verb 調査 doing teiru; the dictionary
        // headword is the noun 調査, not a literal 調査する or the coincidental
        // homophone 弔する (ちょうする) that whole-reading deconjugation finds.
        for (text, want_label) in [
            ("調査している", "teiru"),
            ("調査していた", "past + teiru"),
            ("調査して", "te"),
        ] {
            let span = h.lookup(text, 0);
            assert_eq!(span.surface, text);
            assert_eq!(top_reading(&span), "ちょうさ", "{text} should resolve to 調査");
            assert_eq!(
                span.deconjugated_from.as_deref(),
                Some(want_label),
                "{text} should be labeled {want_label}, got {:?}",
                span.deconjugated_from
            );
        }
    }

    #[test]
    fn unknown_leading_token_deconjugates_surface_not_truncated_reading() {
        let h = Harness::new();
        // One-liner: ジイさんじゃない. The ジイ token is unknown (empty reading),
        // so the concatenated reading "さんじゃない" would misfire to さん. The
        // full surface じいさんじゃない deconjugates to じいさん (爺さん) instead.
        let span = h.lookup("ジイさんじゃない。", 0);
        assert_eq!(span.surface, "ジイさんじゃない");
        assert_eq!(top_reading(&span), "じいさん");
        assert_eq!(span.deconjugated_from.as_deref(), Some("copula"));

        // マズいんだ: the unknown katakana マズ must not collapse to the tail's
        // deconjugation (いんだ -> いぬ); instead the false maz/boundary is
        // crossed to form the real word マズい (= 不味い, まずい).
        let span = h.lookup("マズいんだ", 0);
        assert_eq!(span.surface, "マズい");
        assert_eq!(top_reading(&span), "まずい");
    }

    #[test]
    fn even_if_rules_name_the_condition() {
        let h = Harness::new();
        // JL has no rules for the も-ending te-form, so 急がなくても used to
        // fall back to naming an intermediate stem ("adverbial stem").
        for (text, base, expected) in [
            ("急がなくても", "いそぐ", "even if not"),
            ("食べなくても", "たべる", "even if not"),
            ("行かなくても", "いく", "even if not"),
            ("しなくても", "する", "even if not"),
            ("食べても", "たべる", "even if"),
            ("行っても", "いく", "even if"),
            ("飲んでも", "のむ", "even if"),
            ("高くても", "たかい", "even if"),
        ] {
            let span = h.lookup(text, 0);
            assert_eq!(
                span.deconjugated_from.as_deref(),
                Some(expected),
                "{text} should be labeled {expected}, got {:?}",
                span.deconjugated_from
            );
            assert_eq!(top_reading(&span), base, "{text} should resolve to {base}");
        }
    }
}

