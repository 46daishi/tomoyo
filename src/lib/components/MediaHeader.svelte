<script>
    import AppImage from '$lib/components/AppImage.svelte';
    import { STATUS_COLORS } from '$lib/constants.js';
    import { openUrl } from '@tauri-apps/plugin-opener';
    import { vndbUrl, getVnDescription } from '$lib/vndb.js';
    import { stripVndbMarkup, saveSynopsis } from '$lib/synopsis.js';

    let { media, children, titleActions } = $props();

    let editing = $state(false);
    let draft = $state('');
    let saving = $state(false);
    let draftEl = $state(/** @type {HTMLDivElement | null} */ (null));

    /** @param {string} id */
    async function openVndb(id) {
        const url = vndbUrl(id);
        if (url) await openUrl(url);
    }

    // Auto-populate the synopsis from VNDB once per page open when the entry
    // has none yet. The fetched text is saved to the DB right away so it is
    // editable (and persists) like a hand-written synopsis afterwards.
    let fetchedFor = /** @type {number | null} */ (null);
    $effect(() => {
        const entry = media;
        if (!entry?.vndb_id) return;
        if ((entry.media_type ?? 'visual_novel') !== 'visual_novel') return;
        if (entry.synopsis?.trim()) return;
        if (fetchedFor === entry.id) return;
        fetchedFor = entry.id;
        (async () => {
            try {
                const description = stripVndbMarkup(await getVnDescription(entry.vndb_id));
                if (!description) return;
                await saveSynopsis(entry.id, description);
                if (media.id === entry.id) media.synopsis = description;
            } catch (err) {
                console.error('VNDB synopsis fetch failed:', err);
            }
        })();
    });

    // Line breaks are stored as <br> separators, not raw "\n" characters:
    // WebKit collapses literal newlines in text nodes once the editable host
    // starts editing.
    /** @param {string} text */
    function setEditorText(text) {
        if (!draftEl) return;
        draftEl.replaceChildren();
        if (!text) return;
        const fragment = document.createDocumentFragment();
        text.split('\n').forEach((line, i) => {
            if (i > 0) fragment.appendChild(document.createElement('br'));
            if (line) fragment.appendChild(document.createTextNode(line));
        });
        draftEl.appendChild(fragment);
    }

    // Mirror the stored synopsis into the editor whenever it isn't focused
    // (initial render, VNDB auto-fetch, saves from elsewhere).
    $effect(() => {
        const el = draftEl;
        const text = media.synopsis ?? '';
        if (!el || document.activeElement === el) return;
        setEditorText(text);
        draft = text;
    });

    function beginEdit() {
        editing = true;
    }

    async function commitSynopsis() {
        editing = false;
        if (saving || !draftEl) return;
        const current = media.synopsis ?? '';
        const text = draftEl.innerText.replace(/\n+$/, '');
        draft = text;
        if (text.trim() === current.trim()) return;
        saving = true;
        try {
            await saveSynopsis(media.id, text);
            const saved = text.trim();
            media.synopsis = saved || null;
            draft = saved;
        } catch (err) {
            console.error('Failed to save synopsis:', err);
            draft = current;
            setEditorText(current);
        } finally {
            saving = false;
        }
    }

    function onInput() {
        draft = draftEl?.innerText ?? '';
    }

    /** @param {KeyboardEvent} e */
    function onDraftKeydown(e) {
        if (e.key === 'Escape') {
            e.preventDefault();
            const text = media.synopsis ?? '';
            draft = text;
            setEditorText(text);
            draftEl?.blur();
        } else if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
            e.preventDefault();
            draftEl?.blur();
        }
    }

    /** @param {ClipboardEvent} e */
    function onPaste(e) {
        e.preventDefault();
        document.execCommand('insertText', false, e.clipboardData?.getData('text/plain') ?? '');
    }
</script>

