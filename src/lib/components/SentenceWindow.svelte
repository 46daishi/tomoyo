<script>
    import { isMostlyJapanese } from '$lib/japaneseDetect.js';
    import { lookupAtPosition } from '$lib/lookup.js';
    import { showTooltipAt, hideTooltip } from '$lib/tooltipWindow.js';
    import { startClipboardListener, stopClipboardListener } from '$lib/clipboardListener.js';
    import LookupTooltip from '$lib/components/LookupTooltip.svelte';

    let { settings, miniMode, session } = $props();

    // Sentence & History State
    let currentText = $state('');
    let historyEntries = $state([]);
    let historyIndex = $state(0);

    let displayedText = $derived(
        historyIndex === 0 ? currentText : (historyEntries[historyIndex - 1] ?? currentText)
    );
    let displayedChars = $derived([...displayedText]);
    let viewingHistory = $derived(historyIndex > 0);

    // Hover & Tooltip State
    let hoveredSpan = $state(null);
    let tooltipSpan = $state(null);
    let tooltipVisible = $state(false);
    let tooltipX = $state(0);
    let tooltipY = $state(0);
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
    }

    // Automatically manage clipboard listener when session state changes or component unmounts
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

    function calculateTooltipCoords(targetEl) {
        if (!targetEl || !sentenceWindowEl) return { x: 0, y: 0 };

        const charRect = targetEl.getBoundingClientRect();
        const containerRect = sentenceWindowEl.getBoundingClientRect();
        const rawX = charRect.left - containerRect.left;
        const tooltipWidth = 420;
        const maxX = containerRect.width - tooltipWidth - 16;

        return {
            x: Math.max(8, Math.min(rawX, maxX)),
            y: charRect.bottom - containerRect.top + 6
        };
    }

    function openTooltipForSpan(span, event) {
        tooltipSpan = span;
        tooltipVisible = true;

        if (miniMode) {
            const charRect = event.currentTarget.getBoundingClientRect();
            showTooltipAt(charRect.left, charRect.bottom + 6, span);
        } else {
            const coords = calculateTooltipCoords(event.currentTarget);
            tooltipX = coords.x;
            tooltipY = coords.y;
        }
    }

    function closeHoverTooltip() {
        tooltipVisible = false;
        if (miniMode) hideTooltip();
    }

    function positionTooltipUnderChar(charEl) {
        const coords = calculateTooltipCoords(charEl);
        tooltipX = coords.x;
        tooltipY = coords.y;
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
            tooltipSpan = result;
            tooltipVisible = true;

            if (miniMode) {
                const rect = charEl.getBoundingClientRect();
                showTooltipAt(rect.left, rect.bottom + 6, result);
            } else {
                positionTooltipUnderChar(charEl);
            }
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
                    if (miniMode) hideTooltip();
                } else {
                    tooltipSpan = hoveredSpan;
                    tooltipVisible = true;

                    if (miniMode) {
                        const rect = lastHoverEl.getBoundingClientRect();
                        showTooltipAt(rect.left, rect.bottom + 6, hoveredSpan);
                    } else {
                        positionTooltipUnderChar(lastHoverEl);
                    }
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
                if (tooltipVisible && tooltipSpan === hoveredSpan) {
                    tooltipVisible = false;
                    if (miniMode) hideTooltip();
                } else {
                    openTooltipForSpan(hoveredSpan, event);
                }
            } else if (tooltipVisible && tooltipSpan === hoveredSpan) {
                tooltipVisible = false;
                if (miniMode) hideTooltip();
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

    $effect(() => {
        if (settings?.history_span != null && historyEntries.length > settings.history_span) {
            historyEntries = historyEntries.slice(0, settings.history_span);
        }
    });
</script>

<svelte:window 
    onclick={() => { tooltipVisible = false; if (miniMode) hideTooltip(); }} 
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

        {#if tooltipVisible && tooltipSpan && !miniMode}
            <LookupTooltip
                {tooltipSpan}
                {settings}
                {tooltipX}
                {tooltipY}
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