<script>
    import { fly } from 'svelte/transition';
    import { session } from '$lib/stores/session.svelte.js';
</script>

{#if session.running}
    <span class="session-timer" transition:fly={{ y: -8, duration: 200 }}>
        {session.formattedTime}
    </span>
{/if}

<style>
    .session-timer {
        font-size: 0.75rem;
        font-weight: 600;
        color: var(--theme-textSecondary, #b3b3b3);
        font-variant-numeric: tabular-nums;
        text-align: center;
    }
</style>