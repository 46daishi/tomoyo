<script>
    import { page } from '$app/state';
    import { onMount } from 'svelte';
    import { fly } from 'svelte/transition'; // Added missing transition import
    import { getDb, coverSrc } from '$lib/db';
    import { setMediaTitle } from '$lib/stores/presence.svelte.js';
    import { loadSettings } from '$lib/settings.js';
    import { goto } from '$app/navigation';
    import { extractDominantColor } from '$lib/utils/color.js';

    import { initMiniMode } from '$lib/miniMode.js';
    import { createSessionStore } from '$lib/stores/session.svelte.js';

    import { ICONS } from '$lib/icons';

    import MediaHeader from '$lib/components/MediaHeader.svelte';
    import SideNav from '$lib/components/SideNav.svelte';
    import ActionButton from '$lib/components/ActionButton.svelte';
    import SentenceViewer from '$lib/components/SentenceWindow.svelte';
    import MediaFormModal from '$lib/components/MediaFormModal.svelte';
    import MediaStats from '$lib/components/MediaStats.svelte';
    import StatusMenu from '$lib/components/StatusMenu.svelte';

    import { STATUS_LEVELS } from '$lib/constants';
    import { parseDefinitions, updateWordStatus } from '$lib/dictionary.js';

    let settings = $state(null);
    let showEditModal = $state(false);
    let miniMode = $state(false);

    let mediaId = $derived(Number(page.params.id));
    let media = $state(/** @type {Record<string, any> | null} */ (null));
    let mediaCoverSrc = $derived(media?.cover_path ? coverSrc(media.cover_path) : null);

    let mediaRequestId = 0;

    // Initialize session store
    const session = createSessionStore(mediaId);

    async function loadMedia(id) {
        const my = ++mediaRequestId;
        const db = await getDb();
        const rows = await db.select('SELECT * FROM media WHERE id = $1', [id]);
        if (mediaRequestId !== my) return;
        media = rows[0] ?? null;

        if (media?.title) {
            setMediaTitle(media.title);
        }
    }

    $effect(() => {
        loadMedia(mediaId);
    });

    onMount(async () => {
        settings = await loadSettings();
        const miniModeManager = initMiniMode(settings, (mode) => (miniMode = mode));

        return () => {
            setMediaTitle(null);
            session.destroy();
            miniModeManager.destroy();
        };
    });

    let pageGlow = $state(null);
    let glowRequestId = 0;
    let glowColor = $derived(pageGlow ?? media?.color ?? '#dc143c');
    
    $effect(() => {
        const path = media?.cover_path;
        const requestId = ++glowRequestId;
    
        if (!path) {
            pageGlow = null;
            return;
        }
    
        extractDominantColor(coverSrc(path)).then((color) => {
            if (requestId === glowRequestId) {
                pageGlow = color;
            }
        });
    });

    // Words mined during the current session
    let sessionWords = $state(/** @type {Array<Record<string, any>>} */ ([]));
    let sessionWordsLoaded = $state(false);
    let sessionWordsExpanded = $state(false);
    let minedVersion = $state(0);
    let wordStatusVersion = $state(0);
    let sessionWordsRequestId = 0;

    let statusMenuWordId = $state(null);
    let statusMenuPos = $state({ x: 0, y: 0 });

    async function loadSessionWords(sid) {
        const my = ++sessionWordsRequestId;
        const db = await getDb();
        const rows = await db.select(
            `SELECT w.id, w.spelling, w.reading, w.definitions, w.status,
                    MIN(ws.created_at) as mined_at
             FROM word_sentences ws
             JOIN words w ON w.id = ws.word_id
             WHERE ws.session_id = $1
             GROUP BY w.id
             ORDER BY mined_at DESC`,
            [sid]
        );
        if (sessionWordsRequestId !== my) return;
        sessionWords = rows.map((r) => ({ ...r, definitions: parseDefinitions(r.definitions) }));
        sessionWordsLoaded = true;
    }

    $effect(() => {
        const running = session.running;
        const sid = session.sessionId;
        minedVersion;
        wordStatusVersion;
        if (!running || !sid) {
            sessionWords = [];
            sessionWordsLoaded = false;
            sessionWordsExpanded = false;
            statusMenuWordId = null;
            return;
        }
        loadSessionWords(sid);
    });

    function openStatusMenu(word, event) {
        if (statusMenuWordId === word.id) {
            closeStatusMenu();
            return;
        }
        const rect = event.currentTarget.getBoundingClientRect();
        statusMenuPos = { x: rect.right + 8, y: rect.top };
        statusMenuWordId = word.id;
    }

    function closeStatusMenu() {
        statusMenuWordId = null;
    }

    async function selectWordStatus(word, status) {
        word.status = status;
        closeStatusMenu();
        await updateWordStatus({ wordId: word.id, status });
        wordStatusVersion += 1;
    }
