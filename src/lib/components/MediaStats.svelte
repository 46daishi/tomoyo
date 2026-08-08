<script>
    import { getMediaStats } from '$lib/sessions.js';
    import { getVocabularyCoverage } from '$lib/coverage';
    import { ICONS } from '$lib/icons';

    let { mediaId, media, refreshKey, settings } = $props();

    let stats = $state({ last_active: null, moji_read: 0, reading_seconds: 0, words_mined: 0, session_count: 0 });
    let vocab = $state({ gathering: true, percentage: null });

    async function load() {
        stats = await getMediaStats(mediaId);
        if (settings?.estimate_coverage) {
            vocab = await getVocabularyCoverage(mediaId);
        }
    }

    $effect(() => {
        mediaId;
        refreshKey;
        settings?.estimate_coverage;
        load();
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

<div class="media-stats" style="--stat-color: {media?.color}">
    <div class="stat-tile">
        <span class="bar"></span>
        <div class="stat-header">
            <span class="stat-icon">{@html ICONS.calendar ?? ''}</span>
            <div class="stat-value">{formatLastRead(stats.last_active)}</div>
        </div>
        <div class="stat-value-label">last session</div>
    </div>

    <div class="stat-tile">
        <span class="bar"></span>
        <div class="stat-header">
            <span class="stat-icon">{@html ICONS.clock ?? ''}</span>
            <div class="stat-value">{formatDuration(stats.reading_seconds)}</div>
        </div>
        <div class="stat-value-label">time spent reading</div>
    </div>

    <div class="stat-tile">
        <span class="bar"></span>
        <div class="stat-header">
            <span class="stat-icon">{@html ICONS.translate ?? ''}</span>
            <div class="stat-value">{stats.moji_read.toLocaleString()} 文字</div>
        </div>
        <div class="stat-value-label">characters read</div>
    </div>

    <div class="stat-tile">
        <span class="bar"></span>
        <div class="stat-header">
            <span class="stat-icon">{@html ICONS.plus ?? ''}</span>
            <div class="stat-value">{stats.session_count.toLocaleString()}</div>
        </div>
        <div class="stat-value-label">sessions logged</div>
    </div>

    <div class="stat-tile">
        <span class="bar"></span>
        <div class="stat-header">
            <span class="stat-icon">{@html ICONS.book ?? ''}</span>
            <div class="stat-value">{stats.words_mined.toLocaleString()}</div>
        </div>
        <div class="stat-value-label">total words mined</div>
    </div>

    {#if settings?.estimate_coverage}
            <div class="stat-tile">
                <span class="bar"></span>
                <div class="stat-header">
                    <span class="stat-icon">{@html ICONS.star_half ?? ''}</span>
                    <div class="stat-value">{vocab.gathering ? 'Gathering data…' : `${vocab.percentage}% coverage`}</div>
                </div>
                <div class="stat-value-label">
                    {vocab.gathering ? 'vocabulary coverage' : 'last 100 sentences'}
                </div>
            </div>
        {/if}
    
</div>

<style>
    .media-stats {
        display: grid;
        grid-template-columns: repeat(3, 1fr); 
        gap: 1rem;
        width: 100%;
    }

    .stat-tile {
        position: relative;
        display: flex;
        flex-direction: column;
        justify-content: left;
        gap: 0.65rem;
        padding: 0.6rem 0.95rem;
        transition: transform 0.15s ease;
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 70%, #000);
        border-radius: 5px;
        overflow: hidden;
        box-shadow: 0 4px 8px var(--theme-shadow, rgba(0, 0, 0, 0.3));
    }

    .stat-tile:hover {
        transform: translateY(-2px);
    }

    .stat-header {
        font-family: "Noto Sans JP";
        display: flex;
        align-items: center;
        gap: 0.55rem;
    }

    .stat-icon {
        font-family: "Symbols Nerd Font";
        display: flex;
        align-items: center;
        justify-content: center;
        color: var(--stat-color, var(--theme-primary, #6c7086));
    }

    .stat-icon :global(svg) {
        width: 0.8rem;
        height: 0.8rem;
    }

    .stat-value {
        font-size: 1.1rem;
        font-weight: 700;
        color: var(--theme-text, #f4f4f4);
        font-variant-numeric: tabular-nums;
        line-height: 1.3;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .stat-value-label {
        font-size: 0.9rem;
        color: color-mix(in srgb, var(--theme-text, #f4f4f4) 45%, transparent);
        font-variant-numeric: tabular-nums;
        line-height: 1.1;
        white-space: nowrap;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .stat-tile .bar {
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
    
</style>