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

    // Distinct spellings a word can be mined as: its primary (kanji) form and
    // its kana reading. Collapses to a single choice for pure-kana words.
    function spellingChoices(entry, surface) {
        const spelling = entry.spellings[0] ?? surface;
        const reading = entry.readings[0];
        const choices = [];
        if (spelling) choices.push({ label: spelling, value: spelling });
        if (reading && reading !== spelling) choices.push({ label: reading, value: reading });
        return choices;
    }

    // Native <select> preselects its first option, which would swallow the
    // first choice's change event. Deselect everything so picking any option
    // (including the first) fires onchange.
    function deselect(node) {
        node.selectedIndex = -1;
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
                            {@const forms = spellingChoices(entry, tooltipSpan.surface)}
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
                                    <div class="mine-select">
                                        <ActionButton
                                            icon={ICONS.plus}
                                            variant={btnState.variant}
                                            disabled={btnState.disabled}
                                            size="mini"
                                            onAction={() => !btnState.disabled && onMine(entry, forms[0].value)}
                                        />
                                        {#if forms.length > 1}
                                            <select
                                                class="mine-native"
                                                aria-label="Mine word"
                                                disabled={btnState.disabled}
                                                use:deselect
                                                onchange={(e) => {
                                                    const value = e.currentTarget.value;
                                                    e.currentTarget.selectedIndex = -1;
                                                    onMine(entry, value);
                                                }}
                                            >
                                                {#each forms as form}
                                                    <option value={form.value}>{form.label}</option>
                                                {/each}
                                            </select>
                                        {/if}
                                    </div>
                                </div>
                            </li>
                        {/each}
        </ul>
    {:else}
        <div class="tooltip-no-match">No dictionary entry found.</div>
    {/if}
</div>

<style>
    .lookup-tooltip {
        font-size: 1rem;
        font-weight: 400;
        text-align: left;
    }
    
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

    .mine-select {
        position: relative;
        display: inline-flex;
        align-items: center;
        flex-shrink: 0;
    }

    /* The transparent <select> overlay intercepts pointer events, so the
       button's own :hover never fires; drive the hover from the container.
       :global() is required because .action-button is scoped to ActionButton. */
    .mine-select:hover :global(.action-button.primary) {
        background-color: var(--theme-primaryHover, #17a4ab);
        border-color: var(--theme-primaryHover, #17a4ab);
        transform: translateY(-1px);
        box-shadow: 0 4px 12px var(--theme-shadow, rgba(0, 0, 0, 0.3));
    }
    .mine-select:hover :global(.action-button.secondary) {
        background-color: var(--theme-button, #1a1a1a);
    }

    /* Invisible native select overlaying the action button so a click opens
       the system dropdown; onchange fires the mine action. */
    .mine-native {
        position: absolute;
        inset: 0;
        width: 100%;
        height: 100%;
        opacity: 0;
        cursor: pointer;
        border: none;
        background: transparent;
    }
</style>