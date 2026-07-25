<script>
    import { getDb, pickCoverImage, coverSrc } from '$lib/db';
    import SelectInput from '$lib/components/SelectInput.svelte';
    import { STATUS_OPTIONS } from '$lib/constants.js';

    let { show = $bindable(false), media = null, onSaved = () => {} } = $props();

    let title = $state('');
    let tag = $state('');
    let status = $state('active');
    let color = $state('#89b4fa');
    let coverPath = $state(null);

    // Re-sync form fields whenever the modal is opened, for either
    // add (media = null) or edit (media = the row being edited).
    $effect(() => {
        if (show) {
            if (media) {
                title = media.title;
                tag = media.tag ?? '';
                status = media.status;
                color = media.color;
                coverPath = media.cover_path;
            } else {
                title = '';
                tag = '';
                status = 'active';
                color = '#89b4fa';
                coverPath = null;
            }
        }
    });

    function close() {
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

    function handleTagInput(e) {
        tag = e.target.value.replace(/^#+/, '');
    }

    async function save() {
        if (!title.trim()) return;
        const db = await getDb();

        if (media) {
            await db.execute(
                'UPDATE media SET title = $1, tag = $2, status = $3, color = $4, cover_path = $5, updated_at = unixepoch() WHERE id = $6',
                [title, tag || null, status, color, coverPath, media.id]
            );
        } else {
            await db.execute(
                'INSERT INTO media (title, tag, status, color, cover_path) VALUES ($1, $2, $3, $4, $5)',
                [title, tag || null, status, color, coverPath]
            );
        }

        show = false;
        onSaved();
    }
</script>

{#if show}
    <div class="modal-overlay" onclick={close}>
        <div class="modal add-media-modal" onclick={(e) => e.stopPropagation()}>
            <h3 class="modal-title">{media ? 'Edit media' : 'Add media'}</h3>

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

                    <div class="tag-input-group">
                        <span class="tag-prefix">#</span>
                        <input class="modal-input tag-input" placeholder="tag" value={tag} oninput={handleTagInput} />
                    </div>

                    <div class="status-color-row">
                        <div class="status-field">
                            <SelectInput options={STATUS_OPTIONS} value={status} on:change={(e) => (status = e.target.value)} />
                        </div>
                        <input class="color-input" type="color" bind:value={color} title="Card color" />
                    </div>
                </div>
            </div>

            <div class="modal-actions">
                <button class="modal-btn primary" onclick={save}>{media ? 'Save' : 'Add'}</button>
                <button class="modal-btn" onclick={close}>Cancel</button>
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

    .tag-input-group {
        position: relative;
        display: flex;
        align-items: center;
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

</style>