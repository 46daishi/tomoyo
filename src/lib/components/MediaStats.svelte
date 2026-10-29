<script>
    import { getMediaStats } from '$lib/sessions.js';
    import { getVocabularyCoverage } from '$lib/coverage';
    import { ICONS } from '$lib/icons';
    import { getVn, formatVnLength, formatVnRating } from '$lib/vndb.js';
    import { loadJitenStats, difficultyLabel, difficultyBand } from '$lib/jiten.js';

    let { mediaId, media, refreshKey, settings } = $props();

    let stats = $state({ last_active: null, moji_read: 0, reading_seconds: 0, words_mined: 0, session_count: 0 });
    let vocab = $state({ gathering: true, percentage: null });
    let vn = $state(/** @type {Record<string, any> | null} */ (null));
    let jiten = $state(/** @type {Record<string, any> | null} */ (null));
    let statsRequestId = 0;

    // Compact mode: once the cards would no longer fit in a single row next
    // to the cover, they collapse to title-only cards that expand on click.
    // The wide threshold depends on how many cards are actually shown, so
    // entries without VNDB/Jiten links stay expanded for as long as they fit.
    const CARD_MIN_WIDTH = 150; // matches the wide grid's minmax(150px, …)
    const COLUMN_GAP = 16; // 1rem
    const COMPACT_SINGLE_WIDTH = 180; // below this even two title cards are unrecognizable
    const SPAN_FIELDS_WIDTH = 340; // below this expanded fields need the full row

    let narrow = $state(false);
    let oneCol = $state(false);
    let spanFields = $state(false);
    let expanded = $state(/** @type {string | null} */ (null));
    let statsEl = $state(/** @type {HTMLElement | null} */ (null));

    // The VNDB/Jiten cards may only appear once their data loads; base the
    // threshold on whether they will be shown so it doesn't jump mid-fetch.
    let cardCount = $derived(
        2 +
            (media?.vndb_id && (media.media_type ?? 'visual_novel') === 'visual_novel' ? 1 : 0) +
            (settings?.jiten_enabled && !media?.jiten_excluded ? 1 : 0)
    );

    $effect(() => {
        const el = statsEl;
        const count = cardCount;
        if (!el) return;
        const update = () => {
            const width = el.clientWidth;
            narrow = width < count * CARD_MIN_WIDTH + (count - 1) * COLUMN_GAP;
            oneCol = width < COMPACT_SINGLE_WIDTH;
            spanFields = narrow && width < SPAN_FIELDS_WIDTH;
        };
        update();
        const observer = new ResizeObserver(update);
        observer.observe(el);
        return () => observer.disconnect();
    });

    /** @param {string} key */
    function toggleCard(key) {
        expanded = expanded === key ? null : key;
    }

    /**
     * Interactive attributes for a card head in compact mode; empty in the
     * full-width layout where every card is always expanded.
     * @param {string} key
     */
    function headAttrs(key) {
        if (!narrow) return {};
        return {
            role: 'button',
            tabindex: 0,
            'aria-expanded': expanded === key,
            onclick: () => toggleCard(key),
            onkeydown: (/** @type {KeyboardEvent} */ e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                    e.preventDefault();
                    toggleCard(key);
                }
            },
        };
    }

    async function loadStats() {
        // Session start/end can bump the refresh key back-to-back; drop an
        // older result if a newer load has already started.
        const my = ++statsRequestId;
        const nextStats = await getMediaStats(mediaId);
        if (my !== statsRequestId) return;
        stats = nextStats;

        if (!settings?.estimate_coverage) return;
        const nextVocab = await getVocabularyCoverage(mediaId);
        if (my !== statsRequestId) return;
        vocab = nextVocab;
    }

    // VNDB/Jiten enrichment for the cards below. Best-effort: a failed fetch
    // leaves the VNDB fields showing '—' and hides the Jiten card entirely.
    async function loadExternalInfo() {
        const isVn = (media?.media_type ?? 'visual_novel') === 'visual_novel';
        const vndbId = isVn ? (media?.vndb_id ?? null) : null;

        vn = null;
        if (vndbId) {
            try {
                vn = await getVn(vndbId);
            } catch (err) {
                console.error('VNDB fetch failed:', err);
            }
        }

        jiten = null;
        if (!media || !settings?.jiten_enabled || media.jiten_excluded) return;
        try {
            jiten = await loadJitenStats(mediaId, { vndbId, title: media.title ?? '' });
        } catch (err) {
            console.error('Jiten fetch failed:', err);
        }
    }

    $effect(() => {
        mediaId;
        refreshKey;
        settings?.estimate_coverage;
        loadStats();
    });

    $effect(() => {
        media;
        settings?.jiten_enabled;
        loadExternalInfo();
    });

    function formatLastRead(unixSeconds) {
        if (!unixSeconds) return 'N/A';
        const date = new Date(unixSeconds * 1000);
        return date.toLocaleDateString('ja-JP', {
            year: 'numeric',
            month: 'short',
            day: 'numeric'
        });
    }


    function formatDuration(totalSeconds) {
        const h = Math.floor(totalSeconds / 3600);
        const m = Math.floor((totalSeconds % 3600) / 60);
        if (h > 0) return `${h}h ${m}m`;
        return `${m}m`;
    }
