import { invoke } from '@tauri-apps/api/core';

let cached = null;

export async function loadSettings() {
    cached = await invoke('get_settings');
    return cached;
}

export async function saveSettings(settings) {
    await invoke('save_settings', { settings });
    cached = settings;
}

export function getCachedSettings() {
    return cached;
}