<script>
    /**
     * HeatMap.svelte
     * Standard activity heatmap showing total focus minutes per day with a broad color scale.
     *
     * Props:
     * data         – Array<{ date: string, studyMinutes: number }>
     * palette      - Array of colors from lowest to highest intensity
     * weeks        – maximum number of weeks to show (default 52)
     */
     import { formatDateFull, formatMinutes } from "$lib/utils/chartFormatters.js";
     
     export let data = [];
     export let primaryColor = "var(--theme-accent)";
     export let weeks = 52;
     export let formatValue = formatMinutes;
         
     // Dynamically generate a 9-step palette using modern CSS color-mix()
     // It mixes your primary color with the background for lower intensities, 
     // and with black to deepen the highest intensities.
     $: palette = [
        "var(--theme-border, #333333)",                                               // 0: No focus time
        `color-mix(in srgb, ${primaryColor}, var(--theme-background, #1a1a1a) 85%)`, // 1: Very low
        `color-mix(in srgb, ${primaryColor}, var(--theme-background, #1a1a1a) 70%)`, // 2
        `color-mix(in srgb, ${primaryColor}, var(--theme-background, #1a1a1a) 55%)`, // 3
        `color-mix(in srgb, ${primaryColor}, var(--theme-background, #1a1a1a) 38%)`, // 4
        `color-mix(in srgb, ${primaryColor}, var(--theme-background, #1a1a1a) 22%)`, // 5
        `color-mix(in srgb, ${primaryColor}, var(--theme-background, #1a1a1a) 10%)`, // 6
        primaryColor,                                                                 // 7
        primaryColor,                                                                 // 8: Max activity
     ];

    import { onMount } from "svelte";

    let wrapEl;
    let W = 0;

    // ── Constants ─────────────────────────────────────────────────────────── [...]
    const CELL      = 13;
    const GAP       =  3;
    const DAY_LBL_W = 28;
    const MONTH_H   = 20;

    // How many weeks actually fit in the current width
    $: visibleWeeks = W > 0
        ? Math.min(weeks, Math.max(1, Math.floor((W - DAY_LBL_W) / (CELL + GAP))))
        : weeks;
        
    $: svgW = DAY_LBL_W + visibleWeeks * (CELL + GAP) - GAP;
    $: svgH = MONTH_H   + 7          * (CELL + GAP) - GAP;

    // ── Date-keyed lookup ─────────────────────────────────────────────────────
    $: dataMap = Object.fromEntries(
        data.map((d) => [d.date, d.studyMinutes ?? 0]),
    );

    // ── Grid ───────────────────────────────────────────────────────────[...]
    $: grid = (() => {
        const today = new Date();
        today.setHours(0, 0, 0, 0);

        const weekStart = new Date(today);
        weekStart.setDate(today.getDate() - today.getDay());

        const cols = [];
        for (let w = visibleWeeks - 1; w >= 0; w--) {
            const col = [];
            for (let d = 0; d < 7; d++) {
                const date = new Date(weekStart);
                date.setDate(weekStart.getDate() - w * 7 + d);
              
                const key = `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
                col.push({
                    date,
                    key,
                    mins: dataMap[key] ?? 0,
                    isFuture: date > today,
                });
            }
            cols.push(col);
        }
        return cols;
    })();

    // ── Intensity & Colors ────────────────────────────────────────────────────
    $: maxMins = Math.max(...data.map((d) => d.studyMinutes ?? 0), 1);

    // Replaces the old 0-4 intensity function with a dynamic proportional scale
    function getColor(mins) {
        if (mins <= 0) return palette[0];
        
        const r = mins / maxMins; // Ratio between 0.0 and 1.0
        const maxIndex = palette.length - 1;
        
        // Math.ceil ensures that even tiny amounts get at least index 1
        const index = Math.max(1, Math.ceil(r * maxIndex));
        return palette[Math.min(index, maxIndex)];
    }

    // ── Month labels ────────────────────────────────────────────────────────── [...]
    $: monthLabels = (() => {
        const out = [];
        let last = -1;
        grid.forEach((col, wi) => {
            const m = col[0].date.getMonth();
            if (m !== last) {
                out.push({
                    wi,
                    label: col[0].date.toLocaleDateString("ja-JP", { month: "short" }),
                });
                last = m;
            }
        });
        return out;
    })();

    // ── Tooltip ─────────────────────────────────────────────────────────── [...]
    let tooltip = null;
    
    function showTooltip(e, cell) {
        if (!wrapEl) return;
        const rect = wrapEl.getBoundingClientRect();
        tooltip = { x: e.clientX - rect.left, y: e.clientY - rect.top, cell };
    }
    
    function hideTooltip() { tooltip = null; }

    $: ttX = tooltip ? Math.min(tooltip.x + 14, (W || 400) - 180) : 0;
    $: ttY = tooltip ? Math.max(tooltip.y - 40, 4) : 0;

    // ── Day labels ────────────────────────────────────────────────────────── [...]
    const DAY_NAMES = ["日", "月", "火", "水", "木", "金", "土"];
    const SHOW_ROWS = new Set([0, 1, 2, 3, 4, 5, 6]);

    onMount(() => { W = wrapEl?.clientWidth ?? 0; });
</script>

<div class="heatmap-outer" bind:clientWidth={W} bind:this={wrapEl}>
    {#if W > 0}
    <div class="heatmap-clip">
        <svg width={svgW} height={svgH} style="display:block;overflow:visible;">
            {#each monthLabels as ml}
                <text
                    x={DAY_LBL_W + ml.wi * (CELL + GAP)}
                    y={MONTH_H - 6}
                    font-size="13"
                    fill="var(--theme-textSecondary,#b3b3b3)"
                >{ml.label}</text>
            {/each}

            {#each DAY_NAMES as name, di}
                {#if SHOW_ROWS.has(di)}
                    <text
                        x={DAY_LBL_W - 4}
                        y={MONTH_H + di * (CELL + GAP) + CELL / 2}
                        font-size="13"
                        fill="var(--theme-textSecondary,#b3b3b3)"
                        text-anchor="end"
                        dominant-baseline="middle"
                    >{name}</text>
                {/if}
            {/each}

            {#each grid as col, wi}
                {#each col as cell, di}
                    <rect
                        x={DAY_LBL_W + wi * (CELL + GAP)}
                        y={MONTH_H   + di * (CELL + GAP)}
                        width={CELL} height={CELL}
                        rx="2" ry="2"
                        class="cell"
                        class:future={cell.isFuture}
                        style="fill: {getColor(cell.mins)};"
                        on:mouseenter={(e) => !cell.isFuture && showTooltip(e, cell)}
                        on:mousemove={(e)  => !cell.isFuture && showTooltip(e, cell)}
                        on:mouseleave={hideTooltip}
                    />
                {/each}
            {/each}
        </svg>
    </div>

        {#if tooltip}
            <div class="tooltip" style="left:{ttX}px;top:{ttY}px;">
                <div class="tt-date">{formatDateFull(tooltip.cell.date)}</div>
                <div class="tt-val">{formatValue(tooltip.cell.mins)}</div>
            </div>
        {/if}
    {/if}
</div>

<style>
    .heatmap-outer {
        position: relative;
        width: 100%;
        overflow: visible;
        padding-bottom: 0; 
    }

    .heatmap-clip {
        overflow: hidden;
        width: 100%;
    }

    .cell {
        cursor: default;
        /* Set base opacity to 1 so vibrant colors show clearly */
        opacity: 1;
        transition: filter 0.1s;
    }

    /* Use a brightness filter for the hover effect instead of opacity */
    .cell:not(.future):hover {
        filter: brightness(1.2);
        outline-offset: 1px;
    }

    .cell.future {
        opacity: 0.1;
    }

    .tooltip {
        position: absolute;
        pointer-events: none;
        background: var(--theme-background, #1a1a1a);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 8px;
        padding: 8px 12px;
        font-size: 0.8rem;
        color: var(--theme-text, #f6f6f6);
        box-shadow: 0 4px 16px rgba(0, 0, 0, 0.5);
        white-space: nowrap;
        z-index: 999;
    }

    .tt-date {
        font-weight: 600;
        margin-bottom: 4px;
        color: var(--theme-textSecondary, #b3b3b3);
        font-size: 0.72rem;
        text-transform: uppercase;
        letter-spacing: 0.06em;
    }

    .tt-val {
        font-weight: 600;
        font-size: 0.85rem;
    }
</style>