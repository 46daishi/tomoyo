<script>
    import { calculateLabelStep, calculateNiceMax, formatTick } from '$lib/utils/chartFormatters.js';

    /**
     * @type {{
     *   data?: Array<Record<string, any>>,
     *   series?: Array<{ key: string, color: string, label: string, formatValue?: (v: number) => string, axisFormat?: (v: number) => string }>,
     *   color?: string,
     *   formatValue?: (v: number) => string,
     *   formatLabel?: (k: string) => string,
     *   showAxisLabels?: boolean,
     *   showYAxis?: boolean,
     * }}
     */
    let {
        data = [],
        series = [],
        color = 'var(--theme-primary, #36b7bd)',
        formatValue = (v) => v.toLocaleString(),
        formatLabel = (k) => k,
        showAxisLabels = true,
        showYAxis = false,
    } = $props();

    let chartWidth = $state(400);
    const height = 230;

    const axisTicks = 4;
    const labelGap = 8;
    const charWidth = 5.6;

    let hasYAxis = $derived(showYAxis && series.length > 0);
    let hasRightAxis = $derived(hasYAxis && series.length > 1);

    /** @param {number} si */
    function axisLabelsFor(si) {
        return ticksFor(si).map((t) => formatAxis(si, t));
    }

    let leftLabels = $derived(hasYAxis ? axisLabelsFor(0) : []);
    let rightLabels = $derived(hasRightAxis ? axisLabelsFor(1) : []);
    let widestLeft = $derived(Math.max(0, ...leftLabels.map((s) => s.length)));
    let widestRight = $derived(Math.max(0, ...rightLabels.map((s) => s.length)));
    let leftGutter = $derived(hasYAxis ? Math.min(52, Math.max(26, widestLeft * charWidth + labelGap)) : 2);
    let rightGutter = $derived(hasRightAxis ? Math.min(52, Math.max(26, widestRight * charWidth + labelGap)) : 2);

    let padding = $derived({
        top: 2,
        right: rightGutter,
        bottom: 24,
        left: leftGutter,
    });

    let plotW = $derived(Math.max(0, chartWidth - padding.left - padding.right));
    const plotH = $derived(height - padding.top - padding.bottom);
    const baseY = $derived(padding.top + plotH);

    let chartSeries = $derived(
        series.length > 0 ? series : [{ key: 'value', color, label: '' }]
    );
    let maxes = $derived(chartSeries.map((s) => Math.max(1, ...data.map((d) => d[s.key] ?? 0))));
    let axisMaxes = $derived(chartSeries.map((s) => calculateNiceMax(Math.max(1, ...data.map((d) => d[s.key] ?? 0)))));
    let labelStep = $derived(calculateLabelStep(data.length));

    /** @param {number} si */
    function ticksFor(si) {
        const step = axisMaxes[si] / axisTicks;
        return Array.from({ length: axisTicks + 1 }, (_, i) => i * step);
    }

    /** @param {number} si @param {number} v */
    function formatAxis(si, v) {
        const f = chartSeries[si].axisFormat;
        return f ? f(v) : formatTick(v);
    }

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

            {#if hasYAxis}
                {#each chartSeries as s, si}
                    {#each ticksFor(si) as t}
                        {@const y = baseY - (t / axisMaxes[si]) * plotH}
                        {#if si === 0}
                            <line x1={padding.left} y1={y} x2={chartWidth - padding.right} y2={y} class="grid-line faint" />
                            <text x={padding.left - labelGap} y={y + 3} class="axis-label" text-anchor="end">
                                {formatAxis(si, t)}
                            </text>
                        {:else}
                            <text x={chartWidth - padding.right + labelGap} y={y + 3} class="axis-label" text-anchor="start">
                                {formatAxis(si, t)}
                            </text>
                        {/if}
                    {/each}
                {/each}
            {/if}

                {#each data as point, i}
                    {@const slot = plotW / data.length}
                    {@const inner = slot / chartSeries.length}
                    {@const bw = Math.max(2, inner - 3)}

                    {#each chartSeries as s, si}
                        {@const x = padding.left + i * slot + si * inner + (inner - bw) / 2}
                        {@const barH = ((point[s.key] ?? 0) / axisMaxes[si]) * plotH}
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

                {#each data as point, i}
                    {@const slot = plotW / data.length}
                    {#if showAxisLabels && i % labelStep === 0}
                        <text x={padding.left + i * slot + slot / 2} y={height - 8} class="axis-label" text-anchor="middle">
                            {formatLabel(point.key)}
                        </text>
                    {/if}
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

    .grid-line.faint {
        stroke: color-mix(in srgb, var(--theme-border, #404040) 30%, transparent);
    }

    .axis-label {
        font-size: 10px;
        fill: var(--theme-textSecondary, #b3b3b3);
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
        font-family: "Noto Sans JP";
        font-weight: 600;
        font-size: 0.75rem;
        font-variant-numeric: tabular-nums;
    }
</style>
