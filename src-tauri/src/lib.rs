use std::sync::Mutex;
use tauri::Manager;
use vibrato::{Dictionary, Tokenizer};
use serde::Serialize;
use tauri_plugin_sql::{Migration, MigrationKind};
use zstd::Decoder;
use std::collections::HashMap;
use serde::Deserialize;

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
    // maps any surface form (kanji OR kana) to the entries that use it
    by_text: HashMap<String, Vec<DictEntry>>,
}

impl DictionaryIndex {
    fn build(entries: Vec<DictEntry>) -> Self {
        let mut by_text: HashMap<String, Vec<DictEntry>> = HashMap::new();
        for entry in entries {
            for spelling in entry.spellings.iter().chain(entry.readings.iter()) {
                by_text.entry(spelling.clone()).or_default().push(entry.clone());
            }
        }
        Self { by_text }
    }
}

const MAX_MATCH_CHARS: usize = 12; // JMdict entries rarely exceed this; tune if needed

#[derive(serde::Serialize)]
struct MatchSpan {
    start: usize,     // char index into the sentence
    end: usize,       // char index (exclusive)
    surface: String,
    entries: Vec<DictEntry>, // empty if this span is an unmatched fallback
}

fn longest_match_scan(text: &str, index: &DictionaryIndex) -> Vec<MatchSpan> {
    let chars: Vec<char> = text.chars().collect();
    let mut spans = Vec::new();
    let mut pos = 0;

    while pos < chars.len() {
        let max_len = (chars.len() - pos).min(MAX_MATCH_CHARS);
        let mut matched = None;

        // try longest substring first, shrink until something matches
        for len in (1..=max_len).rev() {
            let candidate: String = chars[pos..pos + len].iter().collect();
            if let Some(entries) = index.by_text.get(&candidate) {
                matched = Some((len, candidate, entries.clone()));
                break;
            }
        }

        match matched {
            Some((len, surface, entries)) => {
                spans.push(MatchSpan { start: pos, end: pos + len, surface, entries });
                pos += len;
            }
            None => {
                // no dictionary entry at all starting here — emit a 1-char
                // fallback span so the frontend still has something to render/click
                let surface: String = chars[pos..pos + 1].iter().collect();
                spans.push(MatchSpan { start: pos, end: pos + 1, surface, entries: vec![] });
                pos += 1;
            }
        }
    }

    spans
}

struct DictState(DictionaryIndex);

#[tauri::command]
fn lookup_sentence(state: tauri::State<DictState>, text: String) -> Vec<MatchSpan> {
    longest_match_scan(&text, &state.0)
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
        .invoke_handler(tauri::generate_handler![tokenize_text, lookup_sentence])
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();

            #[cfg(target_os = "windows")]
            window.set_decorations(true)?;

            #[cfg(target_os = "linux")]
            window.set_decorations(false)?;

            // ── Tokenizer (Vibrato) ──
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

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