</script>

{#key mediaId}
    <main class="page home" style={`--page-glow: ${glowColor}; --page-glow-soft: color-mix(in srgb, ${glowColor} 30%, transparent)`}>
        {#if media}
            <MediaHeader {media}>
                <MediaStats {mediaId} {media} refreshKey={session.running} {settings}/>
            </MediaHeader>
        {/if}

        <SentenceViewer
            {settings}
            {miniMode}
            {session}
            {mediaId}
            mediaTag={media?.tag ?? media?.title}
            onMined={() => (minedVersion += 1)}
            {wordStatusVersion}
            onStatusChanged={() => (wordStatusVersion += 1)}
        />

        {#if session.running}
            <div class="session-words-section">
                <button
                    type="button"
                    class="session-words-head"
                    onclick={() => (sessionWordsExpanded = !sessionWordsExpanded)}
                    aria-expanded={sessionWordsExpanded}
                >
                    <span class="sw-icon">{ICONS.book}</span>
                    <span class="sw-title">Mined this session</span>
                    <span class="sw-count">{sessionWords.length}</span>
                    <span class="sw-chevron">{ICONS[sessionWordsExpanded ? 'collapse' : 'expand']}</span>
                </button>

                {#if sessionWordsExpanded}
                    {#if !sessionWordsLoaded}
                        <p class="sw-empty">Loading…</p>
                    {:else if sessionWords.length === 0}
                        <p class="sw-empty">No words mined in this session yet.</p>
                    {:else}
                        <div class="session-words-grid">
                            {#each sessionWords as word (word.id)}
                                <div class="mined-word-card">
                                    <button
                                        type="button"
                                        class="mined-word-status"
                                        data-status-toggle
                                        style={`--status-color: ${STATUS_LEVELS[word.status ?? 0].color}`}
                                        title={`Status: ${STATUS_LEVELS[word.status ?? 0].label} — click to change`}
                                        aria-haspopup="menu"
                                        aria-expanded={statusMenuWordId === word.id}
                                        onclick={(e) => openStatusMenu(word, e)}
                                    ></button>
                                    {#if statusMenuWordId === word.id}
                                        <StatusMenu
                                            x={statusMenuPos.x}
                                            y={statusMenuPos.y}
                                            levels={STATUS_LEVELS}
                                            current={word.status ?? 0}
                                            onSelect={(status) => selectWordStatus(word, status)}
                                            onClose={closeStatusMenu}
                                        />
                                    {/if}
                                    <span class="mined-word-spelling">{word.spelling}</span>
                                    {#if word.definitions.length > 0}
                                        <span class="mined-word-definition">{word.definitions[0]}</span>
                                    {/if}
                                </div>
                            {/each}
                        </div>
                    {/if}
                {/if}
            </div>
        {/if}

        <MediaFormModal
            bind:show={showEditModal}
            {media}
            onSaved={() => loadMedia(mediaId)}
        />
    </main>
{/key}

<SideNav>
  <ActionButton 
    icon={ICONS.back} 
    variant="primary" 
    size="small" 
    onAction={() => history.back()} 
  />
  <div class="stats-btn-wrap">
    {#if mediaCoverSrc}
        <img class="cover-cover" src={mediaCoverSrc} alt={media?.title ?? 'Stats'} />
    {/if}
    <ActionButton 
      icon={ICONS.stats} 
      variant="secondary" 
      size="small" 
      onAction={() => goto(`/statistics?media=${mediaId}`)}
    />
  </div>
  <ActionButton 
    icon={ICONS.book} 
    variant="secondary" 
    size="small" 
    onAction={() => goto(`/dictionary?media=${mediaId}`)}
  />
  <ActionButton
    icon={ICONS.history}
    variant="secondary"
    size="small"
    onAction={() => goto(`/log?media=${mediaId}`)}
  />
  <ActionButton
    icon={ICONS.settings}
    variant="secondary"
    size="small"
    onAction={() => goto('/settings')}
  />
  <ActionButton 
    icon={ICONS.edit} 
    variant="secondary" 
    size="small" 
    onAction={() => (showEditModal = true)} 
  />
  <ActionButton
    icon={session.running ? ICONS.pause : ICONS.play}
    variant="primary"
    size="small"
    onAction={() => session.toggle()}
  />
  {#if session.running}
    <span class="session-timer" transition:fly={{ y: -8, duration: 200 }}>
      {session.formattedTime}
    </span>
  {/if}
</SideNav>

<style>
    .page.home {
        position: relative;
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
        overflow-x: hidden;
    }

    :global(body.mini-mode) .page.home {
        padding: 0;
        max-height: 100vh;
        overflow: hidden;
        gap: 0;
        background: transparent !important;
    }

    .page.home::before {
        content: '';
        position: absolute;
        top: 0;
        left: 0;
        right: 0;
        height: 420px;
        background: radial-gradient(
            ellipse at top,
            color-mix(in srgb, var(--page-glow, var(--theme-primary, #dc143c)) 20%, transparent) 0%,
            transparent 50%
        );
        pointer-events: none;
        z-index: -1;
        transition: background 0.5s ease;
    }
    
    :global(body.mini-mode) .page.home::before {
        display: none;
    }

    .session-words-section {
        width: 100%;
        max-width: 900px;
        margin-top: 0.5rem;
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 70%, #000);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 12px;
        padding: 0.65rem 1rem;
        box-sizing: border-box;
    }

    :global(body.mini-mode) .session-words-section {
        display: none;
    }

    .session-words-head {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        width: 100%;
        background: none;
        border: none;
        padding: 0;
        font: inherit;
        color: inherit;
        cursor: pointer;
    }

    .sw-icon {
        font-family: "Symbols Nerd Font";
        font-size: 1rem;
        color: var(--theme-primary, #36b7bd);
    }

    .sw-title {
        font-size: 0.9rem;
        font-weight: 600;
    }

    .sw-count {
        font-size: 0.75rem;
        font-weight: 700;
        background: color-mix(in srgb, var(--theme-primary, #36b7bd) 15%, transparent);
        color: var(--theme-primary, #36b7bd);
        border-radius: 999px;
        padding: 0.05rem 0.5rem;
    }

    .sw-chevron {
        font-family: "Symbols Nerd Font";
        font-size: 0.8rem;
        color: var(--theme-textSecondary, #b3b3b3);
        margin-left: auto;
        transition: color 0.15s ease;
    }

    .session-words-head:hover .sw-chevron {
        color: var(--theme-text, #f6f6f6);
    }

    .sw-empty {
        margin: 0.6rem 0 0.25rem;
        font-size: 0.85rem;
        color: var(--theme-textSecondary, #b3b3b3);
        font-style: italic;
    }

    .session-words-grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(160px, 1fr));
        gap: 0.5rem;
        margin-top: 0.75rem;
        padding-top: 0.75rem;
        border-top: 1px solid color-mix(in srgb, var(--theme-border, #404040) 50%, transparent);
    }

    .mined-word-card {
        position: relative;
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 55%, #000);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 10px;
        padding: 0.5rem 0.7rem 0.55rem 0.85rem;
        display: flex;
        flex-direction: column;
        gap: 0.2rem;
        min-width: 0;
    }

    .mined-word-status {
        position: absolute;
        top: 0;
        left: 0;
        bottom: 0;
        width: 6px;
        border: none;
        padding: 0;
        margin: 0;
        cursor: pointer;
        background: var(--status-color, #6c7086);
        border-top-left-radius: 10px;
        border-bottom-left-radius: 10px;
        transition: width 0.15s ease;
    }

    .mined-word-status:hover {
        width: 9px;
    }

    .mined-word-spelling {
        font-size: 0.95rem;
        font-weight: 700;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .mined-word-definition {
        font-size: 0.78rem;
        color: var(--theme-textSecondary, #b3b3b3);
        display: -webkit-box;
        -webkit-line-clamp: 2;
        -webkit-box-orient: vertical;
        line-clamp: 2;
        overflow: hidden;
        line-height: 1.35;
    }

    .session-timer {
            font-size: 0.75rem;
            font-weight: 600;
            color: var(--theme-textSecondary, #b3b3b3);
            font-variant-numeric: tabular-nums; /* keeps digit widths consistent so it doesn't jitter as numbers change */
            text-align: center;
        }

    .stats-btn-wrap {
        position: relative;
        display: inline-flex;
    }

    .cover-cover {
        position: absolute;
        top: 2px;
        left: 2px;
        width: calc(100% - 4px);
        height: calc(100% - 4px);
        object-fit: cover;
        border-radius: 100px;
        pointer-events: none;
        z-index: 1;
        transition: opacity 0.2s ease;
    }

    .stats-btn-wrap:hover .cover-cover {
        opacity: 0.3;
    }
</style>