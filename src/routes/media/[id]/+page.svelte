<script>
    import { page } from '$app/state';
    import { getDb } from '$lib/db';
    import ActionButton from '$lib/components/ActionButton.svelte';
    import { ICONS } from '$lib/icons';
    import { coverSrc } from '$lib/db';
    import { fly } from 'svelte/transition';
    import { STATUS_COLORS } from '$lib/constants.js';
    import { isMostlyJapanese } from '$lib/japaneseDetect.js';
    import { tokenizeSentence } from '$lib/tokenize.js';
    import { lookupAtPosition } from '$lib/lookup.js';
    import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
    import { getCurrentWindow, LogicalPosition, LogicalSize } from '@tauri-apps/api/window';
    import { emit } from '@tauri-apps/api/event';
    import { onMount } from 'svelte';
    import { setMediaTitle } from '$lib/stores/presence.svelte.js';

    let mediaId = $derived(Number(page.params.id));
    let media = $state(null);

    
    
    async function loadMedia(id) {
        const db = await getDb();
        const rows = await db.select('SELECT * FROM media WHERE id = $1', [id]);
        media = rows[0] ?? null;
        
        // Update presence title when media is fetched
        if (media?.title) {
            setMediaTitle(media.title);
        }
    }

    $effect(() => {
        loadMedia(mediaId);
    });

    let sessionRunning = $state(false);
    let sessionSeconds = $state(0);
    let timerHandle = null;
    
    function formatTime(totalSeconds) {
        const h = Math.floor(totalSeconds / 3600);
        const m = Math.floor((totalSeconds % 3600) / 60);
        const s = totalSeconds % 60;
        const pad = (n) => String(n).padStart(2, '0');
        return `${pad(h)}:${pad(m)}:${pad(s)}`;
    }
    
    function toggleSession() {
        if (sessionRunning) {
            clearInterval(timerHandle);
            timerHandle = null;
            sessionRunning = false;
            sessionSeconds = 0;
        } else {
            sessionRunning = true;
            timerHandle = setInterval(() => {
                sessionSeconds += 1;
            }, 1000);
        }
    }
    
    $effect(() => {
        return () => {
            if (timerHandle) clearInterval(timerHandle);
        };
    });

    import { startClipboardListener, stopClipboardListener } from '$lib/clipboardListener.js';

    // The raw sentence text only — nothing is pre-scanned or
    // pre-highlighted. Each character is its own hover/click target, and
    // a lookup only ever runs for the exact character the user is
    // pointing at, mirroring JL's click/cursor-driven model.
    let currentText = $state('');
    let currentChars = $derived([...currentText]);

    // The most recently resolved MatchSpan `{ start, end, surface, entries,
    // deconjugated_from }`, or null if nothing is currently hovered — this
    // only drives the highlight, and clears the instant the pointer leaves
    // the text.
    let hoveredSpan = $state(null);

    // The span the tooltip is showing, captured at the moment it's opened.
    // Deliberately separate from hoveredSpan so the tooltip keeps showing
    // its word's info even after the mouse moves off the text — only an
    // explicit click (see handleWindowClick) closes it.
    let tooltipSpan = $state(null);
    let tooltipVisible = $state(false);

    // How many longest-first candidates to skip at the currently hovered
    // position (see lookupAtPosition's `skip` param). Reset to 0 whenever
    // hover moves to a genuinely new position — cycling is relative to
    // whatever word is currently under the pointer.
    let cycleSkip = 0;

    // Guards against out-of-order results if lookups overlap while the
    // mouse moves quickly across characters.
    let hoverRequestId = 0;

    async function handleClipboardChange(text) {
        if (!isMostlyJapanese(text)) return;
        currentText = text;
        hoveredSpan = null;
        tooltipSpan = null;
        tooltipVisible = false;
        cycleSkip = 0;
    }

    async function handleCharHover(index) {
        // Already covered by the currently resolved span — no need to
        // re-run the lookup for every character inside the same word.
        if (hoveredSpan && index >= hoveredSpan.start && index < hoveredSpan.end) {
            return;
        }

        cycleSkip = 0; // fresh position — start from the longest match again
        const requestId = ++hoverRequestId;
        const result = await lookupAtPosition(currentText, index);
        if (requestId !== hoverRequestId) return; // a newer hover superseded this one

        hoveredSpan = result;
        // Note: deliberately not touching tooltipVisible/tooltipSpan here —
        // moving the hover around shouldn't affect an already-open tooltip.
    }

    // Shift cycles the currently hovered position through progressively
    // shorter dictionary matches (今日は → 今日 → 今, いい天気 → いい,
    // etc.) — the same escape hatch JL/Yomitan give you for a shorter word
    // a longer match swallows. Wraps back to the longest match once it
    // runs out of shorter candidates.
    async function handleCycleShorter() {
        if (!hoveredSpan) return;

        const anchorPos = hoveredSpan.start;
        const nextSkip = cycleSkip + 1;
        const requestId = ++hoverRequestId;
        let result = await lookupAtPosition(currentText, anchorPos, nextSkip);
        if (requestId !== hoverRequestId) return;

        if (result) {
            cycleSkip = nextSkip;
            hoveredSpan = result;
        } else {
            // Ran out of shorter candidates — wrap back to the longest.
            cycleSkip = 0;
            result = await lookupAtPosition(currentText, anchorPos, 0);
            if (requestId !== hoverRequestId) return;
            hoveredSpan = result;
        }
    }

    function handleWindowKeydown(event) {
        if (event.key === 'Shift' && !event.repeat) {
            handleCycleShorter();
        }
    }

    let tooltipWindow = null;
    
    function getTooltipWindow() {
        if (!tooltipWindow) {
            tooltipWindow = WebviewWindow.getByLabel('tooltip');
        }
        return tooltipWindow;
    }
    
    async function showTooltipAt(clientX, clientY, spanData) {
        const tooltip = await getTooltipWindow();
        const mainWindow = getCurrentWindow();
        const mainPos = await mainWindow.outerPosition(); // physical px, top-left of the main window on screen
        const scale = await mainWindow.scaleFactor();
    
        // clientX/clientY are CSS pixels relative to the webview's own viewport;
        // convert to physical screen pixels and add the main window's own
        // screen offset to get an absolute screen position for the tooltip window.
        const screenX = mainPos.x + clientX * scale;
        const screenY = mainPos.y + clientY * scale;
    
        await tooltip.setPosition(new LogicalPosition(screenX / scale, screenY / scale));
        await emit('tooltip-content', spanData);
        await tooltip.show();
    }
    
    async function hideTooltip() {
        const tooltip = await getTooltipWindow();
        await tooltip.hide();
    }

    const MINI_ENTER_WIDTH = 550;
    const MINI_ENTER_HEIGHT = 400;
    const MINI_EXIT_WIDTH = 620;   // slightly larger than enter, to avoid flicker at the boundary
    const MINI_EXIT_HEIGHT = 350;
    
    let miniMode = $state(false);
    let resizeDebounceHandle = null;
    
    function applyMiniModeClasses(active) {
        document.documentElement.classList.toggle('mini-mode', active);
        document.body.classList.toggle('mini-mode', active);
    }
    
    function checkWindowSize() {
        const w = window.innerWidth;
        const h = window.innerHeight;
    
        if (!miniMode && (w <= MINI_ENTER_WIDTH || h <= MINI_ENTER_HEIGHT)) {
            miniMode = true;
            applyMiniModeClasses(true);
            hideTooltip();
            tooltipVisible = false;
        } else if (miniMode && w >= MINI_EXIT_WIDTH && h >= MINI_EXIT_HEIGHT) {
            miniMode = false;
            applyMiniModeClasses(false);
        }
    }
    
    function handleWindowResize() {
        // Resize fires continuously while dragging — debounce so the
        // threshold check (and its class toggling) doesn't run dozens of
        // times a second mid-drag.
        clearTimeout(resizeDebounceHandle);
        resizeDebounceHandle = setTimeout(checkWindowSize, 50);
    }
    
    onMount(() => {
        checkWindowSize(); // handle the case where the window is already small on mount
    
        window.addEventListener('resize', handleWindowResize);
        return () => {
            window.removeEventListener('resize', handleWindowResize);
            clearTimeout(resizeDebounceHandle);
        };
    });

    let tooltipX = $state(0);
    let tooltipY = $state(0);
    let sentenceWindowEl;

    function handleCharClick(index, event) {
        if (hoveredSpan && index >= hoveredSpan.start && index < hoveredSpan.end) {
            if (tooltipVisible && tooltipSpan === hoveredSpan) {
                tooltipVisible = false;
                if (miniMode) hideTooltip();
            } else {
                tooltipSpan = hoveredSpan;
                tooltipVisible = true;
    
                const charRect = event.currentTarget.getBoundingClientRect();
    
                if (miniMode) {
                    showTooltipAt(charRect.left, charRect.bottom + 6, hoveredSpan);
                } else {
                    const containerRect = sentenceWindowEl.getBoundingClientRect();
                    const rawX = charRect.left - containerRect.left;
                    const tooltipWidth = 420; // matches .lookup-tooltip's max-width
                    const maxX = containerRect.width - tooltipWidth - 16;
    
                    tooltipX = Math.max(8, Math.min(rawX, maxX));
                    tooltipY = charRect.bottom - containerRect.top + 6;
                }
            }
            event.stopPropagation();
        }
    }

    // Any click that isn't the one above (i.e. it wasn't stopped) reaches
    // here and closes an open tooltip — clicking elsewhere, including a
    // different word, dismisses it.
    function handleWindowClick() {
        tooltipVisible = false;
        if (miniMode) hideTooltip();
    }

    // mouseleave (unlike mouseout) only fires when the pointer truly
    // leaves the whole element, not when moving between child <span>s, so
    // this only clears the highlight when the cursor leaves the sentence
    // entirely. The tooltip is intentionally untouched here.
    function handleSentenceLeave() {
        hoveredSpan = null;
        cycleSkip = 0;
    }

    function isInHoveredSpan(index) {
        return hoveredSpan !== null && index >= hoveredSpan.start && index < hoveredSpan.end;
    }

    // Only the first/last character of a multi-char match get rounded
    // corners, so the highlight reads as one continuous shape instead of
    // a rounded pill per character.
    function isSpanStart(index) {
        return isInHoveredSpan(index) && index === hoveredSpan.start;
    }

    function isSpanEnd(index) {
        return isInHoveredSpan(index) && index === hoveredSpan.end - 1;
    }
    
    $effect(() => {
        if (sessionRunning) {
            startClipboardListener(handleClipboardChange);
        } else {
            stopClipboardListener();
        }
    
        return () => {
            stopClipboardListener();
        };
    });
