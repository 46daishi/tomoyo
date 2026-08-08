// src/lib/stores/presence.svelte.js
export const presenceState = $state({
    mediaTitle: null,
    reviewProgress: null, // new
});

export function setMediaTitle(title) {
    presenceState.mediaTitle = title;
}

export function setReviewProgress(progress) {
  presenceState.reviewProgress = progress;
}