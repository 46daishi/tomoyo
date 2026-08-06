<script>
    import { onMount } from 'svelte';
    import { lookupAtPosition } from '$lib/lookup.js';
    import { logLookupEvent } from '$lib/lookupEvents.js';
    import { mineWord, getKnownWordsMap, updateWordStatus } from '$lib/dictionary.js';
    import { findKnownWordSpans } from '$lib/lookup.js';
    import { STATUS_LEVELS } from '$lib/constants.js';
    import LookupTooltip from '$lib/components/LookupTooltip.svelte';
    import StatusMenu from '$lib/components/StatusMenu.svelte';
    import { ICONS } from '$lib/icons';

    let { text, settings, mediaId = null } = $props();

    let chars = $derived([...text]);
    let containerEl = $state(null);
    let hoveredSpan = $state(null);
    let tooltipSpan = $state(null);
    let tooltipVisible = $state(false);
    let tooltipX = $state(0);
    let tooltipY = $state(0);
    let tooltipMaxHeight = $state(300);
    let hoverRequestId = 0;

    let knownWordsMap = $state(new Map());
    let knownSpans = $state([]);
    let statusMenu = $state(null); // { x, y, wordId, current } | null

    function isSameSpan(a, b) {
        return !!a && !!b && a.start === b.start && a.end === b.end && a.surface === b.surface;
    }

    function positionTooltip(charEl) {
        if (!charEl || !containerEl) return;
        const charRect = charEl.getBoundingClientRect();
        const containerRect = containerEl.getBoundingClientRect();

        const tooltipWidth = 420;
        const rawX = charRect.left - containerRect.left;
        const maxX = containerRect.width - tooltipWidth - 12;
        tooltipX = Math.max(8, Math.min(rawX, maxX));
        tooltipY = charRect.bottom - containerRect.top + 6;

        const availableHeight = window.innerHeight - charRect.bottom - 6 - 16;
        tooltipMaxHeight = Math.max(80, availableHeight);
    }

    function openTooltip(span, charEl) {
        tooltipSpan = span;
        tooltipVisible = true;
        logLookupEvent({ mediaId, wordId: span.entries[0]?.id ?? null, surfaceText: span.surface });
        positionTooltip(charEl);
    }

    async function handleCharHover(index, event) {
        const charEl = event.currentTarget;
        if (hoveredSpan && index >= hoveredSpan.start && index < hoveredSpan.end) return;

        const requestId = ++hoverRequestId;
        const result = await lookupAtPosition(text, index);
        if (requestId !== hoverRequestId) return;
        hoveredSpan = result;

        if (result && settings?.lookup_mode === 'hover') {
            openTooltip(result, charEl);
        }
    }

    function getKnownSpanAt(index) {
        return knownSpans.find((s) => index >= s.start && index < s.end) ?? null;
    }

    function openStatusMenuFor(span, event) {
        const rect = event.currentTarget.getBoundingClientRect();
        statusMenu = { x: rect.left, y: rect.bottom + 4, wordId: span.wordId, current: span.status };
    }

    function closeStatusMenu() {
        statusMenu = null;
    }

    async function handleStatusSelect(newStatus) {
        if (!statusMenu) return;
        await updateWordStatus({ wordId: statusMenu.wordId, status: newStatus });
        knownWordsMap.set(statusMenu.wordId, newStatus);
        knownSpans = knownSpans.map((s) =>
            s.wordId === statusMenu.wordId ? { ...s, status: newStatus } : s
        );
        statusMenu = null;
    }

    function handleCharClick(index, event) {
        const knownSpan = getKnownSpanAt(index);
        if (knownSpan) {
            const rect = event.currentTarget.getBoundingClientRect();
            const clickedNearBottom = event.clientY - rect.top > rect.height - 6;
            if (clickedNearBottom) {
                event.stopPropagation();
                openStatusMenuFor(knownSpan, event);
                return;
            }
        }

        if (hoveredSpan && index >= hoveredSpan.start && index < hoveredSpan.end) {
            if (tooltipVisible && isSameSpan(tooltipSpan, hoveredSpan)) {
                tooltipVisible = false;
            } else {
                openTooltip(hoveredSpan, event.currentTarget);
            }
            event.stopPropagation();
        }
    }

    function handleLeave() {
        hoveredSpan = null;
    }

    async function handleMine(entry) {
        if (!tooltipSpan || !entry) return;
        await mineWord({
            dictId: entry.id,
            spelling: entry.spellings[0] ?? tooltipSpan.surface,
            reading: entry.readings[0] ?? '',
            definitions: entry.definitions,
            wordType: entry.pos.join(', '),
            mediaId,
            sentenceText: text,
            highlightStart: tooltipSpan.start,
            highlightEnd: tooltipSpan.end,
        });

        await loadKnownWords();
        await rescanKnownWords();
    }

    async function loadKnownWords() {
        knownWordsMap = await getKnownWordsMap();
    }

    async function rescanKnownWords() {
        if (!settings?.highlight_known_words || !text) {
            knownSpans = [];
            return;
        }
        knownSpans = await findKnownWordSpans(text, knownWordsMap);
    }

    $effect(() => {
        text;
        settings?.highlight_known_words;
        rescanKnownWords();
    });

    onMount(() => {
        loadKnownWords();
    });
</script>

<svelte:window onclick={() => (tooltipVisible = false)} />

<div class="interactive-sentence" bind:this={containerEl} onmouseleave={handleLeave}>
    {#each chars as char, i}
        <span
            class="char-token"
            class:hovered={hoveredSpan && i >= hoveredSpan.start && i < hoveredSpan.end}
            class:no-match={hoveredSpan && i >= hoveredSpan.start && i < hoveredSpan.end && hoveredSpan.entries.length === 0}
            class:known-word={getKnownSpanAt(i) !== null}
            style={getKnownSpanAt(i) ? `--status-color: ${STATUS_LEVELS[getKnownSpanAt(i).status]?.color ?? ''}` : ''}
            onmouseenter={(e) => handleCharHover(i, e)}
            onclick={(e) => handleCharClick(i, e)}
        >{char}</span>
    {/each}

    {#if tooltipVisible && tooltipSpan}
        <LookupTooltip
            {tooltipSpan}
            {settings}
            {tooltipX}
            {tooltipY}
            {tooltipMaxHeight}
            onMine={handleMine}
            onMouseLeave={() => {}}
        />
    {/if}

    {#if statusMenu}
        <StatusMenu
            x={statusMenu.x}
            y={statusMenu.y}
            levels={STATUS_LEVELS}
            current={statusMenu.current}
            onSelect={handleStatusSelect}
            onClose={closeStatusMenu}
        />
    {/if}
</div>

<style>
    .interactive-sentence {
        position: relative;
        font-family: "Noto Sans JP", Inter, sans-serif;
        font-size: 2.5rem;
        font-weight: 700;
        line-height: 1.6;
        color: var(--theme-text, #f6f6f6);
        text-align: center;
        max-width: 1000px;
    }

    .char-token {
        cursor: pointer;
        transition: background 0.1s ease;
    }

    .char-token.hovered {
        background: color-mix(in srgb, var(--theme-primary, #36b7bd) 25%, transparent);
        border-radius: 2px;
    }

    .char-token.hovered.no-match {
        background: color-mix(in srgb, var(--theme-textSecondary, #b3b3b3) 20%, transparent);
        cursor: default;
    }

    .char-token.known-word {
        border-bottom: 2px solid var(--status-color, transparent);
        padding-bottom: 1px;
    }
</style>