// src/lib/stores/presence.svelte.js
export const presenceState = $state({
    mediaTitle: null
});

export function setMediaTitle(title) {
    presenceState.mediaTitle = title;
}