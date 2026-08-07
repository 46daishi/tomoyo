<script>
    import { goto } from '$app/navigation';
    import { STATUS_LEVELS } from '$lib/constants.js';
    import MultiSelectInput from '$lib/components/MultiSelectInput.svelte';

    let { show = $bindable(false), mediaFilter = null } = $props();

    let reviewKind = $state('word'); // 'word' | 'sentence'
    let count = $state(20);
    let selectedStatuses = $state([0, 1, 2, 3]);
    let onlyTranslated = $state(false);

    function close() {
        show = false;
    }

    function startCustomReview() {
        const params = new URLSearchParams();
        if (mediaFilter) params.set('media', mediaFilter);
        params.set('count', String(count));

        if (reviewKind === 'word') {
            params.set('statuses', selectedStatuses.join(','));
        } else {
            params.set('onlyTranslated', onlyTranslated ? '1' : '0');
        }

        const url = new URL(window.location.href);
        url.searchParams.set('tab', 'review');
        history.replaceState(history.state, '', url);

        goto(`/review/${reviewKind}?${params.toString()}`);
    }

    let sliderFillPct = $derived(((count - 5) / (100 - 5)) * 100);
</script>

{#if show}
    <div class="modal-overlay" onclick={close}>
        <div class="modal custom-review-modal" onclick={(e) => e.stopPropagation()}>
            <h3 class="modal-title">Custom Review</h3>

            <div class="setting-row">
                <div class="review-kind-toggle">
                    <button
                        type="button"
                        class:active={reviewKind === 'word'}
                        onclick={() => (reviewKind = 'word')}
                    >
                        Words
                    </button>
                    <button
                        type="button"
                        class:active={reviewKind === 'sentence'}
                        onclick={() => (reviewKind = 'sentence')}
                    >
                        Sentences
                    </button>
                </div>
            </div>

            <div class="setting-row count-row">
                <div class="count-row-header">
                    <span class="count-row-label">Number of {reviewKind === 'word' ? 'words' : 'sentences'}</span>
                    <span class="count-badge">{count}<span class="count-badge-unit">{reviewKind === 'word' ? 'words' : 'sentences'}</span></span>
                </div>
                <input
                    type="range"
                    class="count-slider"
                    min="5"
                    max="100"
                    step="5"
                    bind:value={count}
                    style={`--slider-fill: ${sliderFillPct}%`}
                />
            </div>

            {#if reviewKind === 'word'}
                <div class="setting-row">
                    <MultiSelectInput
                        options={STATUS_LEVELS.map((level, i) => ({ value: i, label: level.label, color: level.color }))}
                        values={selectedStatuses}
                        onChange={(next) => (selectedStatuses = next)}
                    />
                </div>
            {:else}
                <div class="setting-row translated-toggle-row">
                    <label class="toggle-option">
                        <span class="toggle-option-text">Only translated sentences</span>
                        <span class="switch">
                            <input type="checkbox" bind:checked={onlyTranslated} />
                            <span class="switch-track"></span>
                        </span>
                    </label>
                </div>
            {/if}

            <div class="modal-actions">
                <button
                    class="modal-btn primary"
                    onclick={startCustomReview}
                    disabled={reviewKind === 'word' && selectedStatuses.length === 0}
                >
                    Start Review
                </button>
                <button class="modal-btn" onclick={close}>Cancel</button>
            </div>
        </div>
    </div>
{/if}

<style>
    .custom-review-modal {
        width: 420px;
        max-width: min(420px, 90vw);
        justify-content: left;
    }

    .review-kind-toggle {
        display: flex;
        background: color-mix(in srgb, var(--theme-background, #1a1a1a) 60%, transparent);
        border-radius: 999px;
        padding: 0.2rem;
        gap: 0.2rem;
    }

    .review-kind-toggle button {
        flex: 1;
        border: none;
        border-radius: 999px;
        padding: 0.4rem 0.9rem;
        background: none;
        color: var(--theme-textSecondary, #b3b3b3);
        font: inherit;
        font-size: 0.82rem;
        font-weight: 600;
        cursor: pointer;
        transition: background 0.15s ease, color 0.15s ease;
    }

    .review-kind-toggle button.active {
        background: var(--theme-primary, #36b7bd);
        color: #fff;
    }

    .count-row {
        display: flex;
        flex-direction: column;
        gap: 0.6rem;
        width: 100%;
    }

    .count-row-header {
        display: flex;
        align-items: baseline;
        justify-content: space-between;
    }

    .count-row-label {
        font-size: 0.85rem;
        font-weight: 600;
        color: var(--theme-text, #f6f6f6);
    }

    .count-badge {
        display: flex;
        align-items: baseline;
        gap: 0.2rem;
        font-size: 1.4rem;
        font-weight: 800;
        color: var(--theme-primary, #36b7bd);
        font-variant-numeric: tabular-nums;
    }

    .count-badge-unit {
        font-size: 0.75rem;
        font-weight: 600;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .count-slider {
        -webkit-appearance: none;
        width: 100%;
        height: 6px;
        border-radius: 999px;
        background: linear-gradient(
            to right,
            var(--theme-primary, #36b7bd) 0%,
            var(--theme-primary, #36b7bd) var(--slider-fill, 50%),
            color-mix(in srgb, var(--theme-border, #404040) 80%, transparent) var(--slider-fill, 50%),
            color-mix(in srgb, var(--theme-border, #404040) 80%, transparent) 100%
        );
        outline: none;
        cursor: pointer;
    }

    .count-slider::-webkit-slider-thumb {
        -webkit-appearance: none;
        width: 16px;
        height: 16px;
        border-radius: 50%;
        background: #fff;
        border: 2px solid var(--theme-primary, #36b7bd);
        box-shadow: 0 1px 4px rgba(0, 0, 0, 0.3);
        cursor: pointer;
        transition: transform 0.1s ease;
    }

    .count-slider::-webkit-slider-thumb:hover {
        transform: scale(1.15);
    }

    .translated-toggle-row {
        flex-direction: column;
        align-items: flex-start;
        gap: 0.6rem;
    }
    
    .toggle-option {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 1.5rem;
        padding: 0.7rem 0.9rem;
        border-radius: 12px;
        background: color-mix(in srgb, var(--theme-background, #1a1a1a) 50%, transparent);
        border: 1px solid var(--theme-border, #404040);
        cursor: pointer;
        transition: border-color 0.15s ease, background 0.15s ease;
    }

    .setting-row.translated-toggle-row {
        align-self: flex-start;
        width: 100%;
    }
    
    .toggle-option:has(input:checked) {
        border-color: color-mix(in srgb, var(--theme-primary, #36b7bd) 50%, transparent);
        background: color-mix(in srgb, var(--theme-primary, #36b7bd) 8%, transparent);
    }
    
    .toggle-option-text {
        font-size: 0.88rem;
        font-weight: 600;
        color: var(--theme-text, #f6f6f6);
    }

    .switch {
        position: relative;
        display: inline-block;
        width: 42px;
        height: 24px;
        flex-shrink: 0;
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
        width: 18px;
        height: 18px;
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
        transform: translateX(18px);
    }
</style>