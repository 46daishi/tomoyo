<script>
    import { onMount, onDestroy } from 'svelte';
    import { page } from '$app/state';
    import { goto } from '$app/navigation';
    import ActionButton from '$lib/components/ActionButton.svelte';
    import { ICONS } from '$lib/icons';
    import { loadSettings } from '$lib/settings.js';
    import { getSentenceReviewPool } from '$lib/dictionary.js';
    import { getReviewWeighting, startReviewSession, endReviewSession, logReviewedItem } from '$lib/reviewStats.js';
    import InteractiveSentenceText from '$lib/components/InteractiveSentenceText.svelte';

    const mediaId = page.url.searchParams.get('media') ? Number(page.url.searchParams.get('media')) : null;

    let settings = $state(null);
    let sessionId = $state(null);
    let queue = $state([]);
    let index = $state(0);
    let translationOpen = $state(false);
    let done = $state(false);
    let completionStats = $state(null);
    let startTime = 0;

    let currentSentence = $derived(queue[index] ?? null);

    function weightedSample(items, weights, n) {
        const keyed = items.map((item, i) => ({ item, key: Math.pow(Math.random(), 1 / weights[i]) }));
        keyed.sort((a, b) => b.key - a.key);
        return keyed.slice(0, n).map((k) => k.item);
    }

    const desiredCountParam = page.url.searchParams.get('count');
    const onlyTranslatedParam = page.url.searchParams.get('onlyTranslated');
    
    async function loadQueue() {
        settings = await loadSettings();
    
        const onlyTranslated = onlyTranslatedParam !== null
            ? onlyTranslatedParam === '1'
            : settings.only_review_translated ?? false;
    
        const pool = await getSentenceReviewPool({ mediaId, onlyTranslated });
        if (pool.length === 0) {
            queue = [];
            return;
        }
    
        const weighting = await getReviewWeighting('sentence', null);
        const weights = pool.map((s) => 1 / (1 + (weighting[s.sentence_text]?.timesReviewed ?? 0)));
    
        const desired = desiredCountParam ? Number(desiredCountParam) : settings.sentence_review_count ?? 20;
        const n = Math.min(desired, pool.length);
        queue = weightedSample(pool, weights, n);
    }

    function formatDuration(seconds) {
        const m = Math.floor(seconds / 60);
        const s = Math.round(seconds % 60);
        return m > 0 ? `${m}m ${s}s` : `${s}s`;
    }

    async function advance() {
        if (!currentSentence) return;
        await logReviewedItem({ sessionId, reviewType: 'sentence', itemKey: currentSentence.sentence_text, mediaId });

        translationOpen = false;
        if (index + 1 >= queue.length) {
            await finishReview();
        } else {
            index += 1;
        }
    }

    async function finishReview() {
        await endReviewSession(sessionId);
        const totalSeconds = (Date.now() - startTime) / 1000;
        completionStats = {
            count: queue.length,
            totalSeconds,
            perReviewSeconds: queue.length > 0 ? totalSeconds / queue.length : 0,
        };
        done = true;
    }

    async function handleExit() {
        if (sessionId && !done) await endReviewSession(sessionId);
        history.back();
    }

    function handleKeydown(e) {
        if (done || !currentSentence) return;
        if (e.key === 'ArrowRight') {
            advance();
        }
    }

    onMount(async () => {
        startTime = Date.now();
        await loadQueue();
        sessionId = await startReviewSession('sentence', mediaId);
    });

    onDestroy(() => {
        if (sessionId && !done) endReviewSession(sessionId);
    });
</script>

<svelte:window onkeydown={handleKeydown} />

