<script>
    import { calculateLabelStep } from '$lib/utils/chartFormatters.js';

    /**
     * @type {{
     *   data?: Array<Record<string, any>>,
     *   series?: Array<{ key: string, color: string, label: string, formatValue?: (v: number) => string }>,
     *   formatLabel?: (k: string) => string,
     *   formatValue?: (v: number) => string,
     *   showAxisLabels?: boolean,
     * }}
     */
    let {
        data = [],
        series = [],
        formatLabel = (k) => k,
        formatValue = (v) => v.toLocaleString(),
        showAxisLabels = true,
    } = $props();

    let chartWidth = $state(700);
    const height = 240;
    const padding = { top: 16, right: 16, bottom: 36, left: 16 };

    let plotW = $derived(Math.max(0, chartWidth - padding.left - padding.right));
    const plotH = height - padding.top - padding.bottom;
    const baseY = padding.top + plotH;

    let maxes = $derived(series.map((s) => Math.max(1, ...data.map((d) => d[s.key] ?? 0))));
    let labelStep = $derived(calculateLabelStep(data.length));

    /** @param {number} i */
    function xFor(i) {
        if (data.length <= 1) return padding.left;
        return padding.left + (i / (data.length - 1)) * plotW;
    }

    /** @param {number} si @param {number} v */
    function yFor(si, v) {
        return baseY - (v / maxes[si]) * plotH;
    }

    /** @param {number} si */
    function linePoints(si) {
        if (data.length < 2) return '';
        const key = series[si].key;
        return data.map((d, i) => `${xFor(i)},${yFor(si, d[key])}`).join(' ');
    }

    let hovered = $state(/** @type {number | null} */ (null));
    let tooltipX = $state(0);

    /** @param {MouseEvent & { currentTarget: SVGSVGElement }} event */
    function handleMove(event) {
        if (data.length === 0) return;
        const rect = event.currentTarget.getBoundingClientRect();
        const relX = event.clientX - rect.left;

        const clampedX = Math.max(padding.left, Math.min(relX, chartWidth - padding.right));
        const idx = Math.round(((clampedX - padding.left) / plotW) * (data.length - 1));

        hovered = Math.max(0, Math.min(data.length - 1, idx));
        tooltipX = xFor(hovered);
    }
</script>

<div class="line-chart-wrapper" bind:clientWidth={chartWidth}>
    <div class="legend">
        {#each series as s}
            <div class="legend-item">
                <span class="dot" style="background: {s.color}"></span>
                {s.label}
            </div>
        {/each}
    </div>

    {#if data.length === 0}
        <div class="empty">No data in this period</div>
    {:else}
        <svg
            viewBox="0 0 {chartWidth} {height}"
            class="chart-svg"
            onmousemove={handleMove}
            onmouseleave={() => (hovered = null)}
            role="graphics-document"
        >
            <!-- Baseline -->
            <line x1={padding.left} y1={baseY} x2={chartWidth - padding.right} y2={baseY} class="grid-line" />

            {#each series as s, si}
                <polyline
                    points={linePoints(si)}
                    fill="none"
                    stroke={s.color}
                    stroke-width="2.5"
                    stroke-linejoin="round"
                    stroke-linecap="round"
                />
            {/each}

            {#if hovered !== null}
                <line x1={tooltipX} y1={padding.top} x2={tooltipX} y2={baseY} class="hover-line" />
                {#each series as s, si}
                    {@const y = yFor(si, data[hovered][s.key])}
                    <circle cx={tooltipX} cy={y} r="4.5" fill={s.color} stroke="var(--theme-surface, #1e1e2e)" stroke-width="2" />
                {/each}
            {/if}

            {#each data as d, i}
                {#if showAxisLabels && i % labelStep === 0}
                    <text x={xFor(i)} y={height - 8} class="axis-label" text-anchor="middle">
                        {formatLabel(d.key)}
                    </text>
                {/if}
            {/each}
        </svg>
    {/if}

    {#if hovered !== null}
        <div class="tooltip" style="left: {Math.min(Math.max(tooltipX, 70), chartWidth - 70)}px">
            <div class="tt-date">{formatLabel(data[hovered].key)}</div>
            {#each series as s}
                <div class="tt-row">
                    <span class="dot" style="background: {s.color}"></span>
                    <span class="tt-val">{(s.formatValue ?? formatValue)(data[hovered][s.key])}</span>
                </div>
            {/each}
        </div>
    {/if}
</div>

<style>
    .line-chart-wrapper {
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
        height: 240px;
        cursor: crosshair;
        overflow: visible;
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

    .axis-label {
        font-size: 10px;
        fill: var(--theme-textSecondary, #b3b3b3);
    }

    .hover-line {
        stroke: var(--theme-textSecondary, #b3b3b3);
        stroke-width: 1;
        stroke-dasharray: 4 4;
        opacity: 0.5;
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
