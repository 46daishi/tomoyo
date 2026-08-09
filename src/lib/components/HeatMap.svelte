<script>
    /**
     * HeatMap.svelte
     * Standard activity heatmap showing total focus minutes per day with a broad color scale.
     *
     * Props:
     * data         – Array<{ date: string, studyMinutes: number }>
     * palette      - Array of colors from lowest to highest intensity
     * weeks        – maximum number of weeks to show (default 52); used when `year` is null
     * year         – when set (number), renders that full calendar year (Jan 1 – Dec 31)
     * formatValue  - formatter for the tooltip value
     */
     import { formatDateFull, formatMinutes } from "$lib/utils/chartFormatters.js";
     
     export let data = [];
     export let primaryColor = "var(--theme-accent)";
     export let weeks = 52;
     export let year = /** @type {number | null} */ (null);
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

    // ── Constants ───────────────────────────────────────────────────────────
    const GAP            =  3;
    const DAY_LBL_W      = 28;
    const MONTH_H        = 20;
    const CELL_TRAILING  = 13;

    // ── Date-keyed lookup ─────────────────────────────────────────────────────
    $: dataMap = Object.fromEntries(
        data.map((d) => [d.date, d.studyMinutes ?? 0]),
    );

    // ── Grid ────────────────────────────────────────────────────────────────
    // Full calendar year (Jan 1 – Dec 31) when `year` is set, otherwise the
    // trailing `weeks` columns ending today.
    $: visibleWeeks = W > 0
        ? Math.min(weeks, Math.max(1, Math.floor((W - DAY_LBL_W) / (CELL_TRAILING + GAP))))
        : weeks;

    $: grid = (() => {
        const today = new Date();
        today.setHours(0, 0, 0, 0);

        const cols = [];
        if (year !== null) {
            const colStart = new Date(year, 0, 1);
            colStart.setDate(colStart.getDate() - colStart.getDay());      // back to Sunday
            const colEnd = new Date(year, 11, 31);
            colEnd.setDate(colEnd.getDate() + (6 - colEnd.getDay()));      // forward to Saturday

            const cursor = new Date(colStart);
            while (cursor <= colEnd) {
                const col = [];
                for (let d = 0; d < 7; d++) {
                    const date = new Date(cursor);
                    date.setDate(cursor.getDate() + d);
                    const key = `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
                    col.push({
                        date,
                        key,
                        mins: dataMap[key] ?? 0,
                        isFuture: date > today,
                        inYear: date.getFullYear() === year,
                    });
                }
                cols.push(col);
                cursor.setDate(cursor.getDate() + 7);
            }
        } else {
            const weekStart = new Date(today);
            weekStart.setDate(today.getDate() - today.getDay());
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
                        inYear: true,
                    });
                }
                cols.push(col);
            }
        }
        return cols;
    })();

    $: numWeeks = grid.length;

    // In full-year mode the cells stretch to fill the available width so the
    // grid ends right at the edge instead of leaving dead space on the right.
    $: CELL = year !== null
        ? W > 0
            ? Math.min(22, Math.max(6, (W - DAY_LBL_W - (numWeeks - 1) * GAP) / numWeeks))
            : 12
        : CELL_TRAILING;

    $: svgW = DAY_LBL_W + numWeeks * (CELL + GAP) - GAP;
    $: svgH = MONTH_H   + 7          * (CELL + GAP) - GAP;

    // ── Intensity & Colors ────────────────────────────────────────────────────
    $: maxMins = Math.max(...data.map((d) => d.studyMinutes ?? 0), 1);

    // ── Month labels ──────────────────────────────────────────────────────────
    $: monthLabels = (() => {
        const out = [];
        let last = -1;
        grid.forEach((col, wi) => {
            const anchor = col.find((c) => c.inYear) ?? col[0];
            const m = anchor.date.getMonth();
            if (m !== last) {
                out.push({
                    wi,
                    label: anchor.date.toLocaleDateString("ja-JP", { month: "short" }),
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

    $: ttX = tooltip ? Math.min(Math.max(tooltip.x, 70), (W || 400) - 70) : 0;
    $: ttY = tooltip ? Math.max(tooltip.y - 48, 8) : 0;

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
                    {@const color = cell.mins <= 0
                        ? palette[0]
                        : palette[Math.min(palette.length - 1, Math.max(1, Math.ceil((cell.mins / maxMins) * (palette.length - 1))))]}
                    <rect
                        x={DAY_LBL_W + wi * (CELL + GAP)}
                        y={MONTH_H   + di * (CELL + GAP)}
                        width={CELL} height={CELL}
                        rx="2" ry="2"
                        class="cell"
                        class:future={cell.isFuture}
                        class:offyear={!cell.inYear}
                        style="fill: {color};"
                        role="presentation"
                        on:mouseenter={(e) => cell.inYear && !cell.isFuture && showTooltip(e, cell)}
                        on:mousemove={(e)  => cell.inYear && !cell.isFuture && showTooltip(e, cell)}
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

    .cell.offyear {
        opacity: 0.12;
    }

    .tooltip {
        position: absolute;
        pointer-events: none;
        transform: translateX(-50%);
        background: var(--theme-background, #1a1a1a);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 8px;
        padding: 8px 12px;
        font-size: 0.8rem;
        color: var(--theme-text, #f6f6f6);
        box-shadow: 0 4px 16px var(--theme-shadow, rgba(0, 0, 0, 0.5));
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
        font-family: "Noto Sans JP";
        font-weight: 600;
        font-size: 0.85rem;
        font-variant-numeric: tabular-nums;
    }
</style>