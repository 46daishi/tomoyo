<script>
    import { page } from '$app/state';
    import { goto } from '$app/navigation';
    import { onMount, onDestroy } from 'svelte';
    import ActionButton from '$lib/components/ActionButton.svelte';
    import AppImage from '$lib/components/AppImage.svelte';
    import { ICONS } from '$lib/icons';
    import { STATUS_LEVELS, statusLevelColor, resolveStatusLevels } from '$lib/constants.js';
    import { loadSettings } from '$lib/settings.js';
    import { getReviewPool, updateWordStatus, parseDefinitions, getSentencesForWord } from '$lib/dictionary.js';
    import { getReviewWeighting, startReviewSession, endReviewSession, logReviewedItem } from '$lib/reviewStats.js';
    import { distributeFurigana } from '$lib/furigana.js';
    import { setReviewProgress } from '$lib/stores/presence.svelte';
    
    const mediaId = page.url.searchParams.get('media') ? Number(page.url.searchParams.get('media')) : null;

    let settings = $state(null);
    let sessionId = $state(null);
    let queue = $state([]);
    let index = $state(0);
    let revealed = $state(false);
    let done = $state(false);
    let completionStats = $state(null);
    let startTime = 0;
    let advancing = $state(false);
    // Cards already logged this session (Back + Next over the same card
    // must not double-count it in review weighting).
    const loggedItemKeys = new Set();

    let currentWord = $derived(queue[index] ?? null);
    let currentSentence = $state(null);
    let mode = $derived(settings?.default_review_mode ?? 'normal');
    let isRevealed = $derived(mode === 'normal' || revealed);

    function weightedSample(items, weights, n) {
        const keyed = items.map((item, i) => ({ item, key: Math.pow(Math.random(), 1 / weights[i]) }));
        keyed.sort((a, b) => b.key - a.key);
        return keyed.slice(0, n).map((k) => k.item);
    }

    const desiredCountParam = page.url.searchParams.get('count');
    const statusesParam = page.url.searchParams.get('statuses');
    const onlyWithImagesParam = page.url.searchParams.get('onlyWithImages');
    
    async function loadQueue() {
        settings = await loadSettings();
    
        const statuses = statusesParam
            ? statusesParam.split(',').map(Number)
            : settings.review_statuses ?? [0, 1, 2, 3];
    
        const pool = await getReviewPool({ mediaId, statuses, onlyWithImages: onlyWithImagesParam === '1' });
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

    function splitHighlighted(sentence) {
        const s = sentence?.sentence_text ?? '';
        const start = Number.isFinite(sentence?.highlight_start) ? sentence.highlight_start : -1;
        const end = Number.isFinite(sentence?.highlight_end) ? sentence.highlight_end : -1;
        if (start < 0 || end <= start || start > s.length || end > s.length) return null;
        return {
            before: s.slice(0, start),
            highlight: s.slice(start, end),
            after: s.slice(end),
        };
    }

    async function loadRandomSentence(wordId) {
        currentSentence = null;
        const sentences = await getSentencesForWord(wordId);
        if (sentences.length === 0) {
            currentSentence = null;
            return;
        }
        currentSentence = sentences[Math.floor(Math.random() * sentences.length)];
    }

    function advance() {
        setReviewProgress(queue.length > 0
            ? `${index+1}/${queue.length} reviewed`
            : null);
        revealed = false;
        if (index + 1 >= queue.length) {
            finishReview();
        } else {
            index += 1;
        }
    }

    function goBack() {
        if (index <= 0 || !currentWord || advancing) return;
        index -= 1;
        revealed = false;
        setReviewProgress(queue.length > 0
            ? `${index+1}/${queue.length} reviewed`
            : null);
    }

    async function selectStatus(status) {
        if (!currentWord || advancing) return;
        advancing = true;
        try {
            await updateWordStatus({ wordId: currentWord.id, status });
            if (!loggedItemKeys.has(currentWord.id)) {
                await logReviewedItem({ sessionId, reviewType: 'word', itemKey: currentWord.id, mediaId });
                loggedItemKeys.add(currentWord.id);
            }
            advance();
        } finally {
            advancing = false;
        }
    }

    async function skipNext() {
        if (!currentWord || !isRevealed || advancing) return;
        advancing = true;
        try {
            if (!loggedItemKeys.has(currentWord.id)) {
                await logReviewedItem({ sessionId, reviewType: 'word', itemKey: currentWord.id, mediaId });
                loggedItemKeys.add(currentWord.id);
            }
            advance();
        } finally {
            advancing = false;
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
        if (e.key === 'ArrowLeft' && index > 0) {
            goBack();
        }
    }

    onMount(async () => {
        startTime = Date.now();
        await loadQueue();
        sessionId = await startReviewSession('word', mediaId);
        setReviewProgress(queue.length > 0
            ? `${index}/${queue.length} reviewed`
            : null);
    });

    $effect(() => {
        const id = currentWord?.id;
        if (id != null) loadRandomSentence(id);
    });

    onDestroy(() => {
        if (sessionId && !done) endReviewSession(sessionId);
        setReviewProgress(null);
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
        {@const rFurigana = distributeFurigana(currentWord.spelling ?? '', currentWord.reading ?? '')}
        {@const showRuby = rFurigana.some((s) => s.reading)}
        <div class="review-card">
            <div class="review-word"><span class="review-word-text">{#if showRuby}{#each rFurigana as seg}{#if seg.reading}<ruby><rb>{seg.text}</rb><rt class="entry-reading-kana" class:concealed={!isRevealed}>{seg.reading}</rt></ruby>{:else}{seg.text}{/if}{/each}{:else}{currentWord.spelling}{/if}</span></div>

            <div class="current-status-badge" style={`--status-color: ${statusLevelColor(settings, currentWord.status)}`}>
                {STATUS_LEVELS[currentWord.status]?.label}
            </div>

            {#if isRevealed}
                {#if !showRuby && currentWord.reading && currentWord.reading !== currentWord.spelling}
                    <div class="review-reading">{currentWord.reading}</div>
                {/if}
                <div class="review-definitions">{parseDefinitions(currentWord.definitions).join('; ')}</div>
                {#if currentWord.image_path}
                    <div class="review-image-row">
                        <AppImage path={currentWord.image_path} class="review-image" alt="" />
                    </div>
                {/if}
                {#if currentSentence}
                    <div class="review-sentence-wrap">
                        {#if splitHighlighted(currentSentence)}
                            <p class="review-sentence">
                                {splitHighlighted(currentSentence).before}<mark class="sentence-highlight">{splitHighlighted(currentSentence).highlight}</mark>{splitHighlighted(currentSentence).after}
                            </p>
                        {:else}
                            <p class="review-sentence">{currentSentence.sentence_text}</p>
                        {/if}
                        {#if currentSentence.translation}
                            <p class="review-sentence-translation">{currentSentence.translation}</p>
                        {/if}
                    </div>
                {/if}
            {/if}
        </div>

        {#if !isRevealed}
            {#if currentSentence}
                <div class="review-sentence-wrap">
                    {#if splitHighlighted(currentSentence)}
                        <p class="review-sentence">
                            {splitHighlighted(currentSentence).before}<mark class="sentence-highlight">{splitHighlighted(currentSentence).highlight}</mark>{splitHighlighted(currentSentence).after}
                        </p>
                    {:else}
                        <p class="review-sentence">{currentSentence.sentence_text}</p>
                    {/if}
                </div>
            {/if}
            <button class="reveal-btn" onclick={() => (revealed = true)}>
                Show details <span class="key-hint">Space</span>
            </button>
        {:else}
            <div class="status-btn-row">
                {#each resolveStatusLevels(settings) as level, i}
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
            <button class="next-btn" onclick={goBack} disabled={index === 0}>
                <span class="key-hint">{ICONS.left}</span> Back
            </button>
            <div class="review-counter">{index+1}/{queue.length}</div>
            <button class="next-btn" onclick={skipNext} disabled={!isRevealed}>
                Next <span class="key-hint">{ICONS.right}</span>
            </button>
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
        font-size: 1rem;
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
    }

    .next-btn:disabled {
        opacity: 0.35;
        cursor: default;
    }

    .next-btn:disabled:hover {
        border-color: transparent;
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
        max-width: 800px;
    }

    .current-status-badge {
        font-size: 0.63rem;
        font-weight: 800;
        text-transform: uppercase;
        letter-spacing: 0.04em;
        padding: 0rem 0.4rem;
        margin-top: -0.4rem;
        border-radius: 5px;
        color: black;
        background: color-mix(in srgb, var(--status-color, #6c7086) 85%, transparent);
    }

    /* Review ruby runs much larger than tooltip/dictionary furigana —
       scoped here so the shared global size is untouched. */
    .review-word .entry-reading-kana {
        font-size: 1.3rem;
    }

    /* Pre-reveal ruby is invisible but reserves its exact space, so
       revealing the answer never moves the word or the pill. */
    .review-word .concealed {
        visibility: hidden;
    }

    /* Zero out the reading's line box: the annotation then paints as
       pure overflow and contributes no descent, so ruby and kana words
       share byte-identical metrics below the baseline. */
    .review-word rt {
        line-height: 0;
    }

    .review-word {
        font-family: "Noto Sans JP", Inter, sans-serif;
        font-size: 3rem;
        font-weight: 700;
        color: var(--theme-text, #f6f6f6);
        /* Fixed box, content bottom-aligned through a single inner span:
           ruby and kana share the same font strut, so base glyphs land at
           identical positions, ruby overflows upward visibly, and the pill
           below always lands at the same distance — kana, ruby, revealed
           or not. A wrapping long word overflows upward too, never into
           the pill. */
        display: flex;
        flex-direction: column;
        justify-content: flex-end;
        height: 3rem;
        line-height: 1;
    }

    .review-image-row {
        display: flex;
        justify-content: center;
        margin-top: 0.6rem;
    }

    :global(.review-image) {
        max-width: min(420px, 80vw);
        max-height: 60vh;
        border-radius: 12px;
        border: 1px solid var(--theme-border, #404040);
        object-fit: contain;
        pointer-events: none;
        user-select: none;
    }

    .review-reading {
        padding-top:0.7rem;
        font-size: 1.2rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .review-definitions {
        font-size: 1.2rem;
        color: var(--theme-text, #f6f6f6);
        max-width: 420px;
        margin-top: 0.6rem;
    }

    .review-sentence-wrap {
        margin-top: 1.25rem;
        padding-top: 1rem;
        border-top: 1px solid var(--theme-border, #404040);
        max-width: 800px;
    }

    .review-sentence {
        margin: 0;
        font-family: "Noto Sans JP", Inter, sans-serif;
        font-size: 1.6rem;
        line-height: 1.6;
        color: var(--theme-text, #f6f6f6);
    }

    .review-sentence-translation {
        margin: 0.4rem 0 0;
        font-size: 1.1rem;
        color: var(--theme-text, #b3b3b3);
    }

    .sentence-highlight {
        margin: 0;
        padding: 0 0.1em;
        border-radius: 3px;
        background: none;
        color: var(--theme-primary, #f6f6f6);
        font-weight: 600;
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