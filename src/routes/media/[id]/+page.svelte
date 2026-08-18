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
</script>

{#key mediaId}
    <main class="page home" style={`--page-glow: ${glowColor}; --page-glow-soft: color-mix(in srgb, ${glowColor} 30%, transparent)`}>
        {#if media}
            <MediaHeader {media}>
                <MediaStats {mediaId} {media} refreshKey={session.running} {settings}/>
            </MediaHeader>
        {/if}

        <SentenceViewer {settings} {miniMode} {session} {mediaId} mediaTag={media?.tag ?? media?.title} />

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
        overflow:hidden;
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