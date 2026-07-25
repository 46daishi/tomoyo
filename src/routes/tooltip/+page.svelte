<script>
    import { onMount } from 'svelte';
    import { listen } from '@tauri-apps/api/event';
    import { getCurrentWindow, LogicalSize } from '@tauri-apps/api/window';
    
    let tooltipRootEl;

    let span = $state(null);

    onMount(() => {
        const unlistenContent = listen('tooltip-content', (event) => {
            span = event.payload;
        });
    
        const unlistenTheme = listen('theme-changed', (event) => {
            const root = document.documentElement;
            for (const [k, v] of Object.entries(event.payload)) {
                root.style.setProperty(`--theme-${k}`, v);
            }
        });

        const resizeObserver = new ResizeObserver((entries) => {
            for (const entry of entries) {
                const height = Math.ceil(entry.contentRect.height);
                const width = Math.ceil(entry.contentRect.width);
                getCurrentWindow().setSize(new LogicalSize(width, height));
            }
        });
    
        if (tooltipRootEl) resizeObserver.observe(tooltipRootEl);
    
        return () => {
            unlistenContent.then((f) => f());
            unlistenTheme.then((f) => f());
            resizeObserver.disconnect();
        };
    });
</script>

{#if span}
    <div class="lookup-tooltip" bind:this={tooltipRootEl}>
        <div class="tooltip-surface">
            {span.surface}
            {#if span.deconjugated_from}
                <span class="tooltip-deconj">({span.deconjugated_from})</span>
            {/if}
        </div>

        {#if span.entries.length > 0 || span.related_entries.length > 0}
            <ul class="tooltip-entries">
                {#each [...span.entries, ...span.related_entries] as entry}
                    <li>
                        <span class="entry-readings">
                            {entry.spellings[0] ?? entry.readings[0]}
                            {#if entry.readings[0] && entry.spellings.length > 0}
                                <span class="entry-reading-kana">({entry.readings[0]})</span>
                            {/if}
                        </span>
                        <div class="entry-pos">{entry.pos.join(', ')}</div>
                        <div class="entry-definitions">{entry.definitions.join('; ')}</div>
                    </li>
                {/each}
            </ul>
        {:else}
            <div class="tooltip-no-match">No dictionary entry found.</div>
        {/if}
    </div>
{/if}

<style>
    :global(html), :global(body) {
        background: transparent;
        margin: 0;
        overflow: hidden;
    }

    .lookup-tooltip {
        position: absolute;
        text-align: left;
        min-width:300px;
        max-width: 500px;
        background: color-mix(
            in srgb,
            color-mix(in srgb, var(--theme-surface, #1e1e2e) 80%, black 100%) 20%,
            transparent
        );
        border: 1px solid var(--theme-border, #404040);
        border-radius: 10px;
        padding: 0.9rem 1.1rem;
        box-shadow: 0 8px 24px rgba(0, 0, 0, 0.35);
        z-index: 10;
        font-family: "Noto Sans JP", Inter, sans-serif;
    }
</style>