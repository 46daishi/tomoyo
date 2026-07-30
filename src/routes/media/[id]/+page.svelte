<script>
    import { page } from '$app/state';
    import { onMount } from 'svelte';
    import { fly } from 'svelte/transition'; // Added missing transition import
    import { getDb } from '$lib/db';
    import { setMediaTitle } from '$lib/stores/presence.svelte.js';
    import { loadSettings } from '$lib/settings.js';
    import { goto } from '$app/navigation';

    import { initMiniMode } from '$lib/miniMode.js';
    import { createSessionStore } from '$lib/stores/session.svelte.js';

    import { ICONS } from '$lib/icons';

    import MediaHeader from '$lib/components/MediaHeader.svelte';
    import SideNav from '$lib/components/SideNav.svelte';
    import ActionButton from '$lib/components/ActionButton.svelte';
    import SentenceViewer from '$lib/components/SentenceWindow.svelte';
    import MediaFormModal from '$lib/components/MediaFormModal.svelte';

    let settings = $state(null);
    let showEditModal = $state(false);
    let miniMode = $state(false);

    let mediaId = $derived(Number(page.params.id));
    let media = $state(null);

    // Initialize session store
    const session = createSessionStore(mediaId);

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

    onMount(async () => {
        settings = await loadSettings();
        const miniModeManager = initMiniMode(settings, (mode) => (miniMode = mode));

        return () => {
            session.destroy();
            miniModeManager.destroy();
        };
    });
</script>

{#key mediaId}
    <main class="page home">
        {#if media}
            <MediaHeader {media} />
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
  <ActionButton 
    icon={ICONS.edit} 
    variant="secondary" 
    size="small" 
    onAction={() => (showEditModal = true)} 
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
    onAction={() => goto(`/dictionary?media=${mediaId}`)}
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

    :global(body.mini-mode) .page.home {
        padding: 0;
        max-height: 100vh;
        overflow: hidden;
        gap: 0;
        background: transparent !important;
    }

    .session-timer {
            font-size: 0.75rem;
            font-weight: 600;
            color: var(--theme-textSecondary, #b3b3b3);
            font-variant-numeric: tabular-nums; /* keeps digit widths consistent so it doesn't jitter as numbers change */
            text-align: center;
        }
</style>