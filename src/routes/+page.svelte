<script>
    import ActionButton from "$lib/components/ActionButton.svelte";
    import SelectInput from "$lib/components/SelectInput.svelte";
    import SideNav from "$lib/components/SideNav.svelte";
    import MediaFormModal from '$lib/components/MediaFormModal.svelte';
    import AboutModal from '$lib/components/AboutModal.svelte';
    import { ICONS } from "$lib/icons.js";
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';
    import { getDb, coverSrc } from '$lib/db';
    import { confirm } from '@tauri-apps/plugin-dialog';
    import { STATUS_COLORS, FILTER_OPTIONS, STATUS_OPTIONS } from '$lib/constants.js';
    import { clearDictionaryData } from "$lib/dictionary";
    import { loadSettings } from '$lib/settings.js';

    let mediaList = $state([]);
    let statusFilter = $state('all');
    let showModal = $state(false);
    let editingMedia = $state(null);
    let settings = $state(/** @type {Record<string, any> | null} */ (null));
    let showAbout = $state(false);

    let profilePicSrc = $derived(settings?.profile_picture ? coverSrc(settings.profile_picture) : null);

    const STATUS_ORDER = Object.fromEntries(STATUS_OPTIONS.map((o, i) => [o.value, i]));

    async function loadMedia() {
        const db = await getDb();
        mediaList = await db.select('SELECT * FROM media ORDER BY updated_at DESC');
    }

    let filtered = $derived.by(() => {
        const list = statusFilter === 'all'
            ? [...mediaList]
            : mediaList.filter((m) => m.status === statusFilter);

        return list.sort((a, b) => {
            const rankDiff = (STATUS_ORDER[a.status] ?? 0) - (STATUS_ORDER[b.status] ?? 0);
            if (rankDiff !== 0) return rankDiff;
            return b.updated_at - a.updated_at;
        });
    });

    function openAddModal() {
        editingMedia = null;
        showModal = true;
    }

    function openEditModal(media) {
        editingMedia = media;
        showModal = true;
    }
    
    async function handleDelete(media) {
        const yes = await confirm(
            `Delete "${media.title}"? This also permanently erases all reading stats, lookup history, and mined dictionary entries tied to it. This cannot be undone.`,
            { title: 'Delete media', kind: 'warning' }
        );
        if (!yes) return;
    
        const db = await getDb();
    
        await db.execute('DELETE FROM sessions WHERE media_id = $1', [media.id]);
        await db.execute('DELETE FROM sentence_read_events WHERE media_id = $1', [media.id]);
        await db.execute('DELETE FROM review_sessions WHERE media_id = $1', [media.id]);
        await db.execute('DELETE FROM review_log WHERE media_id = $1', [media.id]);
        await clearDictionaryData({ mediaId: media.id });
    
        await db.execute('DELETE FROM media WHERE id = $1', [media.id]);
        await loadMedia();
    }

    function handleCardKeydown(e, id) {
        if (e.target instanceof HTMLButtonElement) return;
        if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            openMedia(id);
        }
    }

    function openMedia(id) {
        goto(`/media/${id}`);
    }

    async function onMountHome() {
        settings = await loadSettings();
        await loadMedia();
    }

    onMount(onMountHome);
</script>

