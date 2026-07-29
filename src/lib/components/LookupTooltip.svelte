<script>
    import { fly } from 'svelte/transition';

    let { tooltipSpan, settings, tooltipX, tooltipY, onMouseLeave } = $props();
</script>

<div
    class="lookup-tooltip"
    style="left: {tooltipX}px; top: {tooltipY}px;"
    transition:fly={{ y: 6, duration: 120 }}
    onclick={(event) => event.stopPropagation()}
    onwheel={(event) => event.stopPropagation()}
    onmouseleave={onMouseLeave}
>
    <div class="tooltip-surface">
        {tooltipSpan.surface}
        {#if tooltipSpan.deconjugated_from}
            <span class="tooltip-deconj">({tooltipSpan.deconjugated_from})</span>
        {/if}
    </div>
    {#if tooltipSpan.entries.length > 0 || (tooltipSpan.related_entries.length > 0 && settings?.show_related_entries)}
        <ul class="tooltip-entries">
            {#each [...tooltipSpan.entries, ...(settings?.show_related_entries ? tooltipSpan.related_entries : [])] as entry}
                <li>
                    <span class="entry-readings">
                        {entry.spellings[0] ?? entry.readings[0]}
                        {#if entry.readings[0] && entry.spellings.length > 0}
                            <span class="entry-reading-kana">{entry.readings[0]}</span>
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