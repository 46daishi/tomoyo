import { invoke } from '@tauri-apps/api/core';

export async function lookupSentence(text) {
    try {
        return await invoke('lookup_sentence', { text });
    } catch (err) {
        console.error('lookup failed:', err);
        return [];
    }
}