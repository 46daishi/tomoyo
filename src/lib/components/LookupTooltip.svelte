<script>
    import { fly } from 'svelte/transition';
    import ActionButton from '$lib/components/ActionButton.svelte';
    import { ICONS } from '$lib/icons';

    let { tooltipSpan, settings, tooltipX = null, tooltipY = null, tooltipMaxHeight = null, onMouseLeave, onMine, mineStatuses = {} } = $props();
    
    let positionStyle = $derived(
        (tooltipX !== null && tooltipY !== null ? `left: ${tooltipX}px; top: ${tooltipY}px;` : '') +
        (tooltipMaxHeight !== null ? ` max-height: ${tooltipMaxHeight}px;` : '')
    );

    // Maps an entry's mine status to how its action button should look:
    // not mined yet -> primary "add" button
    // mined from a different sentence/media -> secondary, still clickable (adds another example)
    // already mined from this exact sentence/media -> secondary + disabled (nothing new to add)
    function mineButtonState(entryId) {
        const status = mineStatuses[entryId];
        if (status === 'same') return { variant: 'secondary', disabled: true };
        if (status === 'different') return { variant: 'secondary', disabled: false };
        return { variant: 'primary', disabled: false };
    }
</script>

<div
    class="lookup-tooltip"
    class:static-position={tooltipX === null}
    style={positionStyle}
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
                {@const btnState = mineButtonState(entry.id)}
                <li>
                    <div class="entry-row">
                        <div class="entry-body">
                            <span class="entry-readings">
                                {entry.spellings[0] ?? entry.readings[0]}
                                {#if entry.readings[0] && entry.spellings.length > 0}
                                    <span class="entry-reading-kana">{entry.readings[0]}</span>
                                {/if}
                            </span>
                            <div class="entry-pos">{entry.pos.join(', ')}</div>
                            <div class="entry-definitions">{entry.definitions.join('; ')}</div>
                        </div>
                        <ActionButton
                            icon={ICONS.plus}
                            variant={btnState.variant}
                            disabled={btnState.disabled}
                            size="mini"
                            onAction={() => !btnState.disabled && onMine(entry)}
                        />
                    </div>
                </li>
            {/each}
        </ul>
    {:else}
        <div class="tooltip-no-match">No dictionary entry found.</div>
    {/if}
</div>

<style>
    .entry-row {
        display: flex;
        align-items: flex-start;
        justify-content: space-between;
        gap: 0.75rem;
        width: 100%;
    }

    .entry-body {
        flex: 1;
        min-width: 0;
    }
</style>