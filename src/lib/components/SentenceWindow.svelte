<script>
    import { isMostlyJapanese } from '$lib/japaneseDetect.js';
    import { lookupAtPosition } from '$lib/lookup.js';
    import { startClipboardListener, stopClipboardListener } from '$lib/clipboardListener.js';
    import { logLookupEvent } from '$lib/lookupEvents.js';
    import { mineWord } from '$lib/dictionary.js';
    import LookupTooltip from '$lib/components/LookupTooltip.svelte';

    let { settings, miniMode, session, mediaId, mediaTag } = $props();

    let currentText = $state('');
    let historyEntries = $state([]);
    let historyIndex = $state(0);

    let displayedText = $derived(
        historyIndex === 0 ? currentText : (historyEntries[historyIndex - 1] ?? currentText)
    );
    let displayedChars = $derived([...displayedText]);
    let viewingHistory = $derived(historyIndex > 0);

    let hoveredSpan = $state(null);
    let tooltipSpan = $state(null);
    let tooltipVisible = $state(false);
    let tooltipX = $state(0);
    let tooltipY = $state(0);
    let tooltipMaxHeight = $state(300);
    let hotkeyHeld = $state(false);

    let cycleSkip = 0;
    let hoverRequestId = 0;
    let lastHoverEl = null;
    let sentenceWindowEl = $state(null);

    async function handleClipboardChange(text) {
        if (!isMostlyJapanese(text)) return;

        if (settings?.history_enabled && currentText) {
            const span = settings.history_span ?? 50;
            historyEntries = [currentText, ...historyEntries].slice(0, span);
        }

        currentText = text;
        historyIndex = 0;
        hoveredSpan = null;
        tooltipSpan = null;
        tooltipVisible = false;
        cycleSkip = 0;

        session?.recordSentence(text.length);
    }

    $effect(() => {
        if (session?.running) {
            startClipboardListener(handleClipboardChange);
        } else {
            stopClipboardListener();
        }

        return () => {
            stopClipboardListener();
        };
    });

    // Close any open tooltip whenever mini mode toggles — the whole
    // sentence window changes shape/size, so a previously positioned
    // tooltip would otherwise be left stranded at stale coordinates.
    $effect(() => {
        miniMode; // tracked dependency
        tooltipVisible = false;
    });

    function calculateTooltipCoords(targetEl) {
        if (!targetEl || !sentenceWindowEl) return { x: 0, y: 0, maxHeight: 300 };
    
        const charRect = targetEl.getBoundingClientRect();
        const containerRect = sentenceWindowEl.getBoundingClientRect();
    
        const tooltipWidth = miniMode ? 260 : 420;
    
        const rawX = charRect.left - containerRect.left;
        const maxX = containerRect.width - tooltipWidth - 12;
        const x = Math.max(8, Math.min(rawX, maxX));
    
        const y = charRect.bottom - containerRect.top + 6;
    
        const bottomMargin = 16;
        const availableHeight = window.innerHeight - charRect.bottom - 6 - bottomMargin;
        const maxHeight = Math.max(80, availableHeight);
    
        return { x, y, maxHeight };
    }

    function positionTooltipUnderChar(charEl) {
        const coords = calculateTooltipCoords(charEl);
        tooltipX = coords.x;
        tooltipY = coords.y;
        tooltipMaxHeight = coords.maxHeight;
    }

    function openTooltipAndLog(span, charEl) {
        tooltipSpan = span;
        tooltipVisible = true;

        logLookupEvent({
            mediaId,
            wordId: span.entries[0]?.id ?? null,
            surfaceText: span.surface,
        });

        positionTooltipUnderChar(charEl);
    }

    function closeHoverTooltip() {
        tooltipVisible = false;
    }

    async function handleCharHover(index, event) {
        const charEl = event.currentTarget;
        lastHoverEl = charEl;

        if (hoveredSpan && index >= hoveredSpan.start && index < hoveredSpan.end) return;

        cycleSkip = 0;
        const requestId = ++hoverRequestId;
        const result = await lookupAtPosition(displayedText, index);
        if (requestId !== hoverRequestId) return;

        hoveredSpan = result;
        if (!result) return;

        if (settings?.lookup_mode === 'hover') {
            openTooltipAndLog(result, charEl);
        }
    }

    async function handleCycleShorter() {
        if (!hoveredSpan) return;

        const anchorPos = hoveredSpan.start;
        const nextSkip = cycleSkip + 1;
        const requestId = ++hoverRequestId;
        let result = await lookupAtPosition(displayedText, anchorPos, nextSkip);
        if (requestId !== hoverRequestId) return;

        if (result) {
            cycleSkip = nextSkip;
            hoveredSpan = result;
        } else {
            cycleSkip = 0;
            result = await lookupAtPosition(displayedText, anchorPos, 0);
            if (requestId !== hoverRequestId) return;
            hoveredSpan = result;
        }

        if (result && tooltipVisible) {
            tooltipSpan = result;
        }
    }

    function isSameSpan(a, b) {
        return !!a && !!b && a.start === b.start && a.end === b.end && a.surface === b.surface;
    }

    function handleGlobalKeydown(event) {
        if (
            settings?.lookup_mode === 'hotkey' &&
            settings?.lookup_hotkey &&
            event.code === settings.lookup_hotkey &&
            !event.repeat
        ) {
            hotkeyHeld = true;

            if (hoveredSpan && lastHoverEl && lastHoverEl.isConnected) {
                if (tooltipVisible && isSameSpan(tooltipSpan, hoveredSpan)) {
                    tooltipVisible = false;
                } else {
                    openTooltipAndLog(hoveredSpan, lastHoverEl);
                }
            }
        }

        if (settings?.cycle_key && event.code === settings.cycle_key && !event.repeat) {
            event.preventDefault();
            handleCycleShorter();
        }
    }

    function handleGlobalKeyup(event) {
        if (settings?.lookup_mode === 'hotkey' && settings?.lookup_hotkey && event.code === settings.lookup_hotkey) {
            hotkeyHeld = false;
        }
    }

    function handleCharClick(index, event) {
        if (hoveredSpan && index >= hoveredSpan.start && index < hoveredSpan.end) {
            const clickModeActive = !settings?.lookup_mode || settings.lookup_mode === 'click';

            if (clickModeActive) {
                if (tooltipVisible && isSameSpan(tooltipSpan, hoveredSpan)) {
                    tooltipVisible = false;
                } else {
                    openTooltipAndLog(hoveredSpan, event.currentTarget);
                }
            } else if (tooltipVisible && isSameSpan(tooltipSpan, hoveredSpan)) {
                tooltipVisible = false;
            }
            event.stopPropagation();
        }
    }

    function isRelatedTargetInSentenceArea(relatedTarget) {
        if (!relatedTarget?.closest) return false;
        return relatedTarget.closest('.char-token') || relatedTarget.closest('.lookup-tooltip');
    }

    function handleSentenceLeave(event) {
        hoveredSpan = null;
        cycleSkip = 0;
        lastHoverEl = null;

        if (settings?.lookup_mode === 'hover' && !isRelatedTargetInSentenceArea(event.relatedTarget)) {
            closeHoverTooltip();
        }
    }

    function handleSentenceWheel(e) {
        if (!settings?.history_enabled) return;
        if (historyEntries.length === 0 && historyIndex === 0) return;
        e.preventDefault();

        if (e.deltaY < 0) {
            const maxIndex = Math.min(historyEntries.length, settings.history_span ?? 50);
            const newIndex = Math.min(historyIndex + 1, maxIndex);
            if (newIndex !== historyIndex) {
                historyIndex = newIndex;
                hoveredSpan = null;
                tooltipVisible = false;
                cycleSkip = 0;
            }
        } else if (e.deltaY > 0) {
            const newIndex = Math.max(historyIndex - 1, 0);
            if (newIndex !== historyIndex) {
                historyIndex = newIndex;
                hoveredSpan = null;
                tooltipVisible = false;
                cycleSkip = 0;
            }
        }
    }

    async function handleMineWord(span, entry) {
        if (!span || !entry) return;

        await mineWord({
            dictId: entry.id,
            spelling: entry.spellings[0] ?? span.surface,
            reading: entry.readings[0] ?? '',
            definitions: entry.definitions,
            wordType: entry.pos.join(', '),
            tag: mediaTag ?? 'mined',
            sentenceText: displayedText,
            highlightStart: span.start,
            highlightEnd: span.end,
            mediaId,
        });
    }

    $effect(() => {
        if (settings?.history_span != null && historyEntries.length > settings.history_span) {
            historyEntries = historyEntries.slice(0, settings.history_span);
        }
    });
