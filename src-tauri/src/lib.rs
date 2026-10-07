mod deconjugate;
mod normalize;
mod discord_rpc;
mod settings;
mod types;
mod index;
mod rank;
mod rules;
mod lookup;
mod spans;
mod commands;

#[cfg(test)]
#[path = "lookup_tests.rs"]
mod lookup_tests;

use settings::{get_settings, save_settings, SettingsState};
use deconjugate::Deconjugator;
use types::DictEntry;
use index::{DictState, DictionaryIndex};
use commands::{DeconjRulesState, MorphCacheState, TokenizerState, resolve_resource};
use std::collections::HashMap;
use std::sync::Mutex;
use tauri::Manager;
use vibrato::{Dictionary, Tokenizer};
use tauri_plugin_sql::{Migration, MigrationKind};
use zstd::Decoder;
use discord_rpc::DiscordState;

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
        Migration {
            version: 14,
            description: "words_image",
            sql: include_str!("../migrations/0014_words_image.sql"),
            kind: MigrationKind::Up,
        },
        Migration {
            version: 15,
            description: "names",
            sql: include_str!("../migrations/0015_names.sql"),
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
            commands::tokenize_text,
            commands::tokenize_sentence,
            commands::scan_sentence,
            commands::lookup_at_position,
            commands::lookup_exact,
            get_settings, save_settings,
            commands::export_database,
            commands::import_database,
            commands::restart_app,
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
            let resource_path = resolve_resource(app.handle(), "resources/unidic.dic.zst")?;

            let reader = Decoder::new(std::fs::File::open(&resource_path).map_err(|e| {
                std::io::Error::new(
                    e.kind(),
                    format!("{} (tried {})", e, resource_path.display()),
                )
            })?)?;
            let dict = Dictionary::read(reader)?;
            let tokenizer = Tokenizer::new(dict);
            app.manage(TokenizerState(Mutex::new(tokenizer)));

            // ── Dictionary index (JMdict) ──
            let jmdict_path = resolve_resource(app.handle(), "resources/jmdict.json")?;

            let jmdict_json = std::fs::read_to_string(&jmdict_path).map_err(|e| {
                std::io::Error::new(
                    e.kind(),
                    format!("{} (tried {})", e, jmdict_path.display()),
                )
            })?;
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

