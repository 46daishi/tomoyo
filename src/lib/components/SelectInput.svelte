<script>
    /**
     * A styled native <select> used for profile switching and time-range
     * selection. Replaces TimeRangeSelector and all .profile-select instances.
     *
     * @type {Array<{ value: string, label: string }>}
     */
    export let options = [];
    /** Currently selected value. */
    export let value = "";
    /** When true, the select is non-interactive and visually dimmed. */
    export let disabled = false;
</script>

<select class="select-input" {value} {disabled} on:change>
    {#each options as opt (opt.value)}
        <option value={opt.value}>{opt.label}</option>
    {/each}
</select>

<style>
    .select-input {
        background-color: var(--theme-surface, #2d2d2d);
        color: var(--theme-text, #f6f6f6);
        border: 2px solid var(--theme-border, #404040);
        border-radius: 8px;
        padding: 0.4rem 2.5rem 0.4rem 1rem;
        font-size: 0.9rem;
        font-weight: 500;
        cursor: pointer;
        appearance: none;
        min-width: 100px;
        background-image: url("data:image/svg+xml,%3csvg xmlns='http://www.w3.org/2000/svg' fill='none' viewBox='0 0 20 20'%3e%3cpath stroke='%236b7280' stroke-linecap='round' stroke-linejoin='round' stroke-width='1.5' d='M6 8l4 4 4-4'/%3e%3c/svg%3e");
        background-position: right 0.75rem center;
        background-repeat: no-repeat;
        background-size: 1.5rem 1.5rem;
        transition:
            border-color 0.2s,
            background-color 0.2s;
    }

    .select-input:hover:not(:disabled) {
        border-color: var(--theme-primary, #36b7bd);
        background-color: var(--theme-surface, #3a3a3a);
    }

    .select-input:focus {
        outline: none;
        border-color: var(--theme-primary, #36b7bd);
        box-shadow: 0 0 0 3px rgba(54, 183, 189, 0.15);
    }

    .select-input:disabled {
        opacity: 0.5;
        cursor: not-allowed;
    }
</style>