</script>

<svelte:window
    onclick={() => { tooltipVisible = false; }}
    onkeydown={handleGlobalKeydown}
    onkeyup={handleGlobalKeyup}
/>

<div class="sentence-window" bind:this={sentenceWindowEl} onwheel={handleSentenceWheel}>
    {#if displayedChars.length > 0}
        <p
            class="sentence-text"
            class:history-text={viewingHistory}
            onmouseleave={handleSentenceLeave}
            style={`--font-size: ${settings?.font_size ?? 30}px; --font-family: '${settings?.font_family ?? 'Noto Sans JP'}'`}
        >
            {#each displayedChars as char, i}
                <span
                    class="char-token"
                    class:hovered={hoveredSpan && i >= hoveredSpan.start && i < hoveredSpan.end && settings?.word_highlight_enabled}
                    class:span-start={hoveredSpan && i === hoveredSpan.start && settings?.word_highlight_enabled}
                    class:span-end={hoveredSpan && i === hoveredSpan.end - 1 && settings?.word_highlight_enabled}
                    class:no-match={hoveredSpan && i >= hoveredSpan.start && i < hoveredSpan.end && hoveredSpan.entries.length === 0 && settings?.word_highlight_enabled}
                    onmouseenter={(event) => handleCharHover(i, event)}
                    onclick={(event) => handleCharClick(i, event)}
                >{char}</span>
            {/each}
        </p>

        {#if tooltipVisible && tooltipSpan}
            <LookupTooltip
                {tooltipSpan}
                {settings}
                {tooltipX}
                {tooltipY}
                {tooltipMaxHeight}
                onMine={(entry) => handleMineWord(tooltipSpan, entry)}
                onMouseLeave={(e) => {
                    if (settings?.lookup_mode === 'hover' && !isRelatedTargetInSentenceArea(e.relatedTarget)) {
                        closeHoverTooltip();
                    }
                }}
            />
        {/if}
    {:else}
        <p class="sentence-placeholder">Waiting for a sentence…</p>
    {/if}
</div>

<style>
    .sentence-window {
        position: relative;
        width: 100%;
        max-width: 800px;
        min-height: 200px;
        margin-top: 2rem;
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 70%, #000);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 16px;
        display: flex;
        align-items: top;
        justify-content: center;
        padding: 2rem 2.5rem;
        box-sizing: border-box;
        container-type: inline-size; /* enables cqw units below, scoped to this element's own width */
    }
    

    .sentence-text {
        font-family: var(--font-family, "Noto Sans JP"), Inter, sans-serif;
        color: var(--theme-text, #f6f6f6);
        font-size: var(--font-size, 30px);
        font-weight: 700;
        line-height: 1.6;
        text-align: left;
        margin: 0;
        width: 100%;
        transition: color 0.15s ease;
    }

    .sentence-text.history-text {
        color: #ffe14d;
    }

    .sentence-placeholder {
        color: var(--theme-textSecondary, #b3b3b3);
        font-size: 1rem;
        text-align: center;
        margin: 0;
    }

    .char-token {
        cursor: pointer;
        margin: 0;
        padding: 0;
        border-radius: 0;
        transition: background 0.1s ease;
    }

    .char-token.hovered {
        background: color-mix(in srgb, var(--theme-primary, #36b7bd) 25%, transparent);
    }

    .char-token.hovered.no-match {
        background: color-mix(in srgb, var(--theme-textSecondary, #b3b3b3) 20%, transparent);
        cursor: default;
    }

    .char-token.hovered.span-start {
        border-top-left-radius: 4px;
        border-bottom-left-radius: 4px;
    }

    .char-token.hovered.span-end {
        border-top-right-radius: 4px;
        border-bottom-right-radius: 4px;
    }

    :global(body.mini-mode) .sentence-window {
        width: 100vw;
        height: 100vh;
        max-width: none;
        min-height: 0;
        margin: 0;
        border-radius: 0;
        border: none;
        padding: 1.5rem;
        background: color-mix(
            in srgb,
            color-mix(in srgb, var(--theme-surface, #1e1e2e) 80%, black 20%) var(--mini-color-weight, 70%),
            transparent
        );
    }
</style>