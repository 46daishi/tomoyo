use tauri::Manager;
use tauri_plugin_sql::{Migration, MigrationKind};

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
        .plugin(tauri_plugin_fs::init())
        .plugin(
                    tauri_plugin_sql::Builder::default()
                        .add_migrations("sqlite:immersion.db", migrations)
                        .build(),
                )
        .invoke_handler(tauri::generate_handler![])
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();

            #[cfg(target_os = "windows")]
            window.set_decorations(true)?;

            #[cfg(target_os = "linux")]
            window.set_decorations(false)?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