<main class="page review-page">
    <div class="review-header">
        <ActionButton icon={ICONS.back} variant="primary" size="small" onAction={handleExit} />
    </div>

    {#if done}
        <div class="review-complete">
            <h2>Review complete</h2>
            <p class="complete-summary">
                {completionStats.count} sentence{completionStats.count === 1 ? '' : 's'} reviewed in {formatDuration(completionStats.totalSeconds)}
                ({completionStats.perReviewSeconds.toFixed(1)}s per sentence)
            </p>
            <button class="modal-btn primary" onclick={() => history.back()}>
                Back to Reviews
            </button>
        </div>
    {:else if currentSentence}
        <div class="review-card">
            <div class="sentence-content-block">
                {#if settings?.sentence_review_text === 'plain'}
                    <p class="plain-sentence">{currentSentence.sentence_text}</p>
                {:else}
                    <InteractiveSentenceText text={currentSentence.sentence_text} {settings} mediaId={currentSentence.media_id ?? mediaId} />
                {/if}

                {#if currentSentence.translation}
                    <div class="translation-container">
                        <button class="translation-toggle" onclick={() => (translationOpen = !translationOpen)} aria-label="Toggle translation">
                            {@html ICONS.translate}
                        </button>
                        {#if translationOpen}
                            <p class="translation-text">{currentSentence.translation}</p>
                        {:else}
                            <p class="translation-text">...</p>
                        {/if}
                    </div>
                {/if}
            </div>
        </div>

        <div class="bottom-nav">
            <div class="review-counter">Progress: {index+1}/{queue.length}</div>
            <button class="next-btn" onclick={advance}>
                Next <span class="key-hint">{ICONS.right}</span>
            </button>
        </div>
    {:else}
        <p class="no-words-notice">No sentences available to review right now.</p>
    {/if}
</main>

<style>
    .review-page {
        position: relative;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: flex-start; /* <--- Pushes content toward top instead of vertical middle */
        padding: 6rem 2rem 2rem 2rem; /* <--- Extra top padding to clear the pinned back button */
        box-sizing: border-box;
        min-height: 100vh;
        gap: 2rem;
    }
    
    .review-header {
        position: absolute;
        top: 1.5rem;
        left: 1.5rem;
        z-index: 10;
    }
    
    .review-card {
        display: flex;
        flex-direction: column;
        align-items: center;
        width: 100%;
        max-width: 1000px;
        margin-top: 2rem; /* Adjust this to push sentence up or down slightly */
    }
    
    .sentence-content-block {
        display: inline-flex;
        flex-direction: column;
        align-items: flex-start;
        max-width: 100%;
    }

    /* Centered Bottom Navigation */
    .bottom-nav {
        width: 100%;
        max-width: 500px;
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 1.5rem;
        margin-top: auto;
        padding-top: 1rem;
    }

    .review-counter {
        font-size: 0.95rem;
        font-weight: 700;
        color: var(--theme-textSecondary, #b3b3b3);
        font-variant-numeric: tabular-nums;
    }

    .next-btn {
        display: flex;
        align-items: center;
        gap: 0.4rem;
        background: transparent;
        border: 1px solid transparent;
        border-radius: 10px;
        padding: 0.4rem 0.8rem;
        color: var(--theme-text, #f6f6f6);
        font: inherit;
        font-size: 0.85rem;
        font-weight: 600;
        cursor: pointer;
        transition: border-color 0.15s ease, background 0.15s ease;
    }

    .next-btn:hover {
        border-color: var(--theme-primary, #36b7bd);
        transform: translateY(-1px);
    }

    .key-hint {
        font-family: "Symbols Nerd Font";
        font-size: 0.7rem;
        color: var(--theme-textSecondary, #b3b3b3);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 4px;
        padding: 0rem 0.38rem;
    }

    .plain-sentence {
        font-family: "Noto Sans JP", Inter, sans-serif;
        font-size: 1.6rem;
        font-weight: 700;
        line-height: 1.6;
        color: var(--theme-text, #f6f6f6);
        margin: 0;
    }

    .translation-container {
        display: flex;
        align-items: center;
        gap: 0.6rem;
        margin-top: 1.5rem;
        padding-left: 0.4rem;
    }
    
    .translation-toggle {
        margin: 0;
        background: none;
        border: none;
        padding: 0;
        color: var(--theme-textSecondary, #b3b3b3);
        font-size: 1.7rem;
        cursor: pointer;
        display: flex;
        align-items: center;
        justify-content: center;
        transition: color 0.15s ease;
    }
    
    .translation-toggle:hover {
        color: var(--theme-text, #f6f6f6);
    }
    
    .translation-text {
        margin: 0;
        font-size: 1.2rem !important;
        color: var(--theme-textSecondary, #b3b3b3);
        text-align: left;
        line-height: 1.4;
    }
    
    button.translation-toggle {
        font-family: "Symbols Nerd Font";
    }
    
    .translation-text {
        margin: 0;
        font-size: 1rem;
        color: var(--theme-textSecondary, #b3b3b3);
        text-align: left;
    }

    .review-complete,
    .no-words-notice,
    .complete-summary {
        color: var(--theme-textSecondary, #b3b3b3);
        text-align: center;
    }
</style>