<main class="page home">
    <div class="toolbar">
        <SelectInput
            options={FILTER_OPTIONS}
            value={statusFilter}
            on:change={(e) => (statusFilter = e.target.value)}
        />
        <ActionButton
            icon={ICONS.plus}
            variant="primary"
            size="tiny"
            onAction={openAddModal}
        />
    </div>

    <div class="grid">
        {#each filtered as media (media.id)}
            <div
                class="card"
                style="--accent: {media.color}"
                onclick={() => openMedia(media.id)}
                onkeydown={(e) => handleCardKeydown(e, media.id)}
                role="button"
                tabindex="0"
            >
                <div class="cover">
                    {#if media.cover_path}
                        <img src={coverSrc(media.cover_path)} alt={media.title} />
                    {:else}
                        <div class="cover-placeholder"></div>
                    {/if}

                    <div class="cover-actions" onclick={(e) => e.stopPropagation()}>
                        <ActionButton
                            icon={ICONS.edit}
                            variant="primary"
                            size="mini"
                            onAction={() => openEditModal(media)}
                        />
                        <ActionButton
                            icon={ICONS.trash}
                            variant="danger"
                            size="mini"
                            onAction={() => handleDelete(media)}
                        />
                    </div>
                </div>

                <div class="title">{media.title}</div>
                <div class="status">
                    <span class="status-dot" style="--dot-color: {STATUS_COLORS[media.status]}"></span>
                    {media.status}
                </div>
            </div>
        {/each}
    </div>

    {#if mediaList.length === 0}
        <div class="empty-notice">
            <p>No media added yet.</p>
        </div>
    {:else if filtered.length === 0}
        <p class="empty-notice">There are no media entries that match this query.</p>
    {/if}

    <MediaFormModal bind:show={showModal} media={editingMedia} onSaved={loadMedia} />

    <AboutModal bind:show={showAbout} />
</main>

<SideNav>
    <div class="stats-btn-wrap">
        {#if profilePicSrc}
            <img class="profile-cover" src={profilePicSrc} alt="Your profile" />
        {/if}
        <ActionButton
        icon={ICONS.stats}
        variant="secondary"
        size="small"
        onAction={() => goto('/statistics')}
        />
    </div>
  <ActionButton
    icon={ICONS.book}
    variant="secondary"
    size="small"
    onAction={() => goto(`/dictionary`)}
  />
  <ActionButton
    icon={ICONS.history}
    variant="secondary"
    size="small"
    onAction={() => goto('/log')}
  />
  <ActionButton
    icon={ICONS.settings}
    variant="secondary"
    size="small"
    onAction={() => goto('/settings')}
  />
  <ActionButton
    icon={ICONS.question}
    variant="secondary"
    size="small"
    onAction={() => (showAbout = true)}
  />
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

    .toolbar {
        display: flex;
        gap: 10px;
    }

    .grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
        gap: 1.5rem 2.5rem;
        width: 100%;
        max-width: 1100px;
        margin-top: 10px;
    }

    @media (max-width: 550px) {
        .grid {
            grid-template-columns: repeat(2, minmax(120px, 1fr));
        }
    }

    .card {
        display: flex;
        flex-direction: column;
        background: none;
        border: none;
        padding: 0;
        cursor: pointer;
        text-align: left;
        color: inherit;
    }

    .cover {
        position: relative;
        aspect-ratio: 2 / 3;
        width: 100%;
        border-radius: 8px;
        overflow: hidden;
        background: var(--surface1, #313244);
        border: 2px solid transparent;
        transition: border-color 0.15s ease, transform 0.15s ease;
    }

    .cover img,
    .cover-placeholder {
        position: relative;
        top: 0;
        transition: filter 0.3s ease;
    }

    .card:hover .cover {
        border-color: var(--accent);
        transform: translateY(-2px);
    }

    .card:hover .cover img,
    .card:hover .cover-placeholder {
        filter: brightness(60%);
    }

    .cover-actions {
        position: absolute;
        top: 8px;
        right: 8px;
        display: flex;
        gap: 0.4rem;
        opacity: 0;
        transition: opacity 0.4s ease;
        z-index: 2;
    }

    .card:hover .cover-actions,
    .card:focus-visible .cover-actions {
        opacity: 1;
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
        background: linear-gradient(135deg, color-mix(in srgb, var(--accent, var(--theme-primary, #36b7bd)) 35%, #000), var(--theme-surface, #2d2d2d));
    }

    .title {
        font-family: "Noto Sans JP", Inter, sans-serif;
        margin-top: 0.5rem;
        font-weight: 600;
        font-size: 0.9rem;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .status {
        display: flex;
        align-items: center;
        gap: 0.3rem;
        margin-left: 0.2rem;
        font-size: 0.75rem;
        color: var(--subtext0, #a6adc8);
        text-transform: capitalize;
    }

    .status-dot {
        width: 6px;
        height: 6px;
        border-radius: 50%;
        background: var(--dot-color, var(--subtext0, #a6adc8));
        flex-shrink: 0;
    }

    .empty-notice {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 1rem;
        color: var(--theme-textSecondary, #b3b3b3);
        font-size: 1.2rem;
        text-align: center;
        margin-top: 3rem;
        opacity: 0.85;
    }

    .stats-btn-wrap {
        position: relative;
        display: inline-flex;
    }

    .profile-cover {
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

    .stats-btn-wrap:hover .profile-cover {
        opacity: 0.3;
    }
</style>