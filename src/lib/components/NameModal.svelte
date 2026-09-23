<script>
    import { saveName } from '$lib/names';

    let { show = $bindable(false), mediaId = null, name = '', onSaved } = $props();

    let reading = $state('');
    let saving = $state(false);
    let error = $state(/** @type {string | null} */ (null));

    function close() {
        show = false;
        reading = '';
        saving = false;
        error = null;
    }

    async function handleSave() {
        if (!name.trim() || saving) return;
        saving = true;
        error = null;
        try {
            await saveName({ mediaId, name, reading });
            onSaved?.();
            close();
        } catch (e) {
            console.error('save name failed:', e);
            error = 'Could not save name.';
            saving = false;
        }
    }
</script>

{#if show}
    <div class="modal-overlay" onclick={close}>
        <div class="modal name-modal" onclick={(e) => e.stopPropagation()}>
            <h3 class="modal-title">Save name</h3>
            <div class="name-row">
                <div class="name-surface">{name}</div>
                <textarea
                    class="modal-input name-textarea"
                    placeholder="How is it read?"
                    bind:value={reading}
                    rows={1}
                    disabled={saving}
                ></textarea>
            </div>
            {#if error}
                <p class="name-error">{error}</p>
            {/if}
            <div class="modal-actions">
                <button
                    class="modal-btn primary"
                    onclick={handleSave}
                    disabled={saving || name.trim().length === 0}
                >
                    {saving ? 'Saving…' : 'Save'}
                </button>
                <button class="modal-btn" onclick={close} disabled={saving}>Cancel</button>
            </div>
        </div>
    </div>
{/if}

<style>
    .name-modal {
        width: 380px;
        max-width: min(380px, 90vw);
    }

    .name-row {
        display: flex;
        align-items: center;
        gap: 0.75rem;
        margin-bottom: 0.75rem;
    }

    .name-surface {
        font-family: "Noto Sans JP", Inter, sans-serif;
        font-size: 1.25rem;
        font-weight: 700;
        color: var(--theme-text, #f6f6f6);
        overflow-wrap: anywhere;
        flex-shrink: 0;
        max-width: 45%;
    }

    .name-textarea {
        flex: 1;
        min-width: 0;
        resize: none;
        min-height: 0;
        font-size: 0.9rem;
        font-family: "Noto Sans JP", Inter, sans-serif;
    }

    .name-error {
        margin: 0.5rem 0 0;
        font-size: 0.85rem;
        color: #f38ba8;
    }
</style>
