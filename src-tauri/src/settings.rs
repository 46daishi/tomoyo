use serde::{Deserialize, Serialize};
use std::fs;
use std::sync::Mutex;
use tauri::Manager;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct AppSettings {
    // ── General ──
    pub font_family: String,
    pub font_size: u32,
    pub resume_last_session: bool,
    pub discord_rpc_enabled: bool,

    // ── Lookup ──
    pub lookup_mode: String, // "click" | "hover" | "hotkey"
    pub lookup_hotkey: String,
    pub cycle_key: String,
    pub lookup_limit_enabled: bool,
    pub lookup_limit_per_hour: u32,
    pub word_highlight_enabled: bool,
    pub show_related_entries: bool,
    pub track_unknown_words: bool,

    // ── Input & History ──
    pub input_mode: String, // "clipboard" | "websocket"
    pub websocket_address: String,
    pub jp_detection_threshold: f32,
    pub history_enabled: bool,
    pub history_span: u32,

    // ── Mini Mode ──
    pub mini_mode_enabled: bool,
    pub mini_mode_enter_height: u32,
    pub mini_mode_exit_height: u32,
    pub mini_mode_transparency: f32,

    // ── Review (placeholder) ──
    pub default_review_mode: String, // "normal" | "flashcard"
    pub difficulty_estimation_enabled: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            font_family: "Noto Sans JP".into(),
            font_size: 32,
            resume_last_session: false,
            discord_rpc_enabled: false,

            lookup_mode: "click".into(),
            lookup_hotkey: "AltLeft".into(),
            cycle_key: "ShiftLeft".into(),
            lookup_limit_enabled: false,
            lookup_limit_per_hour: 30,
            word_highlight_enabled: true,
            show_related_entries: true,
            track_unknown_words: false,

            input_mode: "clipboard".into(),
            websocket_address: "ws://127.0.0.1:6677".into(),
            jp_detection_threshold: 0.3,
            history_enabled: true,
            history_span: 10,

            mini_mode_enabled: true,
            mini_mode_enter_height: 300,
            mini_mode_exit_height: 350,
            mini_mode_transparency: 0.5,

            default_review_mode: "normal".into(),
            difficulty_estimation_enabled: false,
        }
    }
}

pub struct SettingsState(pub Mutex<AppSettings>);

fn settings_path(app: &tauri::AppHandle) -> Result<std::path::PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("settings.json"))
}

pub fn load_settings_from_disk(app: &tauri::AppHandle) -> AppSettings {
    let path = match settings_path(app) {
        Ok(p) => p,
        Err(_) => return AppSettings::default(),
    };

    match fs::read_to_string(&path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => AppSettings::default(), // file doesn't exist yet — first run
    }
}

#[tauri::command]
pub fn get_settings(state: tauri::State<SettingsState>) -> AppSettings {
    state.0.lock().unwrap().clone()
}

#[tauri::command]
pub fn save_settings(
    app: tauri::AppHandle,
    state: tauri::State<SettingsState>,
    settings: AppSettings,
) -> Result<(), String> {
    let path = settings_path(&app)?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let json = serde_json::to_string_pretty(&settings).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())?;

    *state.0.lock().unwrap() = settings;
    Ok(())
}