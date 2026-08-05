<script>
    let { options, values = [], onChange } = $props();

    function toggle(value) {
        const next = values.includes(value)
            ? values.filter((v) => v !== value)
            : [...values, value];
        onChange(next);
    }
</script>

<div class="multi-select">
    {#each options as option}
        <button
            type="button"
            class="multi-select-option"
            class:active={values.includes(option.value)}
            onclick={() => toggle(option.value)}
        >
            {#if option.color}
                <span class="option-dot" style={`--dot-color: ${option.color}`}></span>
            {/if}
            {option.label}
        </button>
    {/each}
</div>

<style>
    .multi-select {
        display: flex;
        flex-wrap: wrap;
        gap: 0.4rem;
    }

    .multi-select-option {
        display: flex;
        align-items: center;
        gap: 0.4rem;
        background: color-mix(in srgb, var(--theme-textSecondary, #b3b3b3) 10%, transparent);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 999px;
        padding: 0.35rem 0.75rem;
        font: inherit;
        font-size: 0.8rem;
        color: var(--theme-textSecondary, #b3b3b3);
        cursor: pointer;
        transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
    }

    .multi-select-option:hover {
        border-color: var(--theme-textSecondary, #b3b3b3);
    }

    .multi-select-option.active {
        background: color-mix(in srgb, var(--theme-primary, #36b7bd) 18%, transparent);
        border-color: var(--theme-primary, #36b7bd);
        color: var(--theme-text, #f6f6f6);
        font-weight: 600;
    }

    .option-dot {
        width: 0.7rem;
        height: 0.7rem;
        border-radius: 50%;
        background: var(--dot-color, #6c7086);
        flex-shrink: 0;
    }
</style>