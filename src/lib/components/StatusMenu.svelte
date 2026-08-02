<script>
    import { portal } from '$lib/actions/portal.js';

    let { x, y, levels, current, onSelect, onClose } = $props();

    let menuEl = $state(null);
    let style = $state(`left: ${x}px; top: ${y}px;`);

    // Clamp into the viewport once we know the menu's real size — the
    // initial x/y is just "next to the status bar" and can overflow the
    // window near the right/bottom edge.
    $effect(() => {
        if (!menuEl) return;
        const rect = menuEl.getBoundingClientRect();
        const padding = 8;
        let left = x;
        let top = y;

        if (left + rect.width > window.innerWidth - padding) {
            left = Math.max(padding, x - rect.width - 16); // flip to the other side
        }
        if (top + rect.height > window.innerHeight - padding) {
            top = Math.max(padding, window.innerHeight - rect.height - padding);
        }

        style = `left: ${left}px; top: ${top}px;`;
    });

    function handlePointerDown(e) {
        if (menuEl && !menuEl.contains(e.target)) {
            onClose();
        }
    }

    function handleKeydown(e) {
        if (e.key === 'Escape') onClose();
    }
</script>

<svelte:window onpointerdown={handlePointerDown} onkeydown={handleKeydown} />

<div class="status-menu" {style} bind:this={menuEl} use:portal role="menu">
    {#each levels as level, i}
        <button
            type="button"
            role="menuitemradio"
            aria-checked={i === current}
            class="status-menu-item"
            class:active={i === current}
            onclick={() => onSelect(i)}
        >
            <span class="status-dot" style={`--dot-color: ${level.color}`}>{i}</span>
            {level.label}
        </button>
    {/each}
</div>

<style>
    .status-menu {
        position: fixed;
        z-index: 1000;
        min-width: 160px;
        background: var(--theme-surface, #2d2d2d);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 10px;
        padding: 0.35rem;
        box-shadow: 0 12px 28px rgba(0, 0, 0, 0.45);
        display: flex;
        flex-direction: column;
        gap: 0.1rem;
    }

    .status-menu-item {
        display: flex;
        align-items: center;
        gap: 0.6rem;
        background: none;
        border: none;
        border-radius: 6px;
        padding: 0.5rem 0.65rem;
        font: inherit;
        font-size: 0.85rem;
        color: var(--theme-text, #f6f6f6);
        text-align: left;
        cursor: pointer;
        transition: background 0.12s ease;
    }

    .status-menu-item:hover {
        background: color-mix(in srgb, var(--theme-primary, #36b7bd) 15%, transparent);
    }

    .status-menu-item.active {
        font-weight: 700;
        background: color-mix(in srgb, var(--theme-primary, #36b7bd) 10%, transparent);
    }

    .status-dot {
        width: 1.3rem;
        height: 1.3rem;
        border-radius: 50%;
        background: var(--dot-color, #6c7086);
        flex-shrink: 0;
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 0.7rem;
        font-weight: 800;
        color: black;
        text-shadow: 0 1px 0 rgba(255, 255, 255, 0.25);
    }
</style>