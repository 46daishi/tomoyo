<script>
    import { onMount, tick } from 'svelte';

    /**
     * Overlapping pill tab strip (All + media categories).
     * @type {{ items: Array<{ value: string, label: string, icon: string, abr?: string }>, active: string, onSelect: (value: string) => void }}
     */
    let { items, active, onSelect } = $props();

    let trackEl = $state(/** @type {HTMLElement|null} */ (null));
    // Indicator geometry in track coordinates. Measured from the live
    // buttons (which carry the overlap margins), so it always matches.
    let indicator = $state({ x: 0, w: 0 });
    let indicatorReady = $state(false);

    function measure() {
        if (!trackEl) return;
        const buttons = trackEl.querySelectorAll(':scope > .category-pill');
        const idx = items.findIndex((it) => it.value === active);
        const el = idx >= 0 ? /** @type {HTMLElement|null} */ (buttons[idx]) : null;
        if (!el) return;
        // offsetLeft counts the track's own border, but the absolutely
        // positioned indicator starts inside it: subtract clientLeft so
        // both resolve in padding-box coordinates (otherwise the fill
        // sits ~1px right and the pill looks cut off on its left edge).
        // Round outward (floor left, ceil width + 1px into the neighbour
        // overlap, where the indicator hides behind the buttons anyway)
        // so subpixel layouts can't leave a sliver either.
        const rawX = el.offsetLeft - trackEl.clientLeft;
        indicator = { x: Math.floor(rawX), w: Math.ceil(el.offsetWidth) + 1 };
        if (!indicatorReady) {
            // First measure lands after first paint: show instantly
            // instead of gliding in from the corner.
            indicatorReady = true;
        }
    }

    /** @param {KeyboardEvent} e @param {number} index */
    function handleKeydown(e, index) {
        if (e.key !== 'ArrowRight' && e.key !== 'ArrowLeft') return;
        e.preventDefault();
        const next = (index + (e.key === 'ArrowRight' ? 1 : items.length - 1)) % items.length;
        onSelect(items[next].value);
        document.getElementById(`category-pill-${items[next].value}`)?.focus();
    }

    $effect(() => {
        active;
        tick().then(measure);
    });

    onMount(() => {
        measure();
        if (document.fonts?.ready) {
            document.fonts.ready.then(() => measure()).catch(() => {});
        }
        const ro = new ResizeObserver(() => measure());
        if (trackEl) ro.observe(trackEl);
        return () => ro.disconnect();
    });
</script>

<div class="category-bar" role="tablist" aria-label="Media categories" bind:this={trackEl}>
    <div
        class="category-indicator"
        class:ready={indicatorReady}
        aria-hidden="true"
        style={`transform: translateX(${indicator.x}px); width: ${indicator.w}px;`}
    ></div>
    {#each items as item, i}
        <button
            type="button"
            id={`category-pill-${item.value}`}
            role="tab"
            aria-selected={active === item.value}
            class="category-pill"
            class:active={active === item.value}
            onclick={() => onSelect(item.value)}
            onkeydown={(e) => handleKeydown(e, i)}
            tabindex={active === item.value ? 0 : -1}
        >
            <span class="pill-icon" aria-hidden="true">{@html item.icon}</span>
            <span>{item.abr || item.label}</span>
        </button>
    {/each}
</div>

<style>
    .category-bar {
        position: relative;
        display: flex;
        align-items: stretch;
        width: max-content;
        max-width: 100%;
        margin: 0 auto;
        padding: 0.3rem;
        overflow-x: auto;
        background: var(--theme-surface, #2d2d2d);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 999px;
    }

    /* Sliding active fill: measured to the active pill, glides on
       transform/width. Hidden until the first measure so it never
       sweeps in from the corner on load. */
    .category-indicator {
        position: absolute;
        top: 0.3rem;
        bottom: 0.3rem;
        left: 0;
        border-radius: 999px;
        background-color: var(--theme-primary, #36b7bd);
        /* Elevation without outer overflow: an outer glow larger than
           the track padding spills into unreachable scroll space and
           reads as clipped (worst on the first pill). Inset edge +
           a shadow that fits inside the padding instead. */
        box-shadow:
            inset 0 0 0 1px rgba(255, 255, 255, 0.22),
            0 1px 2px rgba(0, 0, 0, 0.35);
        opacity: 0;
        transition:
            transform 0.25s ease,
            width 0.25s ease,
            opacity 0.15s ease;
        pointer-events: none;
    }

    .category-indicator.ready {
        opacity: 1;
    }

    @media (prefers-reduced-motion: reduce) {
        .category-indicator {
            transition: none;
        }
    }

    .category-pill {
        position: relative;
        display: inline-flex;
        align-items: center;
        gap: 0.45rem;
        margin-left: -0.55rem;
        padding: 0.45rem 1.05rem 0.45rem 1.05rem;
        border: 1px solid transparent;
        border-radius: 999px;
        background: transparent;
        color: var(--theme-textSecondary, #b3b3b3);
        font: inherit;
        font-size: 0.85rem;
        font-weight: 600;
        white-space: nowrap;
        cursor: pointer;
        transition:
            background-color 0.15s ease,
            color 0.15s ease,
            border-color 0.15s ease,
            box-shadow 0.15s ease;
    }

    /* First *button* (not first child — the indicator div sits before
       the pills, so :first-child never matches and the leading pill
       used to jut out past the track edge, clipped). */
    .category-pill:first-of-type {
        margin-left: 0;
        padding-left: 0.65rem;
    }

    .category-pill:hover:not(.active) {
        z-index: 1;
        color: var(--theme-text, #f6f6f6);
    }

    .category-pill.active {
        z-index: 2;
        background-color: transparent;
        border-color: transparent;
        color: #ffffff;
    }

    .category-pill:focus-visible {
        outline: 2px solid var(--theme-primary, #36b7bd);
        outline-offset: 1px;
        z-index: 3;
    }

    .pill-icon {
        display: inline-flex;
        font-family: "Symbols Nerd Font";
        font-size: 0.85em;
        line-height: 1;
    }

    .pill-icon :global(svg) {
        width: 0.9em;
        height: 0.9em;
    }
</style>
