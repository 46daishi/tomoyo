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
    position: usize,
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
        if !function_word {
            for tok in tokens.iter().filter(|tok| tok.start > position) {
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
        let candidate: String = chars[position..end].iter().collect();
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
                    end,
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
    Reading,         // normalized surface == a reading
    Morphological,   // reached via the tokenizer's base form (e.g. します -> する)
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
    // of matching both homograph readings of 入る. Sub-span candidates that
    // end inside the cursor token have no reading and fall back to surface
    // deconjugation.
    let span_reading: Option<String> = {
        let in_span: Vec<&MorphToken> = tokens
            .iter()
            .filter(|t| t.start >= position && t.end <= position + span_len)
            .collect();
        if in_span.is_empty() {
            None
        } else {
            Some(in_span.iter().map(|t| t.reading.as_str()).collect())
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
            if let Some(entries) = index.by_text.get(&base_norm) {
                for e in entries {
                    if seen_ids.insert(e.id) {
                        let ctx = context_reading.map_or(false, |r| reading_matches_context(e, r));
                        candidates.push((Arc::clone(e), 1, Some(base.to_string()), MatchKind::Morphological, ctx));
                    }
                }
            }
        }
    }

    // Rule-based deconjugation — fallback beneath morphology. Entries already
    // found via literal/morphological paths are skipped via seen_ids, so a
    // word never appears twice just because both paths resolved to it. Each
    // deconjugation result is also validated against the entry's POS (the
    // rule's word class must appear among the entry's parts of speech), so a
    // coincidental conjugation like しる -> 知る (v5r) never surfaces.
    for form in &deconj_forms {
        let key = normalize::normalize_text(&form.text);
        if let Some(entries) = index.by_text.get(&key) {
            let chain_desc = form.rule_chain.clone();
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

    // Sort: how the entry was reached (literal spelling > reading > morph
    // base form > rule deconjugation) first, then whether its reading matches
    // the in-context reading, then priority, then non-bound entries, then
    // fewest deconjugation steps. As a last tie-break, prefer the entry whose
    // spelling matches the tokenizer's base form — JL resolves ties like
    // 行かせられなかった (行く vs 生かす, both 4 steps) with its frequency data,
    // and matching the independently-tokenized base form is the closest
    // JL-faithful signal we have.
    candidates.sort_by(|a, b| {
        let a_prio = priority_score(&a.0);
        let b_prio = priority_score(&b.0);
        a.3.cmp(&b.3)
            .then(b.4.cmp(&a.4)) // context-match: true first
            .then(b_prio.cmp(&a_prio))
            .then(is_bound_only(&a.0).cmp(&is_bound_only(&b.0))) // false (not bound) sorts before true
            .then(a.1.cmp(&b.1))
            .then_with(|| match morph_base {
                Some(base) => {
                    let a_matches =
                        a.0.spellings.iter().any(|s| normalize::normalize_text(s) == base);
                    let b_matches =
                        b.0.spellings.iter().any(|s| normalize::normalize_text(s) == base);
                    b_matches.cmp(&a_matches)
                }
                None => std::cmp::Ordering::Equal,
            })
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

    struct Harness {
        index: DictionaryIndex,
        decon: Deconjugator,
        tokenizer: Tokenizer,
    }

    impl Harness {
        fn new() -> Self {
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

        fn tokens(&self, text: &str) -> Vec<MorphToken> {
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
}
