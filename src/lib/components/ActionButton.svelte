<script>
    /** Nerd Font codepoint string (or any text) shown inside the button. */
    export let icon = "";
    /** Optional text label shown beside the icon. */
    export let label = "";
    /** Called when the button is clicked (not disabled). */
    export let onAction = () => {};
    /** Visual style variant. */
    export let variant = "primary"; // "primary" | "secondary" | "danger"
    /** Size preset. */
    export let size = "default"; // "default" | "tiny" | "small" | "large"
    /** When true, the button is non-interactive and visually dimmed. */
    export let disabled = false;

    function handleClick() {
        if (!disabled) onAction();
    }
</script>

<button
    class="action-button {variant} {size}"
    class:disabled
    on:click={handleClick}
    on:mouseenter
    on:mouseleave
    {disabled}
>
    {#if icon}
        <span class="icon nf" aria-hidden="true">{icon}</span>
    {/if}
    {#if label}
        <span class="label">{label}</span>
    {/if}
</button>

<style>
    .action-button {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: 0.5rem;
        border-radius: 100px;
        border: 1px solid transparent;
        padding: 1em 1.2em;
        font-size: 1em;
        font-weight: 500;
        font-family: inherit;
        cursor: pointer;
        transition: all 0.3s ease;
        box-shadow: 0 4px 8px var(--theme-shadow, rgba(0, 0, 0, 0.3));

        /* Default (secondary-like base) */
        background-color: var(--theme-button, #2d2d2d);
        color: var(--theme-buttonText, #ffffff);
    }

    /* ── Hover / active states ── */
    .action-button:hover:not(:disabled) {
        border-color: var(--theme-primary, #36b7bd);
        transform: translateY(-1px);
        box-shadow: 0 4px 12px var(--theme-shadow, rgba(0, 0, 0, 0.3));
    }
    .action-button:active:not(:disabled) {
        transform: translateY(0);
    }
    .action-button:disabled {
        opacity: 0.5;
        cursor: not-allowed;
        transform: none;
    }

    /* ── Variants ── */
    .action-button.primary {
        background-color: var(--theme-primary, #36b7bd);
        border-color: var(--theme-primary, #36b7bd);
        color: #ffffff;
    }
    .action-button.primary:hover:not(:disabled) {
        background-color: var(--theme-primaryHover, #17a4ab);
        border-color: var(--theme-primaryHover, #17a4ab);
    }

    .action-button.secondary {
        background-color: var(--theme-surface, #2d2d2d);
        color: var(--theme-text, #ffffff);
    }
    .action-button.secondary:hover:not(:disabled) {
        background-color: var(--theme-button, #1a1a1a);
    }

    .action-button.danger {
        background-color: #dc3545;
        border-color:  #dc3545;
        color: #ffffff;
    }
    
    .action-button.danger:hover:not(:disabled) {
        opacity: 0.85;
    }

    /* ── Sizes ── */
    .action-button.tiny {
        padding: 0.7em 0.9em;
        font-size: 0.9em;
    }
    .action-button.small {
        padding: 0.8em 1em;
        font-size: 1em;
    }
    .action-button.large {
        padding: 1.2em 1.5em;
        font-size: 1.1em;
    }

    /* ── Icon ── */
    .icon {
        font-size: 1.2em;
        line-height: 1;
    }

    .label {
        font-weight: 500;
    }
</style>
