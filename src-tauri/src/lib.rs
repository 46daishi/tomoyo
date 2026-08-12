mod deconjugate;
mod normalize;
mod discord_rpc;
mod settings;

use settings::{get_settings, save_settings, load_settings_from_disk, SettingsState};
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
use discord_rpc::DiscordState;

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

    // (entry, chain_len, chain_description) — chain_len 0 and no description
    // means "literal match," anything else means "reached via deconjugation."
    let mut candidates: Vec<(Arc<DictEntry>, usize, Option<String>)> = Vec::new();
    let mut seen_ids: HashSet<u32> = HashSet::new();

    // Literal matches first — inserted with chain_len 0, so they win ties
    // against equal-priority deconjugated results, same as before.
    for key in &variants {
        if let Some(entries) = index.by_text.get(key) {
            for e in entries {
                if seen_ids.insert(e.id) {
                    candidates.push((Arc::clone(e), 0, None));
                }
            }
        }
    }

    // Deconjugated matches — no longer gated behind "only if no literal
    // match exists." Entries already found literally are skipped via
    // seen_ids, so a word never appears twice just because both paths
    // happened to resolve to it.
    for key in &variants {
        for form in deconjugate(key, decon_rules, MAX_DECONJUGATION_DEPTH) {
            if let Some(entries) = index.by_text.get(&form.text) {
                let chain_len = form.rule_chain.len();
                let chain_desc = form.rule_chain.join(" + ");
                for e in entries {
                    if seen_ids.insert(e.id) {
                        candidates.push((Arc::clone(e), chain_len, Some(chain_desc.clone())));
                    }
                }
            }
        }
    }

    if candidates.is_empty() {
        return None;
    }

    // Priority-tagged entries first; among ties, fewest deconjugation
    // steps first (literal matches, at 0 steps, sort ahead of anything
    // requiring inflection to reach).
    candidates.sort_by(|a, b| {
        let a_prio = priority_score(&a.0);
        let b_prio = priority_score(&b.0);
        b_prio.cmp(&a_prio)
            .then(is_bound_only(&a.0).cmp(&is_bound_only(&b.0))) // false (not bound) sorts before true
            .then(a.1.cmp(&b.1))
    });

    let deconjugated_from = candidates[0].2.clone();
    let entries: Vec<Arc<DictEntry>> = candidates.into_iter().map(|(e, _, _)| e).collect();

    Some((entries, deconjugated_from))
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
        .invoke_handler(tauri::generate_handler![
            discord_rpc::connect_discord,
            discord_rpc::update_discord_presence,
            discord_rpc::disconnect_discord,
            tokenize_text, lookup_at_position,
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

            let initial_settings = settings::load_settings_from_disk(&app.handle());
            app.manage(SettingsState(Mutex::new(initial_settings)));

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}