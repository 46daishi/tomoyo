<script>
    /**
     * @type {{
     *   data?: Array<Record<string, any>>,
     *   series?: Array<{ key: string, color: string, label: string, formatValue?: (v: number) => string }>,
     *   color?: string,
     *   formatValue?: (v: number) => string,
     *   formatLabel?: (k: string) => string,
     * }}
     */
    let {
        data = [],
        series = [],
        color = 'var(--theme-primary, #36b7bd)',
        formatValue = (v) => v.toLocaleString(),
        formatLabel = (k) => k,
    } = $props();

    let chartWidth = $state(400);
    const height = 230;
    const padding = { top: 16, right: 10, bottom: 34, left: 10 };

    let plotW = $derived(Math.max(0, chartWidth - padding.left - padding.right));
    const plotH = height - padding.top - padding.bottom;
    const baseY = padding.top + plotH;

    let chartSeries = $derived(
        series.length > 0 ? series : [{ key: 'value', color, label: '' }]
    );
    let maxes = $derived(chartSeries.map((s) => Math.max(1, ...data.map((d) => d[s.key] ?? 0))));

    let hovered = $state(/** @type {number | null} */ (null));

    /** @param {MouseEvent & { currentTarget: SVGSVGElement }} event */
    function handleMove(event) {
        if (data.length === 0) return;
        const rect = event.currentTarget.getBoundingClientRect();
        const relX = event.clientX - rect.left;
        const slot = plotW / data.length;
        const idx = Math.floor((relX - padding.left) / slot);
        hovered = idx >= 0 && idx < data.length ? idx : null;
    }
</script>

<div class="bar-chart-wrapper" bind:clientWidth={chartWidth}>
    {#if data.length > 0 && chartSeries.length > 0}
        <div class="legend">
            {#each chartSeries as s}
                <div class="legend-item">
                    <span class="dot" style="background: {s.color}"></span>
                    {s.label}
                </div>
            {/each}
        </div>
    {/if}

    {#if data.length === 0}
        <div class="empty">No data in this period</div>
    {:else}
        <svg
            viewBox="0 0 {chartWidth} {height}"
            class="chart-svg"
            role="img"
            aria-label="Bar chart"
            onmousemove={handleMove}
            onmouseleave={() => (hovered = null)}
        >
            <!-- Baseline -->
            <line x1={padding.left} y1={baseY} x2={chartWidth - padding.right} y2={baseY} class="grid-line" />

            {#each data as point, i}
                {@const slot = plotW / data.length}
                {@const inner = slot / chartSeries.length}
                {@const bw = Math.max(2, inner - 3)}

                {#each chartSeries as s, si}
                    {@const x = padding.left + i * slot + si * inner + (inner - bw) / 2}
                    {@const barH = ((point[s.key] ?? 0) / maxes[si]) * plotH}
                    {@const y = baseY - barH}

                    <!-- +1 height overdraws the baseline so the bottom stays square while the top rounds -->
                    <rect
                        {x}
                        y={y - 1}
                        width={bw}
                        height={barH + 1}
                        rx="4"
                        fill={s.color}
                        class="bar"
                        class:dimmed={hovered !== null && hovered !== i}
                        class:hot={hovered === i}
                    />
                {/each}
            {/each}
        </svg>
    {/if}

    {#if hovered !== null}
        {@const slot = plotW / data.length}
        {@const cx = padding.left + hovered * slot + slot / 2}
        <div class="tooltip" style="left: {Math.min(Math.max(cx, 70), chartWidth - 70)}px">
            <div class="tt-date">{formatLabel(data[hovered].key)}</div>
            {#each chartSeries as s}
                <div class="tt-row">
                    <span class="dot" style="background: {s.color}"></span>
                    <span class="tt-val">{(s.formatValue ?? formatValue)(data[hovered][s.key])}</span>
                </div>
            {/each}
        </div>
    {/if}
</div>

<style>
    .bar-chart-wrapper {
        position: relative;
        width: 100%;
        height: 100%;
    }

    .legend {
        display: flex;
        gap: 1.5rem;
        margin-bottom: 0.5rem;
        justify-content: flex-end;
    }

    .legend-item {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        font-size: 0.8rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .dot {
        width: 10px;
        height: 10px;
        border-radius: 50%;
        flex-shrink: 0;
    }

    .chart-svg {
        width: 100%;
        height: 230px;
        overflow: visible;
        cursor: crosshair;
    }

    .empty {
        color: var(--theme-textSecondary, #b3b3b3);
        font-size: 0.9rem;
        text-align: center;
        padding: 3rem 0;
    }

    .grid-line {
        stroke: color-mix(in srgb, var(--theme-border, #404040) 60%, transparent);
        stroke-width: 1;
    }

    .bar {
        transition: opacity 0.2s ease, filter 0.2s ease;
    }

    .bar.dimmed {
        opacity: 0.35;
    }

    .bar.hot {
        filter: brightness(1.15);
    }

    .tooltip {
        position: absolute;
        top: 18px;
        transform: translateX(-50%);
        pointer-events: none;
        background: var(--theme-background, #1a1a1a);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 8px;
        padding: 8px 12px;
        font-size: 0.8rem;
        color: var(--theme-text, #f6f6f6);
        box-shadow: 0 4px 16px var(--theme-shadow, rgba(0, 0, 0, 0.5));
        white-space: nowrap;
        z-index: 999;
        min-width: 120px;
    }

    .tt-date {
        font-weight: 600;
        margin-bottom: 4px;
        color: var(--theme-textSecondary, #b3b3b3);
        font-size: 0.72rem;
        text-transform: uppercase;
        letter-spacing: 0.06em;
    }

    .tt-row {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        margin-top: 0.2rem;
    }

    .tt-val {
        font-weight: 600;
        font-size: 0.85rem;
        font-variant-numeric: tabular-nums;
    }
</style>
