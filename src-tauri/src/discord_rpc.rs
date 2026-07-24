use discord_rich_presence::{activity, DiscordIpc, DiscordIpcClient};
use std::sync::Mutex;
use tauri::State;

pub struct DiscordState {
    pub(crate) client: Mutex<Option<DiscordIpcClient>>, // 👈 Marked pub(crate)
}

impl DiscordState {
    pub fn new() -> Self {
        Self {
            client: Mutex::new(None),
        }
    }

    pub fn disconnect(&self) -> Result<(), String> {
        let mut client_lock = self.client.lock().unwrap();
        if let Some(mut client) = client_lock.take() {
            // 1. Tell Discord to remove the active presence overlay
            let _ = client.clear_activity(); // 👈 Essential step!
            
            // 2. Close the IPC connection socket cleanly
            let _ = client.close();
        }
        Ok(())
    }
}

#[tauri::command]
pub fn connect_discord(state: State<DiscordState>, client_id: String) -> Result<(), String> {
    let mut client = DiscordIpcClient::new(&client_id);
    client.connect().map_err(|e| e.to_string())?;

    *state.client.lock().unwrap() = Some(client);
    Ok(())
}

#[tauri::command]
pub fn update_discord_presence(
    state: State<DiscordState>,
    details: Option<String>,
    status: Option<String>,
    large_image: Option<String>,
    large_text: Option<String>,
    small_image: Option<String>,
    small_text: Option<String>,
    start_timestamp: Option<i64>,
    end_timestamp: Option<i64>,
) -> Result<(), String> {
    let mut client_lock = state.client.lock().unwrap();

    if let Some(client) = client_lock.as_mut() {
        let mut activity_builder = activity::Activity::new();

        if let Some(ref d) = details {
            activity_builder = activity_builder.details(d);
        }
        if let Some(ref s) = status {
            activity_builder = activity_builder.state(s);
        }

        if large_image.is_some() || small_image.is_some() {
            let mut assets = activity::Assets::new();

            if let Some(ref img) = large_image {
                assets = assets.large_image(img);
            }
            if let Some(ref txt) = large_text {
                assets = assets.large_text(txt);
            }
            if let Some(ref img) = small_image {
                assets = assets.small_image(img);
            }
            if let Some(ref txt) = small_text {
                assets = assets.small_text(txt);
            }

            activity_builder = activity_builder.assets(assets);
        }

        if start_timestamp.is_some() || end_timestamp.is_some() {
            let mut timestamps = activity::Timestamps::new();

            if let Some(start) = start_timestamp {
                timestamps = timestamps.start(start);
            }
            if let Some(end) = end_timestamp {
                timestamps = timestamps.end(end);
            }

            activity_builder = activity_builder.timestamps(timestamps);
        }

        client
            .set_activity(activity_builder)
            .map_err(|e| e.to_string())?;
        Ok(())
    } else {
        Err("Discord client not connected".to_string())
    }
}

#[tauri::command]
pub fn disconnect_discord(state: State<DiscordState>) -> Result<(), String> {
    state.disconnect()
}