</script>

<div class="media-stats" class:compact={narrow} class:single={oneCol} bind:this={statsEl} style="--stat-color: {media?.color}">
    <div class="stat-card" class:expanded={expanded === 'activity'} class:span={spanFields && expanded === 'activity'}>
        <span class="bar"></span>
        <div class="stat-card-head" {...headAttrs('activity')}>
            <span class="stat-icon">{@html ICONS.clock ?? ''}</span>
            <span class="stat-card-title">Activity</span>
            {#if narrow}
                <span class="stat-card-chevron">{@html ICONS[expanded === 'activity' ? 'collapse' : 'expand'] ?? ''}</span>
            {/if}
        </div>
        {#if !narrow || expanded === 'activity'}
            <div class="stat-fields">
                <div class="stat-field">
                    <span class="stat-icon-inline">{@html ICONS.calendar2 ?? ''}</span>
                    <span class="stat-field-value">{formatLastRead(stats.last_active)}</span>
                    <span class="stat-field-label">last session</span>
                </div>
                <div class="stat-field">
                    <span class="stat-icon-inline">{@html ICONS.clock ?? ''}</span>
                    <span class="stat-field-value">{formatDuration(stats.reading_seconds)}</span>
                    <span class="stat-field-label">on record</span>
                </div>
                <div class="stat-field">
                    <span class="stat-icon-inline">{@html ICONS.history ?? ''}</span>
                    <span class="stat-field-value">{stats.session_count.toLocaleString()}</span>
                    <span class="stat-field-label">sessions</span>
                </div>
            </div>
        {/if}
    </div>

    <div class="stat-card" class:expanded={expanded === 'progress'} class:span={spanFields && expanded === 'progress'}>
        <span class="bar"></span>
        <div class="stat-card-head" {...headAttrs('progress')}>
            <span class="stat-icon">{@html ICONS.book ?? ''}</span>
            <span class="stat-card-title">Progress</span>
            {#if narrow}
                <span class="stat-card-chevron">{@html ICONS[expanded === 'progress' ? 'collapse' : 'expand'] ?? ''}</span>
            {/if}
        </div>
        {#if !narrow || expanded === 'progress'}
            <div class="stat-fields">
                <div class="stat-field">
                    <span class="stat-icon-inline">{@html ICONS.book ?? ''}</span>
                    <span class="stat-field-value">{stats.moji_read.toLocaleString()} 文字</span>
                    <span class="stat-field-label">read</span>
                </div>
                <div class="stat-field">
                    <span class="stat-icon-inline">{@html ICONS.plus ?? ''}</span>
                    <span class="stat-field-value">{stats.words_mined.toLocaleString()}</span>
                    <span class="stat-field-label">words mined</span>
                </div>
                {#if settings?.estimate_coverage}
                    <div class="stat-field">
                        <span class="stat-icon-inline">{@html ICONS.half_circle ?? ''}</span>
                        <span class="stat-field-value">{vocab.gathering ? 'Gathering…' : vocab.percentage != null ? `${vocab.percentage}%` : 'N/A'}</span>
                        <span class="stat-field-label">avg vocab coverage</span>
                    </div>
                {/if}
            </div>
        {/if}
    </div>

    {#if media?.vndb_id && (media.media_type ?? 'visual_novel') === 'visual_novel'}
        <div class="stat-card" class:expanded={expanded === 'vndb'} class:span={spanFields && expanded === 'vndb'}>
            <span class="bar"></span>
            <div class="stat-card-head" {...headAttrs('vndb')}>
                <span class="stat-icon">{@html ICONS.star ?? ''}</span>
                <span class="stat-card-title">VNDB</span>
                {#if narrow}
                    <span class="stat-card-chevron">{@html ICONS[expanded === 'vndb' ? 'collapse' : 'expand'] ?? ''}</span>
                {/if}
            </div>
            {#if !narrow || expanded === 'vndb'}
                <div class="stat-fields">
                    <div class="stat-field">
                        <span class="stat-icon-inline">{@html ICONS.star ?? ''}</span>
                        <span class="stat-field-value">{formatVnRating(vn)}</span>
                        <span class="stat-field-label">rating</span>
                    </div>
                    <div class="stat-field">
                        <span class="stat-icon-inline">{@html ICONS.clock ?? ''}</span>
                        <span class="stat-field-value">{vn ? formatVnLength(vn) : '—'}</span>
                        <span class="stat-field-label">avg length</span>
                    </div>
                </div>
            {/if}
        </div>
    {/if}

    {#if settings?.jiten_enabled && jiten}
        {@const jitenBand = difficultyBand(jiten.difficulty)}
        <div class="stat-card" class:expanded={expanded === 'jiten'} class:span={spanFields && expanded === 'jiten'}>
            <span class="bar"></span>
            <div class="stat-card-head" {...headAttrs('jiten')}>
                <span class="stat-icon">{@html ICONS.book_open ?? ''}</span>
                <span class="stat-card-title">Jiten</span>
                {#if narrow}
                    <span class="stat-card-chevron">{@html ICONS[expanded === 'jiten' ? 'collapse' : 'expand'] ?? ''}</span>
                {/if}
            </div>
            {#if !narrow || expanded === 'jiten'}
                <div class="stat-fields">
                    <div class="stat-field">
                        <span class="stat-icon-inline">{@html ICONS.book_open ?? ''}</span>
                        <span class="stat-field-value">{jiten.characterCount != null ? `${jiten.characterCount.toLocaleString()} 文字` : '—'}</span>
                        <span class="stat-field-label">total length</span>
                    </div>
                    <div class="stat-field">
                        <span class="stat-icon-inline">{@html ICONS.plus ?? ''}</span>
                        <span class="stat-field-value">{jiten.uniqueWords != null ? jiten.uniqueWords.toLocaleString() : '—'}</span>
                        <span class="stat-field-label">unique words</span>
                    </div>
                    <div class="stat-field">
                        <span class="stat-icon-inline">{@html ICONS.star ?? ''}</span>
                        <span class="stat-field-value {jitenBand != null ? `diff-band-${jitenBand}` : ''}" title={jiten.difficulty != null ? `${jiten.difficulty.toFixed(1)}/5 on jiten.moe` : ''}>{difficultyLabel(jiten.difficulty) ?? '—'}</span>
                        <span class="stat-field-label">difficulty</span>
                    </div>
                </div>
            {/if}
        </div>
    {/if}
</div>

<style>
    .media-stats {
        display: grid;
        /* One compact row of cards next to the cover; the JS measured width
           decides when they collapse to the compact title-only form before
           this ever has to wrap. */
        grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
        gap: 1rem;
        width: 100%;
    }

    .stat-card {
        position: relative;
        display: flex;
        flex-direction: column;
        align-items: flex-start;
        text-align: left;
        gap: 0.4rem;
        padding: 0.35rem 0.7rem 0.35rem;
        transition: transform 0.15s ease;
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 70%, #000);
        border-radius: 5px;
        overflow: hidden;
        box-shadow: 0 4px 8px var(--theme-shadow, rgba(0, 0, 0, 0.3));
    }

    .stat-card:hover {
        transform: translateY(-2px);
    }

    .stat-card-head {
        font-family: "Noto Sans JP";
        display: flex;
        align-items: center;
        gap: 0.5rem;
        padding-bottom: 0.3rem;
        border-bottom: 1px solid color-mix(in srgb, var(--theme-border, #404040) 55%, transparent);
    }

    .stat-card-title {
        font-size: 0.9rem;
        font-weight: 700;
        color: var(--theme-text, #f4f4f4);
    }

    .stat-icon {
        font-family: "Symbols Nerd Font";
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 0.9rem;
        color: var(--stat-color, var(--theme-primary, #6c7086));
    }

    .stat-fields {
        display: flex;
        flex-direction: column;
        gap: 0.3rem;
        min-width: 0;
    }

    .stat-field {
        display: flex;
        align-items: center;
        gap: 0.35rem;
        min-width: 0;
        /* Color on the container, not the value: otherwise the scoped
           .stat-field-value rule outranks the shared .diff-band-* colors. */
        color: var(--theme-text, #f4f4f4);
    }

    .stat-icon-inline {
        font-family: "Symbols Nerd Font";
        font-size: 0.8rem;
        color: inherit;
        flex-shrink: 0;
    }

    .stat-field-value {
        font-size: 0.85rem;
        font-weight: 700;
        font-variant-numeric: tabular-nums;
        line-height: 1.2;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    /* Values carrying a hover tooltip (difficulty score): dotted
       underline + help cursor, matching statistics and cover hover
       bars. */
    .stat-field-value[title] {
        text-decoration: underline dotted;
        text-underline-offset: 3px;
        text-decoration-color: var(--theme-primary, #36b7bd);
        cursor: help;
    }

    .stat-field-label {
        font-size: 0.7rem;
        color: color-mix(in srgb, var(--theme-text, #f4f4f4) 45%, transparent);
        line-height: 1;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .stat-card .bar {
        position: absolute;
        top: 0;
        left: 0;
        bottom: 0;
        width: 3px;
        border: none;
        padding: 0;
        margin: 0;
        background: var(--stat-color, var(--theme-primary, #6c7086));
        border-top-left-radius: 12px;
        border-bottom-left-radius: 12px;
        transition: width 0.15s ease;
    }

    /* Compact (title-only) cards: a steady two per row, dropping to a single
       column only when even the titles no longer fit. align-items keeps
       collapsed siblings at their natural height next to an expanded card. */
    .media-stats.compact {
        grid-template-columns: repeat(2, minmax(0, 1fr));
        grid-auto-flow: row dense;
        align-items: start;
    }

    .media-stats.compact.single {
        grid-template-columns: minmax(0, 1fr);
    }

    /* Too narrow for two columns of field values: the expanded card takes
       the whole row so its values stay readable. */
    .media-stats.compact .stat-card.span {
        grid-column: 1 / -1;
    }

    .media-stats.compact .stat-card-head {
        cursor: pointer;
        user-select: none;
    }

    .stat-card.expanded {
        grid-column: 1 / -1;
    }

    .stat-card-chevron {
        font-family: "Symbols Nerd Font";
        font-size: 0.75rem;
        margin-left: auto;
        color: var(--theme-textSecondary, #b3b3b3);
        transition: color 0.15s ease;
    }

    .media-stats.compact .stat-card-head:hover .stat-card-chevron {
        color: var(--theme-text, #f4f4f4);
    }
</style>
