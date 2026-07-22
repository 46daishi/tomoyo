import { readText } from '@tauri-apps/plugin-clipboard-manager';

const POLL_INTERVAL_MS = 500;

let intervalHandle = null;
let lastText = null;

/**
 * Starts polling the clipboard. Calls `onChange(text)` whenever the
 * clipboard content differs from the last seen value.
 */
export function startClipboardListener(onChange) {
    if (intervalHandle) return; // already running, don't double-start

    intervalHandle = setInterval(async () => {
        try {
            const text = await readText();
            if (text && text !== lastText) {
                lastText = text;
                onChange(text);
            }
        } catch (err) {
            console.error('clipboard read failed:', err);
        }
    }, POLL_INTERVAL_MS);
}

export function stopClipboardListener() {
    if (intervalHandle) {
        clearInterval(intervalHandle);
        intervalHandle = null;
    }
    lastText = null; // reset so a stale value doesn't carry into the next session
}