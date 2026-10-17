<script>
    import { getDb, pickCoverImage } from '$lib/db';
    import AppImage from '$lib/components/AppImage.svelte';
    import { STATUS_OPTIONS, statusColor } from '$lib/constants.js';
    import { MEDIA_TYPES, mediaTypeOf, IMPORT_SOURCES } from '$lib/mediaTypes.js';
    import { ICONS } from '$lib/icons.js';
    import { searchVn, getVn, downloadCover, findExactVn, parseVndbId, preferredTitle } from '$lib/vndb.js';
    import { getCachedSettings } from '$lib/settings.js';

    let { show = $bindable(false), media = null, onSaved = () => {} } = $props();

    let title = $state('');
    let tag = $state('');
    let status = $state('active');
    let color = $state('#89b4fa');
    let mediaType = $state('visual_novel');
    let coverPath = $state(/** @type {string | null} */ (null));
    let vndbId = $state('');
    let excludeJiten = $state(false);
    let activeTab = $state(/** @type {'manual' | 'import'} */ ('manual'));
    let importQuery = $state('');
    // True once an import has filled the form: shows the confirmation
    // note and lets Save proceed from the import tab.
    let importResult = $state(false);
    let saving = $state(false);
    let importing = $state(false);
    let error = $state('');
    let dialogEl = $state(/** @type {HTMLDialogElement | null} */ (null));
    let titleInput = $state(/** @type {HTMLInputElement | null} */ (null));

    const typeDef = $derived(mediaTypeOf(mediaType));
    const importSources = $derived(
        typeDef.importers.map((/** @type {string} */ id) => IMPORT_SOURCES[id]).filter(Boolean)
    );

    // Re-sync form fields whenever the modal is opened, for either
    // add (media = null) or edit (media = the row being edited).
    $effect(() => {
        if (show) {
            error = '';
            saving = false;
            importing = false;
            activeTab = 'manual';
            importQuery = '';
            importResult = false;
            if (media) {
                title = media.title;
                tag = media.tag ?? '';
                status = media.status;
                color = media.color;
                mediaType = media.media_type ?? 'visual_novel';
                coverPath = media.cover_path;
                vndbId = media.vndb_id != null ? String(media.vndb_id) : '';
                excludeJiten = (media.jiten_excluded ?? 0) !== 0;
            } else {
                title = '';
                tag = '';
                status = 'active';
                color = '#89b4fa';
                mediaType = 'visual_novel';
                coverPath = null;
                vndbId = '';
                excludeJiten = false;
            }
        }
    });

    // Switching to a type with no import sources while the import tab is
    // open would strand it (the tab strip hides): fall back to manual.
    $effect(() => {
        if (importSources.length === 0 && activeTab === 'import') activeTab = 'manual';
    });

    // Native <dialog> + showModal() renders in the browser top layer, so
    // the backdrop always covers the viewport and the modal stays put when
    // the page scrolls. A plain position: fixed overlay breaks inside
    // transformed ancestors (home's page-scale transform).
    $effect(() => {
        if (!dialogEl) return;
        if (show && !dialogEl.open) dialogEl.showModal();
        else if (!show && dialogEl.open) dialogEl.close();
    });

    // Focus the title field on open so Enter does not re-trigger the
    // close button (showModal focuses the first focusable element).
    $effect(() => {
        if (show && activeTab === 'manual' && titleInput) titleInput.focus();
    });

    /** @param {Event} e */
    function handleDialogCancel(e) {
        if (saving) e.preventDefault();
    }

    function handleDialogClose() {
        show = false;
    }

    /** @param {MouseEvent} e */
    function handleDialogClick(e) {
        if (e.target === dialogEl) close();
    }

    function close() {
        if (saving) return;
        show = false;
    }

    async function handlePickCover() {
        try {
            const path = await pickCoverImage();
            if (path) coverPath = path;
        } catch (err) {
            console.error('cover pick failed:', err);
        }
    }

    /** @param {Event} e */
    function handleTagInput(e) {
        tag = /** @type {HTMLInputElement} */ (e.target).value.replace(/^#+/, '');
    }

    // Best-effort: if the local title matches a VNDB title exactly, grab its id.
    /** @param {string} name */
    async function detectVndbId(name) {
        try {
            const results = await searchVn(name);
            return findExactVn(results, name)?.id ?? null;
        } catch {
            return null;
        }
    }

    /** @param {number|string} id */
    async function fetchVnCover(id) {
        const vn = await getVn(String(id));
        if (vn && vn.image?.url) {
            coverPath = await downloadCover(vn.image.url);
        }
        return vn;
    }

    // Fill the form from the selected import source. Import never saves
    // directly: it populates the same fields the manual tab uses, so Save
    // stays a single path for both add and edit.
    async function runImport() {
        if (importing) return;
        const source = importSources[0];
        if (!source) return;
        const query = importQuery.trim();
        if (!query) {
            error = `Enter a ${source.label} ID or title`;
            return;
        }
        importing = true;
        error = '';
        try {
            // VNDB is the only import source for now; a second source
            // would branch on source.id here.
            let vn = null;
            const parsedId = parseVndbId(query);
            if (parsedId) {
                vn = await getVn(parsedId);
                if (!vn) {
                    error = `No VNDB entry found for ${parsedId}`;
                    return;
                }
            } else {
                const results = await searchVn(query);
                vn = findExactVn(results, query) ?? results[0] ?? null;
                if (!vn) {
                    error = `No VNDB entry found for "${query}"`;
                    return;
                }
            }

            title = preferredTitle(vn, getCachedSettings()?.vndb_title_pref ?? 'romaji');
            vndbId = String(parsedId ?? vn.id);
            mediaType = 'visual_novel';
            if (vn.image?.url) {
                try {
                    coverPath = await downloadCover(vn.image.url);
                } catch (err) {
                    console.error('VNDB cover download failed:', err);
                }
            }
            importResult = true;
        } catch (err) {
            console.error('import failed:', err);
            error = err instanceof Error ? err.message : 'Import failed';
        } finally {
            importing = false;
        }
    }

    async function save() {
        if (saving) return;
        saving = true;
        error = '';

        try {
            const db = await getDb();
            const finalTitle = title.trim();
            if (!finalTitle) {
                error = activeTab === 'import' && !importResult ? 'Import a title first' : 'Enter a title';
                return;
            }

            // Preserve a stored VNDB id on edit (stale ids on non-VN rows
            // are ignored by the rest of the app, but never deleted).
            let finalId = media?.vndb_id ?? null;
            const explicitId = String(vndbId ?? '').trim();
            if (mediaType === 'visual_novel') {
                if (explicitId) {
                    finalId = parseVndbId(explicitId);
                    if (!finalId) {
                        error = 'Invalid VNDB ID format';
                        return;
                    }
                } else {
                    finalId = await detectVndbId(finalTitle);
                }
            }

            // Fetch a cover for a typed VNDB id when none is set yet
            // (matches the old edit-mode behaviour; auto-detected ids and
            // manual adds do not pull a cover).
            let finalCover = coverPath;
            if (mediaType === 'visual_novel' && finalId && explicitId && !finalCover) {
                try {
                    await fetchVnCover(finalId);
                    finalCover = coverPath;
                } catch (err) {
                    console.error('VNDB cover fetch failed:', err);
                }
            }

            if (media) {
                await db.execute(
                    'UPDATE media SET title = $1, tag = $2, status = $3, color = $4, cover_path = $5, vndb_id = $6, jiten_excluded = $7, media_type = $8, updated_at = unixepoch() WHERE id = $9',
                    [finalTitle, tag || null, status, color, finalCover, finalId, excludeJiten ? 1 : 0, mediaType, media.id]
                );
            } else {
                await db.execute(
                    'INSERT INTO media (title, tag, status, color, cover_path, vndb_id, jiten_excluded, media_type) VALUES ($1, $2, $3, $4, $5, $6, $7, $8)',
                    [finalTitle, tag || null, status, color, finalCover, finalId, excludeJiten ? 1 : 0, mediaType]
                );
            }
            show = false;
            onSaved();
        } catch (err) {
            console.error('save failed:', err);
            error = err instanceof Error ? err.message : 'Failed to save media';
        } finally {
            saving = false;
        }
    }
</script>

<dialog
    class="media-dialog"
    bind:this={dialogEl}
    aria-labelledby="media-form-title"
    oncancel={handleDialogCancel}
    onclose={handleDialogClose}
    onclick={handleDialogClick}
>
        <div class="modal add-media-modal">
            <div class="modal-header">
                <span class="modal-header-icon" aria-hidden="true">{@html typeDef.icon}</span>
                <h3 class="modal-title" id="media-form-title">{media ? 'Edit' : 'Add'} {typeDef.label}</h3>
                <button class="modal-close" type="button" aria-label="Close" title="Close" onclick={close}>{@html ICONS.close}</button>
            </div>

            <div class="type-grid" role="radiogroup" aria-label="Media type">
                {#each MEDIA_TYPES as t (t.value)}
                    <button
                        type="button"
                        role="radio"
                        aria-checked={mediaType === t.value}
                        class="type-tile"
                        class:active={mediaType === t.value}
                        title={t.label}
                        onclick={() => (mediaType = t.value)}
                    >
                        <span class="type-tile-icon" aria-hidden="true">{@html t.icon}</span>
                        <span class="type-tile-label">{t.label}</span>
                    </button>
                {/each}
            </div>

            <div class="form-tabs">
                <button class="form-tab" type="button" class:active={activeTab === 'manual'} onclick={() => (activeTab = 'manual')}>Manual</button>
                {#if importSources.length > 0}
                    <button class="form-tab" type="button" class:active={activeTab === 'import'} onclick={() => (activeTab = 'import')}>Import</button>
                {/if}
            </div>

            <div class="modal-body">
                <button class="cover-picker" onclick={handlePickCover} title={coverPath ? 'Change cover' : 'Add cover'}>
                    {#if coverPath}
                        <AppImage path={coverPath} alt="cover preview" />
                        <span class="cover-overlay" aria-hidden="true">{@html ICONS.image}</span>
                    {:else}
                        <span class="cover-empty">
                            <span class="cover-empty-icon" aria-hidden="true">{@html ICONS.image}</span>
                            <span>Add cover</span>
                        </span>
                    {/if}
                </button>

                {#if activeTab === 'import'}
                    <div class="form-fields">
                        <div class="field">
                            <span class="field-label">Import from {importSources[0].label} <span class="req">*</span></span>
                            <div class="import-row">
                                <input
                                    class="modal-input"
                                    placeholder={importSources[0].placeholder}
                                    bind:value={importQuery}
                                    oninput={() => (importResult = false)}
                                    onkeydown={(e) => e.key === 'Enter' && runImport()}
                                />
                                <button class="modal-btn import-btn" onclick={runImport} disabled={importing}>
                                    {importing ? '…' : 'Import'}
                                </button>
                            </div>
                            <span class="form-hint">{importSources[0].hint}</span>
                        </div>
                        {#if importResult}
                            <div class="import-result">
                                <span class="import-thumb">
                                    {#if coverPath}
                                        <AppImage path={coverPath} alt="" />
                                    {:else}
                                        <span class="import-thumb-empty" aria-hidden="true">{@html ICONS.image}</span>
                                    {/if}
                                </span>
                                <span class="import-result-text">
                                    <span class="import-result-title">{title}</span>
                                    <span class="import-result-sub">Imported from {importSources[0].label}</span>
                                </span>
                                <span class="import-check" aria-hidden="true">{@html ICONS.check}</span>
                            </div>
                        {/if}
                    </div>
                {:else}
                    <div class="form-fields manual-fields">
                        <div class="field">
                            <label class="field-label" for="media-title">Title <span class="req">*</span></label>
                            <input id="media-title" class="modal-input" placeholder="Title" bind:value={title} bind:this={titleInput} />
                        </div>

                        <div class="field-row">
                            <div class="field">
                                <label class="field-label" for="media-tag">Tag <span class="req">*</span></label>
                                <div class="tag-color-row">
                                    <input class="color-input" type="color" bind:value={color} title="Tag / card color" />
                                    <div class="tag-input-group">
                                        <span class="tag-prefix">#</span>
                                        <input id="media-tag" class="modal-input tag-input" placeholder="tag" value={tag} oninput={handleTagInput} />
                                    </div>
                                </div>
                            </div>
                            {#if mediaType === 'visual_novel'}
                                <div class="field vndb-id-field">
                                    <label class="field-label" for="media-vndb">VNDB ID</label>
                                    <input id="media-vndb" class="modal-input" placeholder="e.g. 17" bind:value={vndbId} />
                                </div>
                            {/if}
                        </div>

                        <div class="field">
                            <span class="field-label">Status <span class="req">*</span></span>
                            <div class="status-chips">
                                {#each STATUS_OPTIONS as opt (opt.value)}
                                    <button
                                        type="button"
                                        class="status-chip"
                                        class:active={status === opt.value}
                                        style={`--chip: ${statusColor(opt.value)}`}
                                        onclick={() => (status = opt.value)}
                                    >
                                        <span class="status-chip-dot"></span>{opt.label}
                                    </button>
                                {/each}
                            </div>
                        </div>
                    </div>
                {/if}
            </div>

            {#if activeTab === 'manual'}
                <label class="exclude-row" class:on={excludeJiten}>
                    <span class="exclude-label">Exclude from Jiten</span>
                    <span class="switch">
                        <input type="checkbox" bind:checked={excludeJiten} />
                        <span class="switch-track"></span>
                    </span>
                </label>
            {/if}

            {#if error}
                <div class="modal-error">{error}</div>
            {/if}

            <div class="modal-actions">
                <button class="modal-btn" onclick={close} disabled={saving}>Cancel</button>
                <button class="modal-btn primary" onclick={save} disabled={saving}>{saving ? 'Saving…' : media ? 'Save' : 'Add'}</button>
            </div>
        </div>
</dialog>

<style>
    /* Native dialog shell: sits in the browser top layer, so it is immune
       to ancestor transforms and always covers the viewport. */
    .media-dialog {
        margin: auto;
        padding: 0;
        border: none;
        background: transparent;
        max-width: none;
        max-height: none;
        overflow: visible;
        color: inherit;
    }

    .media-dialog::backdrop {
        background: rgba(0, 0, 0, 0.55);
        -webkit-backdrop-filter: blur(6px);
        backdrop-filter: blur(6px);
    }

    .add-media-modal {
        width: 680px;
        max-width: min(680px, 94vw);
        max-height: min(90vh, 760px);
        overflow-y: auto;
        align-items: stretch;
        gap: 1.1rem;
        /* The modal renders inside page wrappers with text-align: center
           (.home); reset so labels and hints sit on the left. */
        text-align: left;
    }

    /* ── Header ─────────────────────────────────────────────────── */
    .modal-header {
        display: flex;
        align-items: center;
        gap: 0.7rem;
    }

    .modal-header-icon {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        flex-shrink: 0;
        width: 2.2rem;
        height: 2.2rem;
        border-radius: 11px;
        background: color-mix(in srgb, var(--theme-primary, #36b7bd) 18%, transparent);
        color: var(--theme-primary, #36b7bd);
        font-family: "Symbols Nerd Font";
        font-size: 1.1rem;
        line-height: 1;
    }

    .modal-header .modal-title {
        text-align: left;
        font-size: 1.15rem;
        min-width: 0;
        overflow: hidden;
        white-space: nowrap;
        text-overflow: ellipsis;
    }

    .modal-close {
        display: inline-flex;
        align-items: center;
        justify-content: center;
        flex-shrink: 0;
        margin-left: auto;
        width: 2rem;
        height: 2rem;
        border: none;
        border-radius: 50%;
        background: transparent;
        color: var(--theme-textSecondary, #b3b3b3);
        font-family: "Symbols Nerd Font";
        font-size: 0.85rem;
        line-height: 1;
        cursor: pointer;
        transition: background-color 0.15s ease, color 0.15s ease;
    }

    .modal-close:hover {
        background: color-mix(in srgb, var(--theme-textSecondary, #b3b3b3) 15%, transparent);
        color: var(--theme-text, #f6f6f6);
    }

    /* ── Type selector (segmented, icon + label inline) ─────────── */
    .type-grid {
        display: grid;
        grid-template-columns: repeat(5, minmax(0, 1fr));
        gap: 0.3rem;
        padding: 0.3rem;
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 70%, #000);
        border-radius: 12px;
    }

    .type-tile {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 0.45rem;
        padding: 0.55rem 0.4rem;
        border: 1px solid transparent;
        border-radius: 9px;
        background: transparent;
        color: var(--theme-textSecondary, #b3b3b3);
        font: inherit;
        font-size: 0.78rem;
        font-weight: 600;
        white-space: nowrap;
        cursor: pointer;
        transition:
            background-color 0.15s ease,
            color 0.15s ease;
    }

    .type-tile:hover:not(.active) {
        color: var(--theme-text, #f6f6f6);
        background: color-mix(in srgb, var(--theme-primary, #36b7bd) 12%, transparent);
    }

    .type-tile.active {
        background: var(--theme-primary, #36b7bd);
        color: #111;
    }

    .type-tile-icon {
        display: inline-flex;
        flex-shrink: 0;
        font-family: "Symbols Nerd Font";
        font-size: 0.95rem;
        line-height: 1;
    }

    .type-tile-label {
        min-width: 0;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    /* ── Manual / Import tabs (underline) ───────────────────────── */
    .form-tabs {
        display: flex;
        gap: 0.35rem;
        width: 100%;
        padding: 0;
        background: none;
        border-radius: 0;
        border-bottom: 1px solid color-mix(in srgb, var(--theme-border, #404040) 70%, transparent);
    }

    .form-tab {
        flex: none;
        margin-bottom: -1px;
        padding: 0.4rem 0.9rem;
        border: none;
        border-bottom: 2px solid transparent;
        border-radius: 0;
        background: none;
        color: var(--theme-textSecondary, #b3b3b3);
        font: inherit;
        font-size: 0.85rem;
        font-weight: 600;
        cursor: pointer;
        transition: color 0.15s ease, border-color 0.15s ease;
    }

    .form-tab:hover:not(.active) {
        color: var(--theme-text, #f6f6f6);
    }

    .form-tab.active {
        color: var(--theme-primary, #36b7bd);
        border-bottom-color: var(--theme-primary, #36b7bd);
    }

    /* ── Body ───────────────────────────────────────────────────── */
    .modal-body {
        display: flex;
        gap: 1.3rem;
        /* Stretch the field column to the cover's height. */
        align-items: stretch;
    }

    .cover-picker {
        position: relative;
        flex-shrink: 0;
        /* Keep the 2:3 aspect: stretch would squash it to the shorter
           form column (e.g. the single import field). */
        align-self: flex-start;
        aspect-ratio: 2 / 3;
        width: 150px;
        border: 2px dashed var(--theme-border, #404040);
        border-radius: 12px;
        background: none;
        display: flex;
        align-items: center;
        justify-content: center;
        cursor: pointer;
        overflow: hidden;
        color: var(--theme-textSecondary, #b3b3b3);
        font-size: 0.8rem;
        transition: border-color 0.15s ease;
    }

    .cover-picker:hover {
        border-color: var(--theme-primary, #36b7bd);
        color: var(--theme-primary, #36b7bd);
    }

    .cover-picker :global(img) {
        width: 100%;
        height: 100%;
        object-fit: cover;
    }

    .cover-empty {
        display: flex;
        flex-direction: column;
        align-items: center;
        gap: 0.35rem;
    }

    .cover-empty-icon {
        font-family: "Symbols Nerd Font";
        font-size: 1.4rem;
        line-height: 1;
        opacity: 0.75;
    }

    /* Replaces the cover on hover with a "change" affordance. */
    .cover-overlay {
        position: absolute;
        inset: 0;
        display: flex;
        align-items: center;
        justify-content: center;
        background: rgba(0, 0, 0, 0.5);
        color: #fff;
        font-family: "Symbols Nerd Font";
        font-size: 1.3rem;
        opacity: 0;
        transition: opacity 0.15s ease;
    }

    .cover-picker:hover .cover-overlay,
    .cover-picker:focus-visible .cover-overlay {
        opacity: 1;
    }

    .form-fields {
        flex: 1;
        display: flex;
        flex-direction: column;
        gap: 1.05rem;
        min-width: 0;
    }

    /* Title / Tag / Status spread to end exactly at the cover's bottom. */
    .manual-fields {
        justify-content: space-between;
    }

    .field {
        display: flex;
        flex-direction: column;
        gap: 0.4rem;
        min-width: 0;
    }

    /* Tag + VNDB ID sit side by side. */
    .field-row {
        display: flex;
        gap: 0.8rem;
        align-items: flex-start;
    }

    .field-row > .field {
        flex: 1;
    }

    .field-row > .vndb-id-field {
        flex: 0 0 38%;
        max-width: 38%;
    }

    /* Required marker next to the field label. */
    .field-label .req {
        color: #ffb86c;
        font-weight: 700;
    }

    .field-label {
        padding-left: 0.15rem;
        font-size: 0.68rem;
        font-weight: 700;
        letter-spacing: 0.06em;
        text-transform: uppercase;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .form-fields .modal-input {
        width: 100%;
        text-align: left;
        font-size: 0.9rem;
        padding-top: 0.55rem;
        padding-bottom: 0.55rem;
    }

    .form-hint {
        font-size: 0.78rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .tag-input-group {
        position: relative;
        display: flex;
        align-items: center;
        flex: 1;
        min-width: 0;
    }

    .tag-color-row {
        display: flex;
        align-items: center;
        gap: 0.6rem;
    }

    .tag-prefix {
        position: absolute;
        left: 0.9rem;
        color: var(--theme-textSecondary, #b3b3b3);
        font-weight: 600;
        pointer-events: none;
    }

    .tag-input {
        padding-left: 1.7rem;
    }

    /* ── Import panel ───────────────────────────────────────────── */
    .import-row {
        display: flex;
        gap: 0.5rem;
        align-items: stretch;
    }

    .import-row .modal-input {
        flex: 1;
        min-width: 0;
    }

    .import-btn {
        flex: none;
        padding: 0 1.2rem;
    }

    .import-result {
        display: flex;
        align-items: center;
        gap: 0.7rem;
        padding: 0.55rem 0.7rem;
        border: 1.5px solid color-mix(in srgb, #a6e3a1 45%, transparent);
        border-radius: 10px;
        background: color-mix(in srgb, #a6e3a1 8%, transparent);
    }

    .import-thumb {
        display: flex;
        align-items: center;
        justify-content: center;
        flex-shrink: 0;
        width: 34px;
        aspect-ratio: 2 / 3;
        border-radius: 5px;
        overflow: hidden;
        background: rgba(0, 0, 0, 0.3);
        color: var(--theme-textSecondary, #b3b3b3);
        font-family: "Symbols Nerd Font";
    }

    .import-thumb :global(img) {
        width: 100%;
        height: 100%;
        object-fit: cover;
    }

    .import-result-text {
        flex: 1;
        display: flex;
        flex-direction: column;
        gap: 0.1rem;
        min-width: 0;
    }

    .import-result-title {
        font-size: 0.85rem;
        font-weight: 600;
        color: var(--theme-text, #f6f6f6);
        overflow: hidden;
        white-space: nowrap;
        text-overflow: ellipsis;
    }

    .import-result-sub {
        font-size: 0.72rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .import-check {
        display: inline-flex;
        flex-shrink: 0;
        font-family: "Symbols Nerd Font";
        font-size: 0.95rem;
        color: #a6e3a1;
    }

    /* ── Status chips ───────────────────────────────────────────── */
    .status-chips {
        display: flex;
        flex-wrap: wrap;
        gap: 0.35rem;
    }

    .status-chip {
        display: inline-flex;
        align-items: center;
        gap: 0.4rem;
        padding: 0.32rem 0.62rem;
        border: 1.5px solid var(--theme-border, #404040);
        border-radius: 999px;
        background: transparent;
        color: var(--theme-textSecondary, #b3b3b3);
        font: inherit;
        font-size: 0.78rem;
        font-weight: 600;
        cursor: pointer;
        transition: all 0.15s ease;
    }

    .status-chip:hover {
        border-color: var(--chip);
        color: var(--theme-text, #f6f6f6);
    }

    .status-chip.active {
        border-color: var(--chip);
        background: color-mix(in srgb, var(--chip) 20%, transparent);
        color: var(--theme-text, #f6f6f6);
    }

    .status-chip-dot {
        flex-shrink: 0;
        width: 0.5rem;
        height: 0.5rem;
        border-radius: 50%;
        background: var(--chip);
    }

    .exclude-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 0.6rem;
        padding: 0.5rem 0.7rem;
        border: 1.5px solid var(--theme-border, #404040);
        border-radius: 10px;
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 55%, transparent);
        font-size: 0.82rem;
        font-weight: 600;
        color: var(--theme-text, #f6f6f6);
        cursor: pointer;
        transition: border-color 0.15s ease, background 0.15s ease;
    }

    .exclude-row:hover {
        border-color: var(--theme-textSecondary, #b3b3b3);
    }

    .exclude-row.on {
        border-color: var(--theme-primary, #36b7bd);
        background: color-mix(in srgb, var(--theme-primary, #36b7bd) 12%, transparent);
    }

    .exclude-label {
        min-width: 0;
        overflow: hidden;
        white-space: nowrap;
        text-overflow: ellipsis;
    }

    /* ── Toggle switch (matches settings) ───────────────────────── */
    .switch {
        position: relative;
        display: inline-block;
        flex-shrink: 0;
        width: 38px;
        height: 22px;
    }

    .switch input {
        opacity: 0;
        width: 0;
        height: 0;
    }

    .switch-track {
        position: absolute;
        inset: 0;
        background: var(--theme-border, #404040);
        border-radius: 100px;
        cursor: pointer;
        transition: background 0.2s ease;
    }

    .switch-track::before {
        content: "";
        position: absolute;
        width: 16px;
        height: 16px;
        left: 3px;
        top: 3px;
        background: #fff;
        border-radius: 50%;
        transition: transform 0.2s ease;
    }

    .switch input:checked + .switch-track {
        background: var(--theme-primary, #36b7bd);
    }

    .switch input:checked + .switch-track::before {
        transform: translateX(16px);
    }

    .switch input:focus-visible + .switch-track {
        outline: 2px solid var(--theme-primary, #36b7bd);
        outline-offset: 2px;
    }

    .color-input {
        display: border-box;
        flex-shrink: 0;
        width: 30px;
        height: 30px;
        border: 2px solid var(--theme-border, #404040);
        border-radius: 50%;
        background: var(--theme-surface, #2d2d2d);
        box-shadow: inset 0 0 0 1px rgba(0, 0, 0, 0.25);
        cursor: pointer;
        padding: 2px;
        overflow: hidden;
        -webkit-appearance: none;
        appearance: none;
        transition: border-color 0.15s ease;
    }

    .color-input:hover {
        border-color: var(--theme-primary, #36b7bd);
    }

    .color-input::-webkit-color-swatch-wrapper {
        padding: 0;
    }

    .color-input::-webkit-color-swatch {
        border: none;
        border-radius: 50%;
    }

    .color-input::-moz-color-swatch {
        border: none;
        border-radius: 50%;
    }

    /* ── Footer ─────────────────────────────────────────────────── */
    .add-media-modal .modal-actions {
        justify-content: flex-end;
    }

    .add-media-modal .modal-btn {
        flex: none;
        min-width: 96px;
    }

    .modal-error {
        font-size: 0.82rem;
        color: #f38ba8;
        padding: 0.4rem 0.2rem 0;
    }
</style>
