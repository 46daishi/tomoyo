mod deconjugate;
mod normalize;
use deconjugate::{build_deconjugation_rules, deconjugate, DeconjRule};
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

#[derive(Serialize)]
struct TokenOut {
    surface: String,
    reading: String,
    pos: String,
    base_form: String,
}

struct TokenizerState(Mutex<Tokenizer>);

#[derive(Deserialize, Serialize, Clone)]
struct DictEntry {
    id: u32,
    spellings: Vec<String>,
    readings: Vec<String>,
    definitions: Vec<String>,
    pos: Vec<String>,
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

    let mut results = Vec::new();
    for id in candidates {
        if let Some(entry) = index.by_id.get(&id) {
            let actually_contains = entry
                .spellings
                .iter()
                .chain(entry.readings.iter())
                .any(|s| normalize::normalize_text(s).contains(&normalized));

            if actually_contains {
                results.push(Arc::clone(entry));
                if results.len() >= limit {
                    break;
                }
            }
        }
    }

    results
}

struct DeconjRulesState(Vec<DeconjRule>);

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
    decon_rules: &[DeconjRule],
) -> Option<MatchSpan> {
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    if position >= len {
        return None;
    }

    let max_len = MAX_CHARS_COMBINED.min(len - position);
    let mut found = 0usize;
    for span_len in (1..=max_len).rev() {
        let candidate: String = chars[position..position + span_len].iter().collect();
        if let Some((entries, deconj_info)) = lookup_candidate(&candidate, index, decon_rules) {
            if found == skip {
                // ── NEW: compute related entries before returning ──
                let exact_ids: HashSet<u32> = entries.iter().map(|e| e.id).collect();
                let related = find_containing(&candidate, index, 20)
                    .into_iter()
                    .filter(|e| !exact_ids.contains(&e.id))
                    .collect();

                return Some(MatchSpan {
                    start: position,
                    end: position + span_len,
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
/// Among deconjugated hits, keeps the one with the *fewest* rule-chain
/// steps, mirroring JL's "show only deconjugation processes with the
/// fewest steps" behavior — otherwise rule iteration order can surface a
/// bogus multi-step chain ahead of a correct one-step chain.
// Increase depth from 3 to 5 to accommodate stacked causative-passive + desire + negative + past
const MAX_DECONJUGATION_DEPTH: usize = 5;

fn lookup_candidate(
    candidate: &str,
    index: &DictionaryIndex,
    decon_rules: &[DeconjRule],
) -> Option<(Vec<Arc<DictEntry>>, Option<String>)> {
    let variants = normalize_variants(candidate);

    for key in &variants {
        if let Some(entries) = index.by_text.get(key) {
            return Some((entries.clone(), None));
        }
    }

    let mut best: Option<(usize, Vec<Arc<DictEntry>>, String)> = None;
    for key in &variants {
        for form in deconjugate(key, decon_rules, MAX_DECONJUGATION_DEPTH) {
            if let Some(entries) = index.by_text.get(&form.text) {
                let chain_len = form.rule_chain.len();
                let is_better = best.as_ref().map_or(true, |(best_len, _, _)| chain_len < *best_len);
                if is_better {
                    best = Some((chain_len, entries.clone(), form.rule_chain.join(" + ")));
                }
            }
        }
    }

    best.map(|(_, entries, chain)| (entries, Some(chain)))
}

struct DictState(DictionaryIndex);

#[tauri::command]
fn lookup_at_position(
    dict_state: tauri::State<DictState>,
    decon_state: tauri::State<DeconjRulesState>,
    text: String,
    position: usize,
    skip: usize,
) -> Option<MatchSpan> {
    lookup_from_position(&text, position, skip, &dict_state.0, &decon_state.0)
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let migrations = vec![
        Migration {
            version: 1,
            description: "create_media_table",
            sql: include_str!("../migrations/0001_media.sql"),
            kind: MigrationKind::Up,
        },
    ];

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(
            tauri_plugin_sql::Builder::default()
                .add_migrations("sqlite:immersion.db", migrations)
                .build(),
        )
        .invoke_handler(tauri::generate_handler![tokenize_text, lookup_at_position])
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
                        app_handle.exit(0);
                    }
            });

            // ── Tokenizer (Vibrato) — still used by tokenize_text for
            // per-word reading/POS/base-form breakdown. It is not used by
            // lookup_at_position, which resolves matches purely from the
            // dictionary index + deconjugation rules. ──
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
            app.manage(DeconjRulesState(build_deconjugation_rules()));

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}