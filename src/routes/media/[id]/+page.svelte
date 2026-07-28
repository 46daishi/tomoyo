<script>
    import { page } from '$app/state';
    import { getDb } from '$lib/db';
    import ActionButton from '$lib/components/ActionButton.svelte';
    import { ICONS } from '$lib/icons';
    import { coverSrc } from '$lib/db';
    import { fly } from 'svelte/transition';
    import { STATUS_COLORS } from '$lib/constants.js';
    import { isMostlyJapanese } from '$lib/japaneseDetect.js';
    import { lookupAtPosition } from '$lib/lookup.js';
    import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
    import { getCurrentWindow, LogicalPosition } from '@tauri-apps/api/window';
    import { emit } from '@tauri-apps/api/event';
    import { onMount } from 'svelte';
    import { setMediaTitle } from '$lib/stores/presence.svelte.js';
    import MediaFormModal from '$lib/components/MediaFormModal.svelte';
    import { loadSettings } from '$lib/settings.js';

    let showEditModal = $state(false);

    let mediaId = $derived(Number(page.params.id));
    let media = $state(null);

    async function loadMedia(id) {
        const db = await getDb();
        const rows = await db.select('SELECT * FROM media WHERE id = $1', [id]);
        media = rows[0] ?? null;

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

    // ── History ──
    // historyEntries[0] is the most recent past sentence, historyEntries[1]
    // the one before that, etc. historyIndex === 0 means "showing the
    // current sentence"; historyIndex === n means "n sentences back".
    let historyEntries = $state([]);
    let historyIndex = $state(0);

    let displayedText = $derived(
        historyIndex === 0 ? currentText : (historyEntries[historyIndex - 1] ?? currentText)
    );
    let displayedChars = $derived([...displayedText]);
    let viewingHistory = $derived(historyIndex > 0);

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

        // Push the sentence that was current a moment ago into history,
        // before it gets overwritten — this is what "scrolling up 1
        // sentence at a time" walks back through.
        if (settings?.history_enabled && currentText) {
            const span = settings.history_span ?? 50;
            historyEntries = [currentText, ...historyEntries].slice(0, span);
        }

        currentText = text;
        historyIndex = 0; // a new clipboard sentence always returns to "current"
        hoveredSpan = null;
        tooltipSpan = null;
        tooltipVisible = false;
        cycleSkip = 0;
    }

    async function handleCharHover(index) {
        if (hoveredSpan && index >= hoveredSpan.start && index < hoveredSpan.end) {
            return;
        }

        cycleSkip = 0;
        const requestId = ++hoverRequestId;
        const result = await lookupAtPosition(displayedText, index);
        if (requestId !== hoverRequestId) return;

        hoveredSpan = result;
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
    }

    function handleWindowKeydown(event) {
        if (!settings?.cycle_key) return;
        if (event.code === settings.cycle_key && !event.repeat) {
            event.preventDefault();
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
        const mainPos = await mainWindow.outerPosition();
        const scale = await mainWindow.scaleFactor();

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

    let miniMode = $state(false);
    let resizeDebounceHandle = null;

    function applyMiniModeClasses(active) {
        document.documentElement.classList.toggle('mini-mode', active);
        document.body.classList.toggle('mini-mode', active);
    }

    function handleWindowResize() {
        clearTimeout(resizeDebounceHandle);
        resizeDebounceHandle = setTimeout(checkWindowSize, 50);
    }

    let settings = $state(null);
    let fontSize;
    let fontFamily;

    function checkWindowSize() {
        const h = window.innerHeight;

        if (!miniMode && h <= settings.mini_mode_enter_height) {
            miniMode = true;
            applyMiniModeClasses(true);
            hideTooltip();
            tooltipVisible = false;
        } else if (miniMode && h >= settings.mini_mode_exit_height) {
            miniMode = false;
            applyMiniModeClasses(false);
        }
    }

    function applyMiniModeTransparency(transparency) {
        const colorWeight = Math.round((1 - transparency) * 100);
        document.documentElement.style.setProperty('--mini-color-weight', `${colorWeight}%`);
    }

    // Keep stored history within whatever span the user currently has set,
    // even if they shrink it live from the settings page mid-session.
    $effect(() => {
        if (settings?.history_span != null && historyEntries.length > settings.history_span) {
            historyEntries = historyEntries.slice(0, settings.history_span);
        }
    });

    onMount(async () => {
        settings = await loadSettings();
        fontSize = settings.font_size;
        fontFamily = settings.font_family;
        applyMiniModeTransparency(settings.mini_mode_transparency);

        checkWindowSize();
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
                    const tooltipWidth = 420;
                    const maxX = containerRect.width - tooltipWidth - 16;

                    tooltipX = Math.max(8, Math.min(rawX, maxX));
                    tooltipY = charRect.bottom - containerRect.top + 6;
                }
            }
            event.stopPropagation();
        }
    }

    function handleWindowClick() {
        tooltipVisible = false;
        if (miniMode) hideTooltip();
    }

    function handleSentenceLeave() {
        hoveredSpan = null;
        cycleSkip = 0;
    }

    function isInHoveredSpan(index) {
        return hoveredSpan !== null && index >= hoveredSpan.start && index < hoveredSpan.end;
    }

    function isSpanStart(index) {
        return isInHoveredSpan(index) && index === hoveredSpan.start;
    }

    function isSpanEnd(index) {
        return isInHoveredSpan(index) && index === hoveredSpan.end - 1;
    }

    // Scrolling inside the sentence window navigates history instead of
    // scrolling the page: up goes further back (bounded by history_span),
    // down comes back toward the current sentence.
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

        <div class="sentence-window" bind:this={sentenceWindowEl} onwheel={handleSentenceWheel}>
            {#if displayedChars.length > 0}
                <p
                    class="sentence-text"
                    class:history-text={viewingHistory}
                    onmouseleave={handleSentenceLeave}
                    style={`--font-size: ${fontSize}px; --font-family: '${fontFamily}'`}
                >
                    {#each displayedChars as char, i}
                        <span
                            class="char-token"
                            class:hovered={isInHoveredSpan(i) && settings?.word_highlight_enabled}
                            class:span-start={isSpanStart(i) && settings?.word_highlight_enabled}
                            class:span-end={isSpanEnd(i) && settings?.word_highlight_enabled}
                            class:no-match={isInHoveredSpan(i) && hoveredSpan.entries.length === 0 && settings?.word_highlight_enabled}
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
                            onwheel={(event) => event.stopPropagation()}
                    >
                        <div class="tooltip-surface">
                            {tooltipSpan.surface}
                            {#if tooltipSpan.deconjugated_from}
                                <span class="tooltip-deconj">({tooltipSpan.deconjugated_from})</span>
                            {/if}
                        </div>
                        {#if tooltipSpan.entries.length > 0 || (tooltipSpan.related_entries.length > 0 && settings.show_related_entries)}
                            <ul class="tooltip-entries">
                                {#each [...tooltipSpan.entries, ...(settings.show_related_entries ? tooltipSpan.related_entries : [])] as entry}
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
    <MediaFormModal
        bind:show={showEditModal}
        media={media}
        onSaved={() => loadMedia(mediaId)}
    />
</main>

<div class="logo">
    <a href="https://x.com/46daishi" target="_blank" rel="noopener noreferrer"><img src="/tomoyo_full.png" alt="tomoyo" /></a>
</div>
<nav class="side-nav" aria-label="App navigation">
  <div class="nav-actions">
      <ActionButton icon={ICONS.back} variant="primary" size="small" onAction={() => history.back()} />
      <ActionButton icon={ICONS.edit} variant="secondary" size="small" onAction={() => (showEditModal = true)} />
      <ActionButton icon={ICONS.stats} variant="secondary" size="small" />
      <ActionButton icon={ICONS.book} variant="secondary" size="small" />
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
        padding-bottom: 2rem;
        max-height: 100vh;
        overflow-y: auto;
    }

    .session-timer {
        font-size: 0.75rem;
        font-weight: 600;
        color: var(--theme-textSecondary, #b3b3b3);
        font-variant-numeric: tabular-nums;
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
        margin-top: 1rem;
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

    /* History sentences render bright yellow, unconditionally, regardless
       of theme — this is a deliberate fixed color, not a --theme-* variable,
       since it needs to read as "you're looking at the past" the same way
       no matter which of the app's themes is active. Higher specificity
       than .sentence-text alone guarantees it wins regardless of rule order. */
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

    .mini-toggle-wrapper {
        position: absolute;
        bottom: 1rem;
        right: 1rem;
        z-index: 5;
    }

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
            color-mix(in srgb, var(--theme-surface, #1e1e2e) 80%, black 20%) var(--mini-color-weight, 70%),
            transparent
        );
    }
</style>