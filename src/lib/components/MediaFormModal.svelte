<script>
    import { getDb, pickCoverImage, coverSrc } from '$lib/db';
    import SelectInput from '$lib/components/SelectInput.svelte';
    import { STATUS_OPTIONS } from '$lib/constants.js';
    import { searchVn, getVn, downloadCover, findExactVn, parseVndbId, preferredTitle } from '$lib/vndb.js';
    import { getCachedSettings } from '$lib/settings.js';

    let { show = $bindable(false), media = null, onSaved = () => {} } = $props();

    let title = $state('');
    let tag = $state('');
    let status = $state('active');
    let color = $state('#89b4fa');
    let coverPath = $state(/** @type {string | null} */ (null));
    let vndbId = $state('');
    let activeTab = $state('manual');
    let saving = $state(false);
    let error = $state('');

    // Re-sync form fields whenever the modal is opened, for either
    // add (media = null) or edit (media = the row being edited).
    $effect(() => {
        if (show) {
            error = '';
            saving = false;
            activeTab = 'manual';
            if (media) {
                title = media.title;
                tag = media.tag ?? '';
                status = media.status;
                color = media.color;
                coverPath = media.cover_path;
                vndbId = media.vndb_id ?? '';
            } else {
                title = '';
                tag = '';
                status = 'active';
                color = '#89b4fa';
                coverPath = null;
                vndbId = '';
            }
        }
    });

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
        tag = e.target.value.replace(/^#+/, '');
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

    /** @param {string} id */
    async function fetchVnCover(id) {
        const vn = await getVn(id);
        if (vn && vn.image?.url) {
            coverPath = await downloadCover(vn.image.url);
        }
        return vn;
    }

    async function save() {
        if (saving) return;
        const db = await getDb();
        saving = true;
        error = '';

        try {
            if (media) {
                // Edit mode: single form, no tabs. Pull the cover from VNDB
                // when an id is provided and there is still no local cover.
                let finalCover = coverPath;
                let finalId = vndbId.trim() ? parseVndbId(vndbId) : null;
                if (vndbId.trim() && !finalId) {
                    error = 'Invalid VNDB ID format';
                    return;
                }
                if (finalId && !finalCover) {
                    try {
                        await fetchVnCover(finalId);
                        finalCover = coverPath;
                    } catch (err) {
                        console.error('VNDB cover fetch failed:', err);
                    }
                }
                if (!finalId && title.trim()) {
                    finalId = await detectVndbId(title);
                }

                await db.execute(
                    'UPDATE media SET title = $1, tag = $2, status = $3, color = $4, cover_path = $5, vndb_id = $6, updated_at = unixepoch() WHERE id = $7',
                    [title.trim(), tag || null, status, color, finalCover, finalId, media.id]
                );
                show = false;
                onSaved();
                return;
            }

            // Add mode.
            let finalTitle = title;
            let finalCover = coverPath;
            let finalId = null;

            if (activeTab === 'vndb') {
                const query = vndbId.trim();
                if (!query) {
                    error = 'Enter a VNDB ID or title';
                    return;
                }

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

                finalTitle = preferredTitle(vn, getCachedSettings()?.vndb_title_pref ?? 'romaji');
                if (vn.image?.url) finalCover = await downloadCover(vn.image.url);
                finalId = parsedId ?? vn.id;
            } else {
                if (!finalTitle.trim()) {
                    error = 'Enter a title';
                    return;
                }
                finalId = await detectVndbId(finalTitle);
            }

            await db.execute(
                'INSERT INTO media (title, tag, status, color, cover_path, vndb_id) VALUES ($1, $2, $3, $4, $5, $6)',
                [finalTitle.trim(), tag || null, status, color, finalCover, finalId]
            );
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

{#if show}
    <div class="modal-overlay" onclick={close}>
        <div class="modal add-media-modal" onclick={(e) => e.stopPropagation()}>
            <h3 class="modal-title">{media ? 'Edit media' : 'Add media'}</h3>

            {#if !media}
                <div class="modal-tabs">
                    <button class="tab-btn" class:active={activeTab === 'manual'} onclick={() => (activeTab = 'manual')}>Manual</button>
                    <button class="tab-btn" class:active={activeTab === 'vndb'} onclick={() => (activeTab = 'vndb')}>VNDB</button>
                </div>
            {/if}

            {#if !media && activeTab === 'vndb'}
                <div class="modal-body">
                    <div class="form-fields">
                        <input class="modal-input" placeholder="VNDB ID or name" bind:value={vndbId} />

                        <div class="tag-color-row">
                            <div class="tag-input-group">
                                <span class="tag-prefix">#</span>
                                <input class="modal-input tag-input" placeholder="tag" value={tag} oninput={handleTagInput} />
                            </div>
                            <input class="color-input" type="color" bind:value={color} title="Tag / card color" />
                        </div>

                        <div class="status-color-row">
                            <div class="status-field">
                                <SelectInput options={STATUS_OPTIONS} value={status} on:change={(/** @type {Event} */ e) => (status = /** @type {HTMLSelectElement} */ (e.target).value)} />
                            </div>
                        </div>

                        <span class="form-hint">Title and cover are fetched from VNDB automatically.</span>
                    </div>
                </div>
            {:else}
                <div class="modal-body">
                    <button class="cover-picker" onclick={handlePickCover}>
                        {#if coverPath}
                            <img src={coverSrc(coverPath)} alt="cover preview" />
                        {:else}
                            <span>+ Cover</span>
                        {/if}
                    </button>

                    <div class="form-fields">
                        <input class="modal-input" placeholder="Title" bind:value={title} />

                        <div class="tag-color-row">
                            <div class="tag-input-group">
                                <span class="tag-prefix">#</span>
                                <input class="modal-input tag-input" placeholder="tag" value={tag} oninput={handleTagInput} />
                            </div>
                            <input class="color-input" type="color" bind:value={color} title="Tag / card color" />
                        </div>

                        {#if media}
                            <input class="modal-input" placeholder="VNDB ID (e.g. 17 or v17)" bind:value={vndbId} />
                        {/if}

                        <div class="status-color-row">
                            <div class="status-field">
                                <SelectInput options={STATUS_OPTIONS} value={status} on:change={(/** @type {Event} */ e) => (status = /** @type {HTMLSelectElement} */ (e.target).value)} />
                            </div>
                        </div>
                    </div>
                </div>
            {/if}

            {#if error}
                <div class="modal-error">{error}</div>
            {/if}

            <div class="modal-actions">
                <button class="modal-btn primary" onclick={save} disabled={saving}>{saving ? 'Saving…' : media ? 'Save' : 'Add'}</button>
                <button class="modal-btn" onclick={close} disabled={saving}>Cancel</button>
            </div>
        </div>
    </div>
{/if}

<style>
    .add-media-modal {
        width: 400px;
        max-width: min(400px, 90vw);
        align-items: stretch;
    }

    .modal-tabs {
        display: flex;
        gap: 0.35rem;
        padding: 0.2rem;
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 70%, #000);
        border-radius: 10px;
        width: 100%;
    }

    .tab-btn {
        flex: 1;
        padding: 0.45rem 0;
        border: none;
        border-radius: 8px;
        background: none;
        color: var(--theme-textSecondary, #b3b3b3);
        font: inherit;
        font-size: 0.85rem;
        font-weight: 600;
        cursor: pointer;
        transition: background 0.15s ease, color 0.15s ease;
    }

    .tab-btn.active {
        background: var(--theme-primary, #36b7bd);
        color: #111;
    }

    .modal-body {
        display: flex;
        gap: 1.25rem;
        align-items: flex-start;
    }

    .cover-picker {
        flex-shrink: 0;
        aspect-ratio: 2 / 3;
        width: 110px;
        border: 2px dashed var(--theme-border, #404040);
        border-radius: 12px;
        background: none;
        display: flex;
        align-items: center;
        justify-content: center;
        cursor: pointer;
        overflow: hidden;
        color: var(--theme-textSecondary, #b3b3b3);
        font-size: 0.85rem;
        transition: border-color 0.15s ease;
    }

    .cover-picker:hover {
        border-color: var(--theme-primary, #36b7bd);
    }

    .cover-picker img {
        width: 100%;
        height: 100%;
        object-fit: cover;
    }

    .form-fields {
        flex: 1;
        display: flex;
        flex-direction: column;
        gap: 0.75rem;
        min-width: 0;
    }

    .form-fields .modal-input {
        width: 100%;
        text-align: left;
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

    .status-color-row {
        display: flex;
        justify-content: flex-start;
        gap: 0.6rem;
        align-items: center;
    }

    .status-field {
        flex: none;
        min-width: 110px;
    }

    .color-input {
        display: border-box;
        flex-shrink: 0;
        width: 40px;
        height: 43px;
        border: 0px solid var(--theme-border, #404040);
        border-radius: 15px;
        background: none;
        cursor: pointer;
        padding: 2px;
    }

    .modal-error {
        font-size: 0.82rem;
        color: #f38ba8;
        padding: 0.4rem 0.2rem 0;
    }
</style>
