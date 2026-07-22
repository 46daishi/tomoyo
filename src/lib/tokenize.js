import { invoke } from '@tauri-apps/api/core';

export async function tokenizeSentence(text) {
    try {
        return await invoke('tokenize_text', { text });
    } catch (err) {
        console.error('tokenize failed:', err);
        return [];
    }
}