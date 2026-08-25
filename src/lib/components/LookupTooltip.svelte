<script>
    import { fly } from 'svelte/transition';
    import ActionButton from '$lib/components/ActionButton.svelte';
    import StatusMenu from '$lib/components/StatusMenu.svelte';
    import { ICONS } from '$lib/icons';
    import { STATUS_LEVELS } from '$lib/constants';
    import { getKnownWordsMap, updateWordStatus } from '$lib/dictionary.js';

    let { tooltipSpan, settings, tooltipX = null, tooltipY = null, tooltipMaxHeight = null, onMouseLeave, onMine, mineStatuses = {}, onStatusChanged } = $props();
    
    let positionStyle = $derived(
        (tooltipX !== null && tooltipY !== null ? `left: ${tooltipX}px; top: ${tooltipY}px;` : '') +
        (tooltipMaxHeight !== null ? ` max-height: ${tooltipMaxHeight}px;` : '')
    );

    let knownWordsMap = $state(new Map());
    let statusMenu = $state(null);

    async function loadKnownWords() {
        if (!settings?.underline_mined_words) {
            knownWordsMap = new Map();
            return;
        }
        knownWordsMap = await getKnownWordsMap();
    }

    $effect(() => {
        settings?.underline_mined_words;
        loadKnownWords();
    });

    function getEntryStatus(entryId) {
        if (!settings?.underline_mined_words) return null;
        return knownWordsMap.get(entryId) ?? null;
    }

    function handleEntryReadingClick(entry, event) {
        if (!settings?.underline_mined_words) return;
        const status = getEntryStatus(entry.id);
        if (status == null) return;
        event.stopPropagation();
        const rect = event.currentTarget.getBoundingClientRect();
        statusMenu = { x: rect.left, y: rect.bottom + 4, wordId: entry.id, current: status };
    }

    async function handleStatusSelect(newStatus) {
        if (!statusMenu) return;
        await updateWordStatus({ wordId: statusMenu.wordId, status: newStatus });
        knownWordsMap.set(statusMenu.wordId, newStatus);
        statusMenu = null;
        onStatusChanged?.();
    }

    function closeStatusMenu() {
        statusMenu = null;
    }

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
                            {@const entryStatus = getEntryStatus(entry.id)}
                            <li>
                                <div class="entry-row">
                                    <div class="entry-body">
                                        <span class="entry-readings">
                                            <span
                                                class="entry-spelling"
                                                class:known-word={entryStatus != null}
                                                style={entryStatus != null ? `--status-color: ${STATUS_LEVELS[entryStatus]?.color ?? ''}` : ''}
                                                onclick={(e) => handleEntryReadingClick(entry, e)}
                                            >{entry.spellings[0] ?? entry.readings[0]}</span>
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

    {#if statusMenu}
        <StatusMenu
            x={statusMenu.x}
            y={statusMenu.y}
            levels={STATUS_LEVELS}
            current={statusMenu.current}
            onSelect={handleStatusSelect}
            onClose={closeStatusMenu}
        />
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

    .entry-spelling.known-word {
        border-bottom: 2px solid var(--status-color, transparent);
        padding-bottom: 1px;
    }
</style>