<div class="media-header" style={`--page-glow: ${media.color}`}>
    <div class="cover">
        {#if media.cover_path}
            <AppImage path={media.cover_path} alt={media.title} />
        {:else}
            <div class="cover-placeholder"></div>
        {/if}
    </div>

    <div class="title-block">
        <div class="title-row">
            <h1 class="media-title">{media.title}</h1>
            {#if media.tag}
                <span class="tag-pill" style="--tag-color: {media.color}">#{media.tag}</span>
            {/if}
            {#if titleActions}
                {@render titleActions()}
            {/if}
        </div>

        <div class="media-meta">
            <span class="status" style="--dot-color: {STATUS_COLORS[media.status]}">
                <span class="status-dot"></span>
                {media.status}
            </span>
            {#if media.vndb_id && (media.media_type ?? 'visual_novel') === 'visual_novel'}
                <button class="vndb-link" onclick={() => openVndb(media.vndb_id)} title="Open on VNDB">v{media.vndb_id.replace(/^v/i, '')}</button>
            {/if}
        </div>

        <div
            class="synopsis-text"
            class:editing
            class:empty={!draft.trim()}
            contenteditable="plaintext-only"
            role="textbox"
            aria-multiline="true"
            aria-label="Synopsis"
            tabindex="0"
            data-placeholder="No synopsis yet - click to write one"
            bind:this={draftEl}
            onfocus={beginEdit}
            onblur={commitSynopsis}
            oninput={onInput}
            onkeydown={onDraftKeydown}
            onpaste={onPaste}
        ></div>
    </div>

    {#if children}
        <div class="media-stats-slot">
            {@render children()}
        </div>
    {/if}
</div>

<style>
    .media-header {
        container-type: inline-size;
        display: grid;
        grid-template-columns: auto 1fr;
        grid-template-areas:
            "cover title"
            "stats stats";
        grid-template-rows: auto 1fr;
        column-gap: 1.5rem;
        row-gap: 0.75rem;
        width: 100%;
        max-width: 900px;
        margin-top: 1rem;
    }

    .cover {
        grid-area: cover;
        align-self: start;
        width: clamp(96px, 20cqi, 180px);
        aspect-ratio: 153 / 230;
        border-radius: 10px;
        overflow: hidden;
        background: var(--surface1, #313244);
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
        background: linear-gradient(135deg, var(--surface1, #313244), var(--surface0, #1e1e2e));
    }

    .title-block {
        grid-area: title;
        min-width: 0;
        display: flex;
        flex-direction: column;
        align-items: flex-start;
        text-align: left;
        gap: 0.3rem;
        padding-top: 0.2rem;
    }

    .media-stats-slot {
        grid-area: stats;
        width: 100%;
        margin-top: 0;
    }

    .title-row {
        font-family: "Noto Sans JP";
        display: flex;
        align-items: center;
        gap: 0.7rem;
        flex-wrap: wrap;
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
        font-weight: 600;
        color: var(--dot-color, var(--theme-textSecondary, #b3b3b3));
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
        padding-top: 0.3rem;
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

    /* Synopsis: click straight into the text to edit; no separate edit mode.
       Idle look is unchanged (clamped to 10 lines, dimmed until hovered). */
    .synopsis-text {
        display: -webkit-box;
        -webkit-box-orient: vertical;
        -webkit-line-clamp: 10;
        line-clamp: 10;
        overflow: hidden;
        width: 100%;
        min-width: 0;
        margin: 0;
        padding: 0;
        border: none;
        background: none;
        font: inherit;
        font-size: 0.85rem;
        line-height: 1.45;
        white-space: pre-line;
        text-align: left;
        cursor: text;
        color: color-mix(in srgb, var(--theme-text, #f4f4f4) 80%, transparent);
        transition: color 0.15s ease;
    }

    .synopsis-text:hover,
    .synopsis-text:focus {
        color: var(--theme-text, #f4f4f4);
    }

    .synopsis-text:focus {
        outline: none;
    }

    .synopsis-text.editing {
        display: block;
        overflow: visible;
    }

    .synopsis-text.empty::before {
        content: attr(data-placeholder);
        font-style: italic;
        color: color-mix(in srgb, var(--theme-text, #f4f4f4) 40%, transparent);
    }

    .synopsis-text.empty:hover::before,
    .synopsis-text.empty:focus::before {
        color: color-mix(in srgb, var(--theme-text, #f4f4f4) 60%, transparent);
    }

    :global(body.mini-mode) .media-header {
        display: none !important;
    }

    @container (max-width: 480px) {
        .media-header {
            grid-template-areas:
                "cover title"
                "stats stats";
            grid-template-rows: auto auto;
        }
        .title-block {
            align-self: center;
        }
        .synopsis-text {
            -webkit-line-clamp: 4;
            line-clamp: 4;
        }
    }

    @container (min-width: 481px) and (max-width: 640px) {
        .media-stats-slot :global(.media-stats) {
            grid-template-columns: repeat(2, 1fr);
        }
    }</style>