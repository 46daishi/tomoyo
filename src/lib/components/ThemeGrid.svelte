<script>
    import { currentTheme, themeList, applyTheme } from "$lib/stores/themes.js";

    let active = $derived($currentTheme);

    function select(key) {
        applyTheme(key);
    }
</script>

<div class="theme-grid">
    {#each themeList as theme (theme.value)}
        <button
            class="swatch"
            class:active={active === theme.value}
            title={theme.label}
            onclick={() => select(theme.value)}
            aria-pressed={active === theme.value}
            aria-label="Select {theme.label} theme"
        >
            <span class="half" style="background:{theme.colors.background}"></span>
            <span class="half" style="background:{theme.colors.primary}"></span>
        </button>
    {/each}
</div>

<style>
    .theme-grid {
        display: flex;
        flex-wrap: wrap;
        gap: 0.5rem;
        max-width: 450px;
    }

    .swatch {
        display: flex;
        width: 36px;
        height: 24px;
        border-radius: 6px;
        border: 1px solid var(--theme-border, #404040);
        overflow: hidden;
        cursor: pointer;
        padding: 0;
        transition: transform 0.15s, box-shadow 0.15s;
    }

    .swatch:hover {
        transform: scale(1.1);
        box-shadow: 0 2px 8px rgba(0, 0, 0, 0.35);
    }

    .swatch.active {
        border-color: var(--theme-primary, #36b7bd);
        box-shadow: 0 0 0 1px var(--theme-primary, #36b7bd);
    }

    .half {
        flex: 1;
        display: block;
    }
</style>