</script>

<svelte:window onclick={handleWindowClick} onkeydown={handleWindowKeydown} />

<main class="page home">
    {#if media}
        <div class="media-header">
            <div class="cover">
                {#if media.cover_path}
                    <img src={coverSrc(media.cover_path)} alt={media.title} />
                {:else}
                    <div class="cover-placeholder"></div>
                {/if}
            </div>

            <div class="media-info">
                <div class="title-row">
                    <h1 class="media-title">{media.title}</h1>
                    {#if media.tag}
                        <span class="tag-pill" style="--tag-color: {media.color}">#{media.tag}</span>
                    {/if}
                </div>
            
                <div class="media-meta">
                    <span class="status">
                        <span class="status-dot" style="--dot-color: {STATUS_COLORS[media.status]}"></span>
                        {media.status}
                    </span>
                </div>
            </div>
        </div>

        <div class="sentence-window" bind:this={sentenceWindowEl}>
            {#if currentChars.length > 0}
                <p class="sentence-text" onmouseleave={handleSentenceLeave}>
                    {#each currentChars as char, i}
                        <span
                            class="char-token"
                            class:hovered={isInHoveredSpan(i)}
                            class:span-start={isSpanStart(i)}
                            class:span-end={isSpanEnd(i)}
                            class:no-match={isInHoveredSpan(i) && hoveredSpan.entries.length === 0}
                            onmouseenter={() => handleCharHover(i)}
                            onclick={(event) => handleCharClick(i, event)}
                        >{char}</span>
                    {/each}
                </p>

                {#if tooltipVisible && tooltipSpan && !miniMode}
                    <div
                            class="lookup-tooltip"
                            style="left: {tooltipX}px; top: {tooltipY}px;"
                            transition:fly={{ y: 6, duration: 120 }}
                            onclick={(event) => event.stopPropagation()}
                    >
                        <div class="tooltip-surface">
                            {tooltipSpan.surface}
                            {#if tooltipSpan.deconjugated_from}
                                <span class="tooltip-deconj">({tooltipSpan.deconjugated_from})</span>
                            {/if}
                        </div>
                        {#if tooltipSpan.entries.length > 0 || tooltipSpan.related_entries.length > 0}
                            <ul class="tooltip-entries">
                                {#each [...tooltipSpan.entries, ...tooltipSpan.related_entries] as entry}
                                    <li>
                                        <span class="entry-readings">
                                            {entry.spellings[0] ?? entry.readings[0]}
                                            {#if entry.readings[0] && entry.spellings.length > 0}
                                                <span class="entry-reading-kana">{entry.readings[0]}</span>
                                            {/if}
                                        </span>
                                        <div class="entry-pos">{entry.pos.join(', ')}</div>
                                        <div class="entry-definitions">{entry.definitions.join('; ')}</div>
                                    </li>
                                {/each}
                            </ul>
                        {:else}
                            <div class="tooltip-no-match">No dictionary entry found.</div>
                        {/if}
                    </div>
                {/if}
            {:else}
                <p class="sentence-placeholder">Waiting for a sentence…</p>
            {/if}

        </div>
    {:else}
        <p>Loading…</p>
    {/if}
</main>

<div class="logo">
    <a href="https://x.com/46daishi" target="_blank" rel="noopener noreferrer"><img src="/tomoyo_full.png" alt="tomoyo" /></a>
</div>
<nav class="side-nav" aria-label="App navigation">
  <div class="nav-actions">
      <ActionButton
          icon={ICONS.back}
          variant="primary"
          size="small"
          onAction={() => history.back()}
      />
      <ActionButton
          icon={ICONS.edit}
          variant="secondary"
          size="small"
      />
      <ActionButton
          icon={ICONS.stats}
          variant="secondary"
          size="small"
      />
      <ActionButton
          icon={ICONS.book}
          variant="secondary"
          size="small"
      />
      <ActionButton
                icon={sessionRunning ? ICONS.pause : ICONS.play}
                variant="primary"
                size="small"
                onAction={toggleSession}
      />
      {#if sessionRunning}
          <span class="session-timer" transition:fly={{ y: -8, duration: 200 }}>
              {formatTime(sessionSeconds)}
          </span>
      {/if}
  </div>
</nav>
<style>
    .page.home {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 1rem;
        box-sizing: border-box;
        width: 100%;
        padding-top: 2rem;
        padding-right: calc(1rem + 48px + 1.5rem);
        padding-left: calc(1rem + 48px + 1.5rem);
        padding-bottom: 2rem; /* breathing room at the bottom of the scroll */
        max-height: 100vh;
        overflow-y: auto;
    }

    .session-timer {
        font-size: 0.75rem;
        font-weight: 600;
        color: var(--theme-textSecondary, #b3b3b3);
        font-variant-numeric: tabular-nums; /* keeps digit widths consistent so it doesn't jitter as numbers change */
        text-align: center;
    }
    
    .title-row {
        display: flex;
        align-items: center;
        gap: 0.7rem;
        flex-wrap: wrap;
    }

    .media-header {
        display: flex;
        gap: 1.5rem;
        align-items: flex-start;
        width: 100%;
        max-width: 800px;
        margin-top:1rem;
    }
    
    .cover {
        flex-shrink: 0;
        aspect-ratio: 2 / 3;
        width: 130px;
        border-radius: 10px;
        overflow: hidden;
        background: var(--surface1, #313244);
    }
    
    .cover img {
        width: 100%;
        height: 100%;
        object-fit: cover;
        display: block;
    }
    
    .cover-placeholder {
        width: 100%;
        height: 100%;
        background: linear-gradient(135deg, var(--surface1, #313244), var(--surface0, #1e1e2e));
    }
    
    .media-info {
        display: flex;
        flex-direction: column;
        align-items: flex-start;
        text-align: left;
        gap: 0.3rem;
        padding-top: 0.4rem;
    }
    
    .media-title {
        font-size: 1.6rem;
        font-weight: 700;
        margin: 0;
    }
    
    .media-meta {
        display: flex;
        align-items: center;
        gap: 0.6rem;
        flex-wrap: wrap;
    }
    
    .status {
        display: flex;
        align-items: center;
        gap: 0.4rem;
        font-size: 0.85rem;
        color: var(--theme-textSecondary, #b3b3b3);
        text-transform: capitalize;
    }
    
    .status-dot {
        width: 8px;
        height: 8px;
        border-radius: 50%;
        background: var(--dot-color, var(--theme-textSecondary, #b3b3b3));
        flex-shrink: 0;
    }
    
    .tag-pill {
        font-size: 0.8rem;
        font-weight: 600;
        padding: 0.01em 0.7em;
        border-radius: 100px;
        color: var(--tag-color, #89b4fa);
        background: color-mix(in srgb, var(--tag-color, #89b4fa) 18%, transparent);
        border: 1px solid color-mix(in srgb, var(--tag-color, #89b4fa) 40%, transparent);
    }
    
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
        font-family: "Noto Sans JP", Inter, sans-serif;
        color: var(--theme-text, #f6f6f6);
        font-size: 1.9rem;
        font-weight: 700;
        line-height: 1.6;
        text-align: left;
        margin: 0;
        width: 100%;
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

    .mini-toggle-wrapper {
        position: absolute;
        bottom: 1rem;
        right: 1rem;
        z-index: 5;
    }

    :global(body.mini-mode) .side-nav,
    :global(body.mini-mode) .logo,
    :global(body.mini-mode) .side-nav,
    :global(body.mini-mode) .logo {
        opacity: 0 !important;
        pointer-events: none !important;
    }
    
    :global(body.mini-mode) .media-header {
        display: none !important;
    }
    :global(body.mini-mode) .page.home {
        padding: 0;
        max-height: 100vh;
        overflow: hidden;
        gap: 0;
    }
    
    :global(body.mini-mode) .page.home {
        background: transparent !important;
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
            color-mix(in srgb, var(--theme-surface, #1e1e2e) 80%, black 20%) 70%,
            transparent
        );
    }

</style>