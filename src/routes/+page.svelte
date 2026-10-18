<script>
    import ActionButton from "$lib/components/ActionButton.svelte";
    import CategoryBar from "$lib/components/CategoryBar.svelte";
    import SelectInput from "$lib/components/SelectInput.svelte";
    import SideNav from "$lib/components/SideNav.svelte";
    import MediaFormModal from '$lib/components/MediaFormModal.svelte';
    import AboutModal from '$lib/components/AboutModal.svelte';
    import { ICONS } from "$lib/icons.js";
    import { onMount, onDestroy } from 'svelte';
    import { goto } from '$app/navigation';
    import { getDb } from '$lib/db';
    import { getAllMediaStats } from '$lib/sessions.js';
    import { getAllCachedJitenStats, loadJitenStats, difficultyLabel, difficultyBand } from '$lib/jiten.js';
    import AppImage from '$lib/components/AppImage.svelte';
    import { confirm } from '@tauri-apps/plugin-dialog';
    import { STATUS_COLORS, FILTER_OPTIONS, STATUS_OPTIONS } from '$lib/constants.js';
    import { MEDIA_TYPES } from '$lib/mediaTypes.js';
    import { clearDictionaryData } from "$lib/dictionary";
    import { loadSettings } from '$lib/settings.js';

    let mediaList = $state([]);
    let statusFilter = $state('all');
    // Per-media stats for the cover hover bars (sessions aggregates +
    // cached Jiten difficulty), keyed by media id. Loaded once, never
    // per hover; Jiten stays cache-only here (no network from the grid).
    let coverStats = $state(/** @type {Record<number, any>} */ ({}));
    let showModal = $state(false);
    let editingMedia = $state(null);
    let settings = $state(/** @type {Record<string, any> | null} */ (null));
    let showAbout = $state(false);

    const STATUS_ORDER = Object.fromEntries(STATUS_OPTIONS.map((o, i) => [o.value, i]));

    // Home category pills: "All" plus the shared media-type registry
    // (mediaTypes.js), so labels/icons never drift from the modal.
    const CATEGORIES = [
        { value: 'all', label: 'All', icon: ICONS.grid },
        ...MEDIA_TYPES,
    ];

    const CATEGORY_BY_VALUE = Object.fromEntries(CATEGORIES.map((c) => [c.value, c]));

    let categoryFilter = $state('all');

    async function loadMedia() {
        const db = await getDb();
        mediaList = await db.select('SELECT * FROM media ORDER BY updated_at DESC');
    }

    async function loadCoverStats() {
        // Sessions and Jiten resolve independently: either source failing
        // (e.g. cache columns missing before a restart picks up the
        // migration) must never hide the other.
        const [sessions, jiten] = await Promise.all([
            getAllMediaStats().catch((/** @type {unknown} */ err) => {
                console.warn('Cover session stats unavailable:', err);
                return /** @type {Record<number, { moji_read: number, reading_seconds: number, last_active: number|null }>} */ ({});
            }),
            getAllCachedJitenStats().catch((/** @type {unknown} */ err) => {
                console.warn('Cover Jiten stats unavailable:', err);
                return /** @type {Record<number, { deckId: number, characterCount: number|null, difficulty: number|null }>} */ ({});
            }),
        ]);
        const merged = /** @type {Record<number, any>} */ ({});
            for (const m of mediaList) {
                const s = sessions[m.id];
                const j = jiten[m.id];
                // Always present (even all-zero) so the time/characters
                // bars render for fresh media too; only difficulty stays
                // conditional on cached Jiten data.
                merged[m.id] = {
                    moji_read: s?.moji_read ?? 0,
                    reading_seconds: s?.reading_seconds ?? 0,
                    last_active: s?.last_active ?? null,
                    difficulty: j?.difficulty ?? null,
                };
        }
        coverStats = merged;
    }

    /** @param {number} totalSeconds */
    function formatBarsDuration(totalSeconds) {
        const h = Math.floor(totalSeconds / 3600);
        const m = Math.floor((totalSeconds % 3600) / 60);
        if (h > 0) return `${h}h ${m}m`;
        return `${m}m`;
    }

    /** @param {number|null|undefined} unixSeconds */
    function formatLastActive(unixSeconds) {
        if (!unixSeconds) return '—';
        const diff = Date.now() / 1000 - unixSeconds;
        if (diff < 3600) return `${Math.max(1, Math.floor(diff / 60))}m ago`;
        if (diff < 86400) return `${Math.floor(diff / 3600)}h ago`;
        if (diff < 86400 * 30) return `${Math.floor(diff / 86400)}d ago`;
        return new Date(unixSeconds * 1000).toLocaleDateString('ja-JP', {
            year: 'numeric',
            month: 'short',
            day: 'numeric',
        });
    }

    let filtered = $derived.by(() => {
        const list = mediaList.filter(
            (m) =>
                (statusFilter === 'all' || m.status === statusFilter) &&
                (categoryFilter === 'all' || (m.media_type ?? 'visual_novel') === categoryFilter)
        );

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
        await loadCoverStats();
        // Resolve uncached media in the background (sequential, one at a
        // time): the grid never waits for the network, and each resolved
        // deck is cached, so repeat visits cost nothing.
        backfillJitenCache();
    }

    onMount(onMountHome);

    let coverBackfillCancelled = false;
    onDestroy(() => {
        coverBackfillCancelled = true;
    });

    async function backfillJitenCache() {
        if (!settings?.jiten_enabled) return;
        const attempted = new Set();
        for (const m of mediaList) {
            if (coverBackfillCancelled) return;
            if (m.jiten_excluded) continue;
            if (attempted.has(m.id)) continue;
            attempted.add(m.id);
            if (coverStats[m.id]?.difficulty != null) continue;
            try {
                const stats = await loadJitenStats(m.id, {
                    vndbId: (m.media_type ?? 'visual_novel') === 'visual_novel' ? (m.vndb_id ?? null) : null,
                    title: m.title ?? '',
                });
                if (coverBackfillCancelled) return;
                if (!stats) continue;
                coverStats = {
                    ...coverStats,
                    [m.id]: {
                        moji_read: coverStats[m.id]?.moji_read ?? 0,
                        reading_seconds: coverStats[m.id]?.reading_seconds ?? 0,
                        last_active: coverStats[m.id]?.last_active ?? null,
                        difficulty: stats.difficulty,
                    },
                };
            } catch (err) {
                console.warn(`Jiten backfill failed for media ${m.id}:`, err);
            }
        }
    }
</script>

<main class="page home">
    <div class="controls-row">
        <div class="category-row">
            <CategoryBar items={CATEGORIES} active={categoryFilter} onSelect={(v) => (categoryFilter = v)} />
        </div>

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
    </div>

    <div class="grid">
        {#each filtered as media (media.id)}
            {@const barStats = coverStats[media.id] ?? null}
            {@const badgeType = CATEGORY_BY_VALUE[media.media_type ?? 'visual_novel'] ?? CATEGORY_BY_VALUE.other}
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
                        <AppImage path={media.cover_path} alt={media.title} />
                    {:else}
                        <div class="cover-placeholder"></div>
                    {/if}
                    <div class="cover-shade" aria-hidden="true"></div>
                    {#if categoryFilter === 'all'}
                        <div class="cover-type-badge" aria-hidden="true">
                            <span class="cover-type-icon">{@html badgeType.icon}</span>
                        </div>
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
                    {#if barStats}
                        {@const showBarDifficulty = settings?.jiten_enabled && !media.jiten_excluded && barStats.difficulty != null}
                        <div class="cover-meta" aria-hidden="true">
                            {#if categoryFilter === 'all'}
                                {@const barType = CATEGORY_BY_VALUE[media.media_type ?? 'visual_novel'] ?? CATEGORY_BY_VALUE.other}
                                <div class="cover-meta-row">
                                    <span class="cover-meta-left"><span class="cover-meta-icon">{@html barType.icon}</span><span class="cover-meta-value">{barType.label}</span></span>
                                </div>
                            {/if}
                            <div class="cover-meta-row">
                                <span class="cover-meta-left"><span class="cover-meta-icon">{@html ICONS.book}</span><span class="cover-meta-value">{formatLastActive(barStats.last_active)}</span></span>
                            </div>
                            <div class="cover-meta-row">
                                <span class="cover-meta-left"><span class="cover-meta-icon">{@html ICONS.clock}</span><span class="cover-meta-value">{formatBarsDuration(barStats.reading_seconds)}</span></span>
                            </div>
                            <div class="cover-meta-row">
                                <span class="cover-meta-left"><span class="cover-meta-icon">{@html ICONS.translate}</span><span class="cover-meta-value">{barStats.moji_read.toLocaleString()} 文字</span></span>
                            </div>
                            {#if showBarDifficulty}
                                {@const barBand = difficultyBand(barStats.difficulty)}
                                <div class="cover-meta-row">
                                    <span class="cover-meta-left"><span class="cover-meta-icon">{@html ICONS.star}</span><span
                                        class="cover-meta-value {barBand != null ? `diff-band-${barBand}` : ''}"
                                        title={`${barStats.difficulty.toFixed(1)}/5 on jiten.moe`}
                                    >{difficultyLabel(barStats.difficulty)}</span></span>
                                </div>
                            {/if}
                        </div>
                    {/if}
                </div>

                <div class="title">{media.title}</div>
                <div class="status" style="--dot-color: {STATUS_COLORS[media.status]}">
                    <span class="status-dot"></span>
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
        {#if settings?.profile_picture}
            <AppImage path={settings.profile_picture} class="profile-cover" alt="Your profile" />
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

    /* Category bar + status filter/plus share one centered row: the whole
       group stays together, the bar shrinks and scrolls when space runs out. */
    .controls-row {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 0.75rem;
        width: 100%;
        max-width: 1100px;
    }

    .toolbar {
        display: flex;
        flex-shrink: 0;
        gap: 10px;
    }

    .category-row {
        display: flex;
        justify-content: center;
        min-width: 0;
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
        /* Container for cover-relative units below: inline-size is
           parent-driven here, so this changes no layout. */
        container-type: inline-size;
    }

    .cover :global(img),
    .cover-placeholder {
        position: relative;
        top: 0;
    }

    /* Hover dimming via an opacity shade, not an animated filter: fading
       a black overlay (opacity 0 -> 0.4) renders identically to
       brightness(60%) but never re-rasters the photo, so the fade can't
       strobe on weaker compositors. The shade sits above the image and
       below the action buttons. */
    .cover-shade {
        position: absolute;
        inset: 0;
        background: #000;
        opacity: 0;
        transition: opacity 0.3s ease;
        pointer-events: none;
    }

    .card:hover .cover {
        border-color: var(--accent);
        transform: translateY(-2px);
    }

    .card:hover .cover-shade {
        opacity: 0.4;
    }

    .cover-actions {
        position: absolute;
        top: 8px;
        right: 8px;
        display: flex;
        gap: 0.4em;
        opacity: 0;
        transition: opacity 0.4s ease;
        z-index: 2;
        /* Overlay buttons scale with the cover: mini buttons size in em,
           so driving the row font in cover-width units keeps them
           proportional (bounded so tiny covers stay tappable and huge
           covers don't grow clown buttons). */
        font-size: 1rem;
        font-size: clamp(0.85rem, 10cqw, 1.15rem);
    }

    .card:hover .cover-actions,
    .card:focus-visible .cover-actions {
        opacity: 1;
    }

    /* Persistent type badge (All tab only): transparent black square in
       the cover's bottom-left corner. Fades out on hover, when the stat
       bars (including the type row) take over the same space. */
    .cover-type-badge {
        position: absolute;
        left: 0;
        bottom: 0;
        z-index: 1;
        display: flex;
        align-items: center;
        justify-content: center;
        width: 2rem;
        height: 2rem;
        background: rgba(0, 0, 0, 0.55);
        border-top-right-radius: 6px;
        color: #f5f5f5;
        pointer-events: none;
        transition: opacity 0.25s ease;
    }

    .card:hover .cover-type-badge,
    .card:focus-visible .cover-type-badge {
        opacity: 0;
    }

    .cover-type-icon {
        display: inline-flex;
        font-family: "Symbols Nerd Font";
        font-size: 0.9rem;
        line-height: 1;
    }

    /* Hover stat bars pinned to the cover bottom: alternating black
       translucency, icon + field left, value right. Display-only
       (pointer-events none) so card clicks pass through; only the
       difficulty value re-enables pointer for its hover tooltip. */
    .cover-meta {
        position: absolute;
        left: 0;
        right: 0;
        bottom: 0;
        z-index: 1;
        opacity: 0;
        transition: opacity 0.25s ease;
        pointer-events: none;
    }

    .card:hover .cover-meta,
    .card:focus-visible .cover-meta {
        opacity: 1;
    }

    .cover-meta-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 0.5rem;
        padding: 0.28rem 0.6rem;
        color: #f5f5f5;
        font-size: 0.72rem;
        line-height: 1.3;
    }

    .cover-meta-row:nth-child(odd) {
        background: rgba(0, 0, 0, 0.65);
    }

    .cover-meta-row:nth-child(even) {
        background: rgba(0, 0, 0, 0.45);
    }

    .cover-meta-left {
        display: inline-flex;
        align-items: center;
        gap: 0.35rem;
        min-width: 0;
        overflow: hidden;
        white-space: nowrap;
        text-overflow: ellipsis;
    }

    .cover-meta-icon {
        display: inline-flex;
        flex-shrink: 0;
        font-family: "Symbols Nerd Font";
        font-size: 0.75rem;
        opacity: 0.85;
    }

    .cover-meta-icon :global(svg) {
        width: 0.75rem;
        height: 0.75rem;
    }

    .cover-meta-value {
        flex-shrink: 0;
        font-variant-numeric: tabular-nums;
        white-space: nowrap;
    }

    .cover-meta-value[title] {
        pointer-events: auto;
        cursor: help;
        text-decoration: underline dotted;
        text-underline-offset: 3px;
    }

    .cover :global(img) {
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
        font-weight: 600;
        color: var(--dot-color, var(--subtext0, #a6adc8));
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

    :global(.profile-cover) {
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

    .stats-btn-wrap:hover :global(.profile-cover) {
        opacity: 0.3;
    }
</style>