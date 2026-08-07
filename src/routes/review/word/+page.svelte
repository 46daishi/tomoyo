<script>
    import { page } from '$app/state';
    import { goto } from '$app/navigation';
    import { onMount, onDestroy } from 'svelte';
    import ActionButton from '$lib/components/ActionButton.svelte';
    import { ICONS } from '$lib/icons';
    import { STATUS_LEVELS } from '$lib/constants.js';
    import { loadSettings } from '$lib/settings.js';
    import { getReviewPool, updateWordStatus } from '$lib/dictionary.js';
    import { getReviewWeighting, startReviewSession, endReviewSession, logReviewedItem } from '$lib/reviewStats.js';

    const mediaId = page.url.searchParams.get('media') ? Number(page.url.searchParams.get('media')) : null;

    let settings = $state(null);
    let sessionId = $state(null);
    let queue = $state([]);
    let index = $state(0);
    let revealed = $state(false);
    let done = $state(false);
    let completionStats = $state(null);
    let startTime = 0;

    let currentWord = $derived(queue[index] ?? null);
    let mode = $derived(settings?.default_review_mode ?? 'normal');
    let isRevealed = $derived(mode === 'normal' || revealed);

    function weightedSample(items, weights, n) {
        const keyed = items.map((item, i) => ({ item, key: Math.pow(Math.random(), 1 / weights[i]) }));
        keyed.sort((a, b) => b.key - a.key);
        return keyed.slice(0, n).map((k) => k.item);
    }

    const desiredCountParam = page.url.searchParams.get('count');
    const statusesParam = page.url.searchParams.get('statuses');
    
    async function loadQueue() {
        settings = await loadSettings();
    
        const statuses = statusesParam
            ? statusesParam.split(',').map(Number)
            : settings.review_statuses ?? [0, 1, 2, 3];
    
        const pool = await getReviewPool({ mediaId, statuses });
        if (pool.length === 0) {
            queue = [];
            return;
        }
    
        const weighting = await getReviewWeighting('word', null);
        const weights = pool.map((w) => 1 / (1 + (weighting[String(w.id)]?.timesReviewed ?? 0)));
    
        const desired = desiredCountParam ? Number(desiredCountParam) : settings.word_review_count ?? 20;
        const n = Math.min(desired, pool.length);
        queue = weightedSample(pool, weights, n);
    }

    function formatDuration(seconds) {
        const m = Math.floor(seconds / 60);
        const s = Math.round(seconds % 60);
        return m > 0 ? `${m}m ${s}s` : `${s}s`;
    }

    function advance() {
        revealed = false;
        if (index + 1 >= queue.length) {
            finishReview();
        } else {
            index += 1;
        }
    }

    async function selectStatus(status) {
        if (!currentWord) return;
        await updateWordStatus({ wordId: currentWord.id, status });
        await logReviewedItem({ sessionId, reviewType: 'word', itemKey: currentWord.id, mediaId });
        advance();
    }

    async function skipNext() {
        if (!currentWord || !isRevealed) return;
        await logReviewedItem({ sessionId, reviewType: 'word', itemKey: currentWord.id, mediaId });
        advance();
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
        if (done || !currentWord) return;

        if (e.code === 'Space') {
            e.preventDefault();
            if (mode === 'flashcard' && !revealed) revealed = true;
            return;
        }
        if (e.key >= '0' && e.key <= '4' && isRevealed) {
            selectStatus(Number(e.key));
            return;
        }
        if (e.key === 'ArrowRight' && isRevealed) {
            skipNext();
        }
    }

    onMount(async () => {
        startTime = Date.now();
        await loadQueue();
        sessionId = await startReviewSession('word', mediaId);
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
                {completionStats.count} word{completionStats.count === 1 ? '' : 's'} reviewed in {formatDuration(completionStats.totalSeconds)}
                ({completionStats.perReviewSeconds.toFixed(1)}s per word)
            </p>
            <button class="modal-btn primary" onclick={() => { if (sessionId && !done) endReviewSession(sessionId); history.back(); }}>
                Back to Reviews
            </button>
        </div>
    {:else if currentWord}
        <div class="review-card">
            <div class="current-status-badge" style={`--status-color: ${STATUS_LEVELS[currentWord.status]?.color}`}>
                {STATUS_LEVELS[currentWord.status]?.label}
            </div>

            <div class="review-word">{currentWord.spelling}</div>

            {#if isRevealed}
                <div class="review-reading">{currentWord.reading}</div>
                <div class="review-definitions">{JSON.parse(currentWord.definitions).join('; ')}</div>
            {/if}
        </div>

        {#if !isRevealed}
            <button class="reveal-btn" onclick={() => (revealed = true)}>
                Show details <span class="key-hint">Space</span>
            </button>
        {:else}
            <div class="status-btn-row">
                {#each STATUS_LEVELS as level, i}
                    <button 
                        type="button" 
                        class="status-btn" 
                        class:active={i === currentWord.status}
                        onclick={() => selectStatus(i)}
                    >
                        <span class="status-dot" style={`--dot-color: ${level.color}`}>{i}</span>
                        {level.label}
                    </button>
                {/each}
            </div>
        {/if}

        <div class="bottom-nav">
            <div class="review-counter">Progress: {index+1}/{queue.length}</div>
            {#if isRevealed}
                <button class="next-btn" onclick={skipNext}>
                    Next <span class="key-hint">{ICONS.right}</span>
                </button>
            {/if}
        </div>
    {:else}
        <p class="no-words-notice">No words available to review right now.</p>
    {/if}
</main>

<style>
    .review-page {
        position: relative;
        display: flex;
        flex-direction: column;
        align-items: center;
        padding: 6rem;
        box-sizing: border-box;
        min-height: 100vh;
        gap: 2rem;
    }
    
    .review-header {
        position: absolute;
        top: 1.5rem;
        left: 1.5rem;
        z-index: 10;
        width: auto;
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

    .review-card {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 0.75rem;
        text-align: center;
        max-width: 500px;
    }

    .current-status-badge {
        font-size: 0.63rem;
        font-weight: 800;
        text-transform: uppercase;
        letter-spacing: 0.04em;
        padding: 0rem 0.4rem;
        margin-bottom:0.2rem;
        border-radius: 5px;
        color: black;
        background: color-mix(in srgb, var(--status-color, #6c7086) 85%, transparent);
    }

    .review-word {
        font-family: "Noto Sans JP", Inter, sans-serif;
        font-size: 3rem;
        font-weight: 700;
        color: var(--theme-text, #f6f6f6);
    }

    .review-reading {
        padding-top:0.7rem;
        font-size: 1.2rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .review-definitions {
        font-size: 1rem;
        color: var(--theme-text, #f6f6f6);
        max-width: 420px;
    }

    .reveal-btn {
        display: flex;
        align-items: center;
        gap: 0.6rem;
        background: transparent;
        border: 1px solid var(--theme-primary, #36b7bd);
        border-radius: 10px;
        padding: 0.35rem 0.75rem;
        color: #fff;
        font: inherit;
        font-size: 0.8rem;
        font-weight: 700;
        cursor: pointer;
        transition: background 0.15s ease, transform 0.15s ease;
    }

    .reveal-btn:hover {
        background: var(--theme-primaryHover, #17a4ab);
        transform: translateY(-1px);
    }

    .reveal-btn .key-hint {
        font-size: 0.6rem;
        border-color: rgba(255, 255, 255, 0.3);
        color: #fff;
        padding: 0rem 0.2rem;
    }

    .status-btn-row {
        display: flex;
        flex-wrap: wrap;
        justify-content: center;
        gap: 0.5rem;
    }

    .status-btn {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        background: var(--theme-surface, #2d2d2d);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 8px;
        padding: 0.45rem 0.75rem;
        font: inherit;
        font-size: 0.85rem;
        color: var(--theme-text, #f6f6f6);
        cursor: pointer;
        transition: background 0.15s ease, border-color 0.15s ease, transform 0.15s ease;
    }

    .status-btn:hover {
        background: color-mix(in srgb, var(--theme-primary, #36b7bd) 15%, transparent);
        border-color: color-mix(in srgb, var(--theme-primary, #36b7bd) 40%, transparent);
        transform: translateY(-1px);
    }

    .status-btn.active {
        font-weight: 700;
        background: color-mix(in srgb, var(--theme-primary, #36b7bd) 10%, transparent);
        border-color: var(--theme-primary, #36b7bd);
    }

    .status-dot {
        width: 1.25rem;
        height: 1.25rem;
        border-radius: 50%;
        background: var(--dot-color, #6c7086);
        flex-shrink: 0;
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 0.7rem;
        font-weight: 800;
        color: #000;
        text-shadow: 0 1px 0 rgba(255, 255, 255, 0.25);
    }

    .review-complete,
    .no-words-notice,
    .complete-summary {
        color: var(--theme-textSecondary, #b3b3b3);
        text-align: center;
    }
</style>