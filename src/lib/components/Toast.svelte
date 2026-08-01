<script>
    import { fly } from 'svelte/transition';
    let { message = null } = $props();
</script>

{#if message}
    <div class="mine-toast" transition:fly={{ y: 12, duration: 150 }}>
        {message}
    </div>
{/if}

<style>
    .mine-toast {
        position: fixed;
        right: 1.5rem;
        bottom: 1.5rem;
        z-index: 2000;
        max-width: 280px;
        background: var(--theme-surface, #2d2d2d);
        color: var(--theme-text, #f6f6f6);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 10px;
        padding: 0.6rem 1rem;
        font-size: 0.9rem;
        line-height: 1.4;
        box-shadow: 0 6px 18px rgba(0, 0, 0, 0.35);
        pointer-events: none;
    }
</style>