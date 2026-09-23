<script>
    import { isMostlyJapanese } from '$lib/japaneseDetect.js';
    import { lookupAtPosition, findHighlightedWordSpans } from '$lib/lookup.js';
    import { startClipboardListener, stopClipboardListener } from '$lib/clipboardListener.js';
    import { startWebsocketListener, stopWebsocketListener } from '$lib/websocketListener.js';
    import { logLookupEvent } from '$lib/lookupEvents.js';
    import { mineWord, getWordMineStatus, getKnownWordsMap, updateWordStatus } from '$lib/dictionary.js';
    import { onMount } from 'svelte';
    
    import LookupTooltip from '$lib/components/LookupTooltip.svelte';
    import Toast from './Toast.svelte';
    import StatusMenu from './StatusMenu.svelte';
    import NameModal from './NameModal.svelte';
    import { getNames, findNameAt, scanNameSpans, nameToEntry } from '$lib/names';

    import { STATUS_LEVELS } from '$lib/constants';

    let { settings, miniMode, session, mediaId, mediaTag, onMined, wordStatusVersion = 0, onStatusChanged, onNameSaved } = $props();

    let currentText = $state('');
    let historyEntries = $state([]);
    let historyIndex = $state(0);

    let displayedText = $derived(
        historyIndex === 0 ? currentText : (historyEntries[historyIndex - 1] ?? currentText)
    );
    let displayedChars = $derived([...displayedText]);
    let viewingHistory = $derived(historyIndex > 0);

    let hoveredSpan = $state(/** @type {any} */ (null));
    let tooltipSpan = $state(/** @type {any} */ (null));
    let tooltipVisible = $state(false);
    let tooltipX = $state(0);
    let tooltipY = $state(0);
    let tooltipMaxHeight = $state(300);
    let hotkeyHeld = $state(false);

    let cycleSkip = 0;
    let hoverRequestId = 0;
    let lastHoverEl = null;
    let sentenceWindowEl = $state(null);

    // entry.id -> 'new' | 'different' | 'same', for the currently open tooltip's entries
    let mineStatuses = $state({});
    let mineStatusRequestId = 0;

    let lookupsRemaining = $state(null); // null = not tracking (limit disabled or no session)
    let lastHourBucket = -1;

    
    let knownWordsMap = $state(new Map());
    let knownSpans = $state([]);
    let statusMenu = $state(null); // { x, y, wordId, current } | null

    // Custom per-media name dictionary.
    let names = $state(/** @type {Array<Record<string, any>>} */ ([]));
    let namesRequestId = 0;
    let nameSpans = $state(/** @type {Array<{ start: number, end: number, nameId: number }>} */ ([]));

    // Text-selection "Save name" popup.
    let selPopup = $state(/** @type {{ x: number, y: number } | null} */ (null)); // relative to the sentence window
    let selText = $state('');
    let suppressClick = false;
    let showNameModal = $state(false);

    $effect(() => {
      if (!session?.running || !settings?.lookup_limit_enabled) {
              lookupsRemaining = null;
              lastHourBucket = -1;
              return;
          }
      
          const hourBucket = Math.floor(session.seconds / 3600);
          if (hourBucket !== lastHourBucket) {
              lastHourBucket = hourBucket;
              lookupsRemaining = settings.lookup_limit_per_hour ?? 30;
          }
    })

    async function handleClipboardChange(text) {
        text = text.replace(/[\r\n]+/g, '');
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

        session?.recordSentence(text);
    }

    $effect(() => {
        if (session?.running) {
            if (settings?.input_mode === 'websocket') {
                startWebsocketListener(settings.websocket_address, handleClipboardChange);
            } else {
                startClipboardListener(handleClipboardChange);
            }
        } else {
            stopClipboardListener();
            stopWebsocketListener();
        }
    
        return () => {
            stopClipboardListener();
            stopWebsocketListener();
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

    async function refreshMineStatuses(span) {
        if (!span) return;

        const entries = [...span.entries, ...(settings?.show_related_entries ? span.related_entries : [])].filter(
            (entry) => !entry.customName
        );
        const requestId = ++mineStatusRequestId;

        const results = await Promise.all(
            entries.map((entry) =>
                getWordMineStatus({ dictId: entry.id, sentenceText: displayedText, mediaId }).then(
                    (status) => [entry.id, status]
                )
            )
        );

        if (requestId !== mineStatusRequestId) return;
        mineStatuses = Object.fromEntries(results);
    }

    function openTooltipAndLog(span, charEl) {
        if (lookupsRemaining !== null && lookupsRemaining <= 0) {
              return;
        }
      
        tooltipSpan = span;
        tooltipVisible = true;

        if (lookupsRemaining !== null) {
            lookupsRemaining -= 1;
        }

        // Custom name lookups are never logged: names have no lookup
        // counts and must not pollute the frequently-looked-up suggestions.
        if (!span.entries[0]?.customName) {
            logLookupEvent({
                mediaId,
                wordId: span.entries[0]?.id ?? null,
                surfaceText: span.surface,
                sessionId: session?.sessionId ?? null,
            });
        }

        positionTooltipUnderChar(charEl);
        refreshMineStatuses(span);
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

        hoveredSpan = withCustomName(result, index);
        if (!hoveredSpan) return;

        if (settings?.lookup_mode === 'hover') {
            openTooltipAndLog(hoveredSpan, charEl);
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
            hoveredSpan = withCustomName(result, anchorPos);
        } else {
            cycleSkip = 0;
            result = await lookupAtPosition(displayedText, anchorPos, 0);
            if (requestId !== hoverRequestId) return;
            hoveredSpan = withCustomName(result, anchorPos);
        }

        if (hoveredSpan && tooltipVisible) {
            tooltipSpan = hoveredSpan;
            refreshMineStatuses(hoveredSpan);
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
      // A drag-selection ending on a char fires click too — swallow it so
      // selecting text for "Save name" never opens a lookup tooltip, and
      // keep it from reaching the window handler so the popup stays open.
      if (suppressClick) {
          suppressClick = false;
          event.stopPropagation();
          return;
      }
      const knownSpan = getKnownSpanAt(index);
          if (knownSpan && knownSpan.status != null) {
              const rect = event.currentTarget.getBoundingClientRect();
              const clickedNearBottom = event.clientY - rect.top > rect.height - 6;
              if (clickedNearBottom) {
                  event.stopPropagation();
                  openStatusMenuFor(knownSpan, event);
                  return;
              }
          }
          
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

    let mineToastMessage = $state(null);
    let mineToastTimeoutId = null;
 
    function showMineToast(text) {
        mineToastMessage = text;
        clearTimeout(mineToastTimeoutId);
        mineToastTimeoutId = setTimeout(() => {
            mineToastMessage = null;
        }, 2200);
    }


    async function handleMineWord(span, entry, spelling) {
        if (!span || !entry) return;

        await mineWord({
            dictId: entry.id,
            spelling: spelling ?? entry.spellings[0] ?? span.surface,
            reading: entry.readings[0] ?? '',
            definitions: entry.definitions,
            wordType: entry.pos.join(', '),
            sentenceText: displayedText,
            highlightStart: span.start,
            highlightEnd: span.end,
            mediaId,
            sessionId: session?.sessionId ?? null,
        });

        mineStatuses = { ...mineStatuses, [entry.id]: 'same' };

        const label = spelling ?? entry.spellings[0] ?? span.surface;
        const reading = entry.readings[0];
        showMineToast(reading && reading !== label ? `Mined ${label} (${reading})` : `Mined ${label}`);

        onMined?.(entry.id, label);

        await loadKnownWords();
        await rescanKnownWords();
    }

    $effect(() => {
        if (settings?.history_span != null && historyEntries.length > settings.history_span) {
            historyEntries = historyEntries.slice(0, settings.history_span);
        }
    });

    async function loadKnownWords() {
        knownWordsMap = await getKnownWordsMap();
    }
    
    async function rescanKnownWords() {
        const mode = settings?.highlight_mode ?? 'none';
        if (mode === 'none' || !displayedText) {
            knownSpans = [];
            return;
        }
        knownSpans = await findHighlightedWordSpans(displayedText, knownWordsMap, mode, settings?.treat_new_as_unknown ?? false);
    }

    /** @param {number | null} id */
    async function loadNames(id) {
        const my = ++namesRequestId;
        if (id == null) {
            names = [];
            return;
        }
        const rows = await getNames({ mediaId: id });
        if (namesRequestId !== my) return;
        names = rows;
    }

    function rescanNames() {
        if (!displayedText || names.length === 0) {
            nameSpans = [];
            return;
        }
        // Names underline only while underlines are enabled at all
        // (highlight_mode 'none' disables everything) and the toggle is on.
        if ((settings?.highlight_mode ?? 'none') === 'none' || settings?.underline_names === false) {
            nameSpans = [];
            return;
        }
        nameSpans = scanNameSpans(displayedText, names);
    }

    /** @param {number} index */
    function getNameSpanAt(index) {
        return nameSpans.find((s) => index >= s.start && index < s.end) ?? null;
    }

    // Prepends the current media's saved name (if any covers `index`) as a
    // custom first lookup result, widening the span to the whole name.
    /** @param {Record<string, any> | null} span @param {number} index */
    function withCustomName(span, index) {
        if (names.length === 0) return span;
        const hit = findNameAt(names, displayedText, index);
        if (!hit) return span;
        const entry = nameToEntry(hit.row);
        const surface = displayedChars.slice(hit.start, hit.end).join('');
        if (!span) {
            return {
                start: hit.start,
                end: hit.end,
                surface,
                entries: [entry],
                deconjugated_from: null,
                related_entries: [],
            };
        }
        const widened = hit.start !== span.start || hit.end !== span.end;
        return {
            ...span,
            start: Math.min(span.start, hit.start),
            end: Math.max(span.end, hit.end),
            surface: widened ? surface : span.surface,
            entries: [entry, ...span.entries],
            deconjugated_from: widened ? null : span.deconjugated_from,
        };
    }

    /** @param {MouseEvent} event */
    function handleSentenceMouseUp(event) {
        const sel = window.getSelection();
        if (!sel || sel.isCollapsed || sel.rangeCount === 0) {
            selPopup = null;
            return;
        }
        const sentenceEl = /** @type {HTMLElement} */ (event.currentTarget);
        if (!sentenceEl.contains(sel.getRangeAt(0).commonAncestorContainer)) {
            selPopup = null;
            return;
        }
        const text = sel.toString().trim();
        if (!text) {
            selPopup = null;
            return;
        }
        const rect = sel.getRangeAt(0).getBoundingClientRect();
        const containerRect = sentenceWindowEl.getBoundingClientRect();
        selText = text;
        // Swallow the click that follows mouseup so selecting never opens a
        // lookup tooltip.
        suppressClick = true;
        selPopup = {
            x: Math.max(8, Math.min(rect.left - containerRect.left, containerRect.width - 200)),
            y: Math.max(8, rect.top - containerRect.top - 95),
        };
    }

    async function handleNameSaved() {
        window.getSelection()?.removeAllRanges();
        selText = '';
        await loadNames(mediaId);
        rescanNames();
        onNameSaved?.();
    }

    $effect(() => {
        loadNames(mediaId);
    });

    $effect(() => {
        displayedText;
        selPopup = null;
    });

    $effect(() => {
        displayedText;
        settings?.highlight_mode;
        settings?.treat_new_as_unknown;
        settings?.underline_names;
        names;
        rescanKnownWords();
        rescanNames();
    });
    
    onMount(() => {
        loadKnownWords();
    });

    // Reload known-word statuses when the page reports a status change
    // (e.g. via the mined-word cards' status bars).
    $effect(() => {
        wordStatusVersion;
        loadKnownWords();
    });
    
    function getKnownSpanAt(index) {
        return knownSpans.find((s) => index >= s.start && index < s.end) ?? null;
    }
    
    function openStatusMenuFor(span, event) {
        const rect = event.currentTarget.getBoundingClientRect();
        statusMenu = { x: rect.left, y: rect.bottom + 4, wordId: span.wordId, current: span.status };
    }
    
    async function handleStatusSelect(newStatus) {
        if (!statusMenu) return;
        await updateWordStatus({ wordId: statusMenu.wordId, status: newStatus });
        knownWordsMap.set(statusMenu.wordId, newStatus);
        knownSpans = knownSpans.map((s) =>
            s.wordId === statusMenu.wordId ? { ...s, status: newStatus } : s
        );
        statusMenu = null;
        onStatusChanged?.();
    }
    
    function closeStatusMenu() {
        statusMenu = null;
    }
</script>

<svelte:window
    onclick={() => {
        tooltipVisible = false;
        // A drag-selection's click lands on the sentence element itself and
        // bubbles here: don't let it dismiss the popup it just created.
        if (suppressClick) {
            suppressClick = false;
        } else {
            selPopup = null;
        }
    }}
    onkeydown={handleGlobalKeydown}
    onkeyup={handleGlobalKeyup}
/>

<div class="sentence-window" bind:this={sentenceWindowEl} onwheel={handleSentenceWheel}>
    {#if displayedChars.length > 0}
        <p
            class="sentence-text"
            class:history-text={viewingHistory}
            onmouseleave={handleSentenceLeave}
            onmousedown={() => (suppressClick = false)}
            onmouseup={handleSentenceMouseUp}
            style={`--font-size: ${settings?.font_size ?? 30}px; --font-family: '${settings?.font_family ?? 'Noto Sans JP'}'`}
        >
            {#each displayedChars as char, i}
                <span
                    class="char-token"
                    class:hovered={hoveredSpan && i >= hoveredSpan.start && i < hoveredSpan.end && settings?.word_highlight_enabled}
                    class:span-start={hoveredSpan && i === hoveredSpan.start && settings?.word_highlight_enabled}
                    class:span-end={hoveredSpan && i === hoveredSpan.end - 1 && settings?.word_highlight_enabled}
                    class:known-word={getKnownSpanAt(i) !== null}
                    class:name-word={getNameSpanAt(i) !== null}
                    class:no-match={hoveredSpan && i >= hoveredSpan.start && i < hoveredSpan.end && hoveredSpan.entries.length === 0 && settings?.word_highlight_enabled}
                    onmouseenter={(event) => handleCharHover(i, event)}
                    onclick={(event) => handleCharClick(i, event)}
                    style={getKnownSpanAt(i) ? `--status-color: ${getKnownSpanAt(i).status != null ? (STATUS_LEVELS[getKnownSpanAt(i).status]?.color ?? '') : '#f38ba8'}` : ''}
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
                {mineStatuses}
                onMine={(entry, spelling) => handleMineWord(tooltipSpan, entry, spelling)}
                {onStatusChanged}
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
    {#if lookupsRemaining !== null}
        <div class="lookup-limit-badge" class:depleted={lookupsRemaining <= 0}>
            {lookupsRemaining}
        </div>
    {/if}

    {#if selPopup}
        <div
            class="selection-popup"
            style={`left: ${selPopup.x}px; top: ${selPopup.y}px`}
        >
            <button
                type="button"
                class="selection-save-btn"
                onclick={(event) => {
                    event.stopPropagation();
                    selPopup = null;
                    suppressClick = false;
                    showNameModal = true;
                }}
            >
                Save name
            </button>
        </div>
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

<NameModal bind:show={showNameModal} {mediaId} name={selText} onSaved={handleNameSaved} />

<Toast message={mineToastMessage} />

<style>
    .sentence-window {
        position: relative;
        width: 100%;
        max-width: 900px;
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

    .lookup-limit-badge {
        position: absolute;
        bottom: 0.75rem;
        right: 0.9rem;
        min-width: 1.6rem;
        height: 1.6rem;
        border-radius: 999px;
        color: var(--theme-textSecondary, #b3b3b3);
        font-size: 0.75rem;
        font-weight: 700;
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 4;
        font-variant-numeric: tabular-nums;
        transition: color 0.2s ease, border-color 0.2s ease, background 0.2s ease;
    }
    
    .lookup-limit-badge.depleted {
        color: #f38ba8;
    }

    .char-token.known-word {
        border-bottom: 2px solid var(--status-color, transparent);
        padding-bottom: 1px;
    }

    /* Custom name dictionary matches underline cyan, winning over the
       known-word color when both apply. */
    .char-token.name-word {
        border-bottom: 2px solid #4FDCFF;
        padding-bottom: 1px;
    }

    .selection-popup {
        position: absolute;
        z-index: 30;
        min-width: 118px;
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 95%, #000);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 8px;
        padding: 0.12rem;
        box-shadow: 0 8px 24px rgba(0, 0, 0, 0.45);
        animation: popup-in 0.12s ease-out;
    }

    @keyframes popup-in {
        from {
            opacity: 0;
        }
    }

    .selection-save-btn {
        display: block;
        width: 100%;
        text-align: left;
        background: none;
        border: none;
        border-radius: 6px;
        font: inherit;
        font-size: 0.78rem;
        font-weight: 700;
        color: var(--theme-text, #f6f6f6);
        padding: 0.28rem 0.6rem;
        cursor: pointer;
        white-space: nowrap;
        transition: background 0.15s ease;
    }

    .selection-save-btn:hover {
        background: color-mix(in srgb, var(--theme-text, #f6f6f6) 8%, transparent);
    }
</style>