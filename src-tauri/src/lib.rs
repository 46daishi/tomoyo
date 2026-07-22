use std::sync::Mutex;
use tauri::Manager;
use vibrato::{Dictionary, Tokenizer};
use serde::Serialize;
use tauri_plugin_sql::{Migration, MigrationKind};
use zstd::Decoder;

#[derive(Serialize)]
struct TokenOut {
    surface: String,
    reading: String,
    pos: String,
    base_form: String,
}

struct TokenizerState(Mutex<Tokenizer>);

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
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(
                    tauri_plugin_sql::Builder::default()
                        .add_migrations("sqlite:immersion.db", migrations)
                        .build(),
                )
        .invoke_handler(tauri::generate_handler![tokenize_text])
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();

            #[cfg(target_os = "windows")]
            window.set_decorations(true)?;

            #[cfg(target_os = "linux")]
            window.set_decorations(false)?;

            let resource_path = app
                .path()
                .resolve("resources/ipadic-mecab.dic.zst", tauri::path::BaseDirectory::Resource)?;
            
                let reader = zstd::Decoder::new(std::fs::File::open(resource_path)?)?;
                let dict = Dictionary::read(reader)?;
                let tokenizer = Tokenizer::new(dict);
            
                app.manage(TokenizerState(Mutex::new(tokenizer)));

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
