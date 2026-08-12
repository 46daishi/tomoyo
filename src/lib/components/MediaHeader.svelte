<script>
    import { coverSrc } from '$lib/db';
    import { STATUS_COLORS } from '$lib/constants.js';
    import { openUrl } from '@tauri-apps/plugin-opener';
    import { vndbUrl } from '$lib/vndb.js';

    let { media, children } = $props();

    /** @param {string} id */
    async function openVndb(id) {
        const url = vndbUrl(id);
        if (url) await openUrl(url);
    }
</script>

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
            {#if media.vndb_id}
                <button class="vndb-link" onclick={() => openVndb(media.vndb_id)} title="Open on VNDB">v{media.vndb_id.replace(/^v/i, '')}</button>
            {/if}
        </div>

        {#if children}
            <div class="media-stats-slot">
                {@render children()}
            </div>
        {/if}
    </div>
</div>

<style>
    .title-row {
        font-family: "Noto Sans JP";
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
            max-width: 900px;
            margin-top: 1rem;
    }

    .cover {
        flex-shrink: 0;
        aspect-ratio: 2 / 3;
        width: 153px;
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
        flex: 1;
        min-width: 0;
        display: flex;
        flex-direction: column;
        align-items: flex-start;
        text-align: left;
        gap: 0.3rem;
        padding-top: 0.2rem;
    }

    .media-stats-slot {
        width: 100%;
        margin-top: 0.75rem;
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
        font-family: "Noto Sans JP";
        padding-top: 0.2rem;
        display: flex;
        align-items: center;
        gap: 0.4rem;
        font-size: 1rem;
        color: var(--theme-textSecondary, #b3b3b3);
        text-transform: capitalize;
    }

    .status-dot {
        width: 10px;
        height: 10px;
        border-radius: 50%;
        background: var(--dot-color, var(--theme-textSecondary, #b3b3b3));
        flex-shrink: 0;
    }

    .vndb-link {
        background: none;
        border: none;
        padding: 0;
        font: inherit;
        padding-top:0.3rem;
        font-size: 0.9rem;
        font-weight: 600;
        cursor: pointer;
        color: var(--theme-primary, #36b7bd);
        transition: color 0.15s ease;
    }

    .vndb-link:hover {
        color: var(--theme-primaryHover, #17a4ab);
        text-decoration: underline;
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

    :global(body.mini-mode) .media-header {
        display: none !important;
    }
</style>