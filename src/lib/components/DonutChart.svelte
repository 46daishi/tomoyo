<script>
    /**
     * Donut chart showing a categorical split. The center shows the total;
     * hovering a segment (or its legend row) swaps the center text to that
     * category's value and label.
     *
     * @type {{
     *   data?: Array<{ label: string, value: number, color: string }>,
     * }}
     */
    let { data = [] } = $props();

    let hovered = $state(/** @type {number | null} */ (null));

    let total = $derived(data.reduce((sum, d) => sum + d.value, 0));

    const R = 70;
    const STROKE = 18;
    const C = 2 * Math.PI * R;
    const GAP = 2.5;

    /** @param {number} idx */
    function offsetFor(idx) {
        let off = 0;
        for (let i = 0; i < idx; i++) off += data[i].value;
        return (off / Math.max(1, total)) * C;
    }
</script>

<div class="donut-chart">
    <div class="donut-wrap">
        <svg viewBox="0 0 200 200" class="donut-svg" role="img" aria-label="Dictionary status split">
            <circle
                cx="100"
                cy="100"
                r={R}
                fill="none"
                stroke="var(--theme-border, #404040)"
                stroke-width={STROKE}
                opacity="0.4"
            />
            {#each data as seg, i}
                {@const len = total > 0 ? Math.max(0, (seg.value / total) * C - (seg.value > 0 ? GAP : 0)) : 0}
                <circle
                    cx="100"
                    cy="100"
                    r={R}
                    fill="none"
                    stroke={seg.color}
                    stroke-width={STROKE}
                    stroke-dasharray={`${len} ${C - len}`}
                    stroke-dashoffset={-offsetFor(i)}
                    transform="rotate(-90 100 100)"
                    role="img"
                    aria-label={seg.label}
                    class="seg"
                    class:dimmed={hovered !== null && hovered !== i}
                    onmouseenter={() => (hovered = i)}
                    onmouseleave={() => (hovered = null)}
                />
            {/each}
        </svg>
        <div class="donut-center">
            <div class="center-value">{(hovered !== null ? data[hovered].value : total).toLocaleString()}</div>
            <div class="center-label">{hovered !== null ? data[hovered].label : 'words'}</div>
        </div>
    </div>

    <div class="donut-legend">
        {#each data as seg, i}
            <div
                class="legend-row"
                class:active={hovered === i}
                role="button"
                tabindex="0"
                onmouseenter={() => (hovered = i)}
                onmouseleave={() => (hovered = null)}
            >
                <span class="legend-dot" style="background: {seg.color}"></span>
                <span class="legend-label">{seg.label}</span>
                <span class="legend-value">{seg.value.toLocaleString()}</span>
            </div>
        {/each}
    </div>
</div>

<style>
    .donut-chart {
        display: flex;
        align-items: center;
        gap: 1.5rem;
        width: 100%;
        height: 100%;
    }

    .donut-wrap {
        position: relative;
        flex-shrink: 0;
        width: 200px;
    }

    .donut-svg {
        width: 100%;
        height: auto;
        display: block;
    }

    .seg {
        transition: opacity 0.15s ease;
        cursor: pointer;
    }

    .seg.dimmed {
        opacity: 0.3;
    }

    .donut-center {
        position: absolute;
        inset: 0;
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: 2px;
        pointer-events: none;
        text-align: center;
        padding: 0 16px;
        box-sizing: border-box;
    }

    .center-value {
        font-size: 1.45rem;
        font-weight: 700;
        color: var(--theme-text, #f6f6f6);
        font-variant-numeric: tabular-nums;
        line-height: 1.1;
        max-width: 100%;
        overflow: hidden;
        text-overflow: ellipsis;
    }

    .center-label {
        font-size: 0.72rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .donut-legend {
        display: flex;
        flex-direction: column;
        gap: 0.4rem;
        flex: 1;
        min-width: 0;
    }

    .legend-row {
        display: flex;
        align-items: center;
        gap: 0.55rem;
        font-size: 0.85rem;
        color: var(--theme-text, #f6f6f6);
        cursor: pointer;
        border-radius: 6px;
        padding: 2px 6px;
        margin: 0 -6px;
        transition: background 0.15s ease;
    }

    .legend-row:hover,
    .legend-row.active {
        background: color-mix(in srgb, var(--theme-border, #404040) 35%, transparent);
    }

    .legend-dot {
        width: 11px;
        height: 11px;
        border-radius: 50%;
        flex-shrink: 0;
    }

    .legend-label {
        flex: 1;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .legend-value {
        font-weight: 600;
        font-variant-numeric: tabular-nums;
    }
</style>
