//! Tauri commands and tokenizer infrastructure: state wrappers,
//! cached tokenization, lookup/scan entry points, database transfer,
//! resource resolution.

use crate::lookup::lookup_candidate;
use crate::spans::lookup_from_position;
use crate::deconjugate::Deconjugator;
use crate::index::{find_containing, DictState};
use crate::normalize;
use crate::types::{MatchSpan, MorphToken, TokenOut};
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use tauri::Manager;
use vibrato::Tokenizer;


pub(crate) struct TokenizerState(pub(crate) Mutex<Tokenizer>);

/// Cache of sentence -> morphological tokens, so repeated lookups against the
/// same sentence (hover, cycle, scan) don't re-run the tokenizer.
pub(crate) struct MorphCacheState(pub(crate) Mutex<HashMap<String, Vec<MorphToken>>>);

pub(crate) struct DeconjRulesState(pub(crate) Deconjugator);


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
pub(crate) fn lookup_at_position(
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

/// Looks up an exact substring (the sentence window's "Look up selected"):
/// whatever text[start..end] is resolves as its own span — with entries when
/// the dictionary reaches it, empty (but related-filled, still displayed)
/// when nothing does. Unlike lookup_at_position this never extends past
/// `end` or falls back to a shorter span.
#[tauri::command]
pub(crate) fn lookup_exact(
    dict_state: tauri::State<DictState>,
    decon_state: tauri::State<DeconjRulesState>,
    morph_cache: tauri::State<MorphCacheState>,
    tokenizer_state: tauri::State<TokenizerState>,
    text: String,
    start: usize,
    end: usize,
) -> Option<MatchSpan> {
    let tokens = tokenize_cached(&morph_cache.0, &tokenizer_state.0, &text);
    let chars: Vec<char> = text.chars().collect();
    if start >= end || end > chars.len() {
        return None;
    }
    let candidate: String = chars[start..end].iter().collect();
    if candidate.is_empty() {
        return None;
    }
    // In-context reading from the token under the cursor (as in the
    // single-char fallback); the tokenizer's base form only when the
    // selection is exactly that token, otherwise morphology would name a
    // word the selection merely overlaps.
    let containing = tokens.iter().find(|t| start >= t.start && start < t.end);
    let context_reading = containing.and_then(|t| {
        if t.reading.is_empty() {
            None
        } else {
            Some(t.reading.as_str())
        }
    });
    let morph_base = tokens
        .iter()
        .find(|t| t.start == start && t.end == end)
        .filter(|t| t.pos == "動詞" && t.base_form != t.surface && t.surface != "っ")
        .map(|t| t.base_form.as_str());
    let (entries, deconj_info) = lookup_candidate(
        &candidate,
        &dict_state.0,
        &decon_state.0,
        context_reading,
        morph_base,
        &tokens,
        start,
    )
    .map_or((Vec::new(), None), |(e, l)| (e, l));
    let exact_ids: HashSet<u32> = entries.iter().map(|e| e.id).collect();
    let related = find_containing(&candidate, &dict_state.0, 20)
        .into_iter()
        .filter(|e| !exact_ids.contains(&e.id))
        .collect();
    Some(MatchSpan {
        start,
        end,
        surface: candidate,
        entries,
        deconjugated_from: deconj_info,
        related_entries: related,
    })
}

/// Morphological tokens for a whole sentence (with char offsets), for
/// frontend consumers that need grammar-aware segmentation.
#[tauri::command]
pub(crate) fn tokenize_sentence(
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
pub(crate) fn scan_sentence(
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
pub(crate) fn tokenize_text(state: tauri::State<TokenizerState>, text: String) -> Vec<TokenOut> {
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
pub(crate) fn export_database(app: tauri::AppHandle, dest: String) -> Result<(), String> {
    let db_path = app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("immersion.db");
    std::fs::copy(&db_path, &dest).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub(crate) fn import_database(app: tauri::AppHandle, source: String) -> Result<(), String> {
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
pub(crate) fn restart_app(app: tauri::AppHandle) {
    app.restart();
}

/// Resolves a bundled runtime resource (e.g. `resources/jmdict.json`) so
/// ad-hoc "executable + resources folder" distributions work, not just
/// proper bundles or in-`target/` runs. First existing file wins:
/// 1. Tauri's Resource dir (bundled installers, cargo-run staging),
/// 2. next to the executable (`<dir>/tomoyo` + `<dir>/resources/…`),
/// 3. the current working directory,
/// 4. the crate dir (dev fallback).
/// A total miss lists every attempted absolute path so the failure is
/// self-diagnosing instead of a bare ENOENT from the setup hook.
pub(crate) fn resolve_resource(app: &tauri::AppHandle, rel: &str) -> Result<std::path::PathBuf, std::io::Error> {
    use std::path::PathBuf;

    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Ok(p) = app.path().resolve(rel, tauri::path::BaseDirectory::Resource) {
        candidates.push(p);
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            candidates.push(dir.join(rel));
        }
    }
    if let Ok(cwd) = std::env::current_dir() {
        candidates.push(cwd.join(rel));
    }
    candidates.push(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(rel));

    for path in &candidates {
        if path.is_file() {
            return Ok(path.clone());
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!(
            "resource `{rel}` not found; looked in:\n{}",
            candidates
                .iter()
                .map(|p| format!("  - {}", p.display()))
                .collect::<Vec<_>>()
                .join("\n")
        ),
    ))
}

