<script>
    import { lookupAtPosition } from '$lib/lookup.js';
    import { mineWordWithTags, updateWordStatus } from '$lib/dictionary.js';
    import { getDb } from '$lib/db';
    import { STATUS_LEVELS } from '$lib/constants.js';
    import SelectInput from '$lib/components/SelectInput.svelte';

    let { show = $bindable(false), onImported } = $props();

    let text = $state('');
    let status = $state(4); // Known
    let importing = $state(false);
    let done = $state(false);
    let added = $state(0);
    /** @type {Array<{ word: string, reason: string }>} */
    let failures = $state([]);

    const statusOptions = STATUS_LEVELS.map((s, i) => ({ value: String(i), label: s.label }));

    /** @param {Event} e */
    function handleStatusChange(e) {
        status = Number(/** @type {HTMLSelectElement} */ (e.target).value);
    }

    function close() {
        show = false;
    }

    $effect(() => {
        if (show) {
            text = '';
            importing = false;
            done = false;
            added = 0;
            failures = [];
        }
    });

    async function handleAdd() {
        const lines = text
            .split('\n')
            .map((l) => l.trim())
            .filter((l) => l.length > 0);
        if (lines.length === 0 || importing) return;
        importing = true;
        done = false;
        failures = [];
        added = 0;

        const db = await getDb();
        const existingRows = await db.select('SELECT id FROM words');
        const minedIds = new Set(existingRows.map((/** @type {{ id: number }} */ r) => r.id));
        const seenLines = new Set();
        const failed = [];
        let count = 0;

        for (const line of lines) {
            if (seenLines.has(line)) {
                failed.push({ word: line, reason: 'duplicate in list' });
                continue;
            }
            seenLines.add(line);

            let span = null;
            try {
                span = await lookupAtPosition(line, 0);
            } catch {
                span = null;
            }

            // Strict exact-line matching: the whole line must resolve to a
            // dictionary entry as-is — no partial matches, no deconjugation.
            const charLen = [...line].length;
            const exact =
                span &&
                (span.entries?.length ?? 0) > 0 &&
                !span.deconjugated_from &&
                span.start === 0 &&
                span.end === charLen;

            if (!exact) {
                failed.push({ word: line, reason: 'no exact dictionary entry found' });
                continue;
            }

            const entry = span.entries[0];
            if (minedIds.has(entry.id)) {
                failed.push({ word: line, reason: 'already in dictionary' });
                continue;
            }

            const wordId = await mineWordWithTags({
                dictId: entry.id,
                spelling: entry.spellings?.[0] ?? line,
                reading: entry.readings?.[0] ?? '',
                definitions: entry.definitions ?? [],
                wordType: (entry.pos ?? []).join(', '),
                mediaIds: [],
            });
            await updateWordStatus({ wordId, status });
            minedIds.add(entry.id);
            count += 1;
        }

        added = count;
        failures = failed;
        importing = false;
        done = true;
        onImported?.({ added: count, skipped: failed.length });
    }
</script>

{#if show}
    <div class="modal-overlay" onclick={close}>
        <div class="modal word-import-modal" onclick={(e) => e.stopPropagation()}>
            <h3 class="modal-title">Import words</h3>
            <p class="import-hint">
                One word per line. Only exact dictionary forms are added; conjugated
                forms and unknown words are skipped.
            </p>
            <textarea
                class="modal-input import-textarea"
                placeholder={'猫\n食べる\n勉強'}
                bind:value={text}
                rows={10}
                disabled={importing}
            ></textarea>
            <div class="import-status-row">
                <span class="import-status-label">Add with status</span>
                <SelectInput
                    options={statusOptions}
                    value={String(status)}
                    disabled={importing}
                    on:change={handleStatusChange}
                />
            </div>
            {#if done}
                <p class="import-summary">
                    Added {added} word{added === 1 ? '' : 's'}, skipped {failures.length}.
                </p>
                {#if failures.length > 0}
                    <details class="import-failures">
                        <summary>Skipped words ({failures.length})</summary>
                        <ul>
                            {#each failures as f (f.word + f.reason)}
                                <li><span class="import-fail-word">{f.word}</span> — {f.reason}</li>
                            {/each}
                        </ul>
                    </details>
                {/if}
            {/if}
            <div class="modal-actions">
                {#if done}
                    <button class="modal-btn primary" onclick={close}>Done</button>
                {:else}
                    <button
                        class="modal-btn primary"
                        onclick={handleAdd}
                        disabled={importing || text.trim().length === 0}
                    >
                        {importing ? 'Adding…' : 'Add'}
                    </button>
                    <button class="modal-btn" onclick={close} disabled={importing}>Cancel</button>
                {/if}
            </div>
        </div>
    </div>
{/if}

<style>
    .word-import-modal {
        width: 480px;
        max-width: min(480px, 92vw);
        align-items: stretch;
        text-align: left;
    }

    .word-import-modal .modal-title {
        text-align: center;
        margin: -0.6rem 0rem;
    }

    .import-hint {
        margin: 0;
        font-size: 0.85rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .import-textarea {
        width: 100%;
        min-height: 180px;
        resize: vertical;
        font-family: inherit;
        line-height: 1.6;
    }

    .import-status-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 1rem;
    }

    .import-status-label {
        font-size: 0.88rem;
        font-weight: 600;
        color: var(--theme-text, #f6f6f6);
    }

    .import-summary {
        margin: 0;
        font-size: 0.9rem;
        font-weight: 600;
        color: var(--theme-text, #f6f6f6);
    }

    .import-failures {
        border: 1px solid var(--theme-border, #404040);
        border-radius: 10px;
        padding: 0.6rem 0.8rem;
        font-size: 0.85rem;
        color: var(--theme-textSecondary, #b3b3b3);
        max-height: 180px;
        overflow-y: auto;
    }

    .import-failures summary {
        cursor: pointer;
        font-weight: 600;
        color: var(--theme-text, #f6f6f6);
    }

    .import-failures ul {
        margin: 0.5rem 0 0;
        padding-left: 1.2rem;
    }

    .import-failures li {
        margin: 0.15rem 0;
    }

    .import-fail-word {
        color: var(--theme-text, #f6f6f6);
        font-weight: 600;
    }
</style>
