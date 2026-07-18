<script>
  import ActionButton from "$lib/components/ActionButton.svelte";
  import { ICONS } from "$lib/icons.js";

</script>

<main class="page home">
</main>

<div class="logo">
    <a href="https://x.com/46daishi" target="_blank" rel="noopener noreferrer"><img src="tomoyo_full.png" alt="tomoyo" /></a>
</div>
<nav class="side-nav" aria-label="App navigation">
  <div class="nav-actions">
      <ActionButton
          icon={ICONS.settings}
          variant="secondary"
          size="small"
      />
      <ActionButton
          icon={ICONS.stats}
          variant="secondary"
          size="small"
      />
      <ActionButton
          icon={ICONS.unmute}
          variant="secondary"
          size="small"
      />
      <ActionButton
          icon={ICONS.plus}
          variant="secondary"
          size="small"
      />
  </div>
</nav>

<style>
    .home {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: flex-start;
        text-align: center;
        gap: 1rem;

        transform: scale(var(--page-scale, 1));
        transform-origin: top center;
        height: 100vh;
        overflow: hidden;
    }

    h1 {
        font-size: 1.7em;
        display: flex;
        align-items: center;
        gap: 0.4rem;
        margin-top: 15vh;
        margin-bottom: 0;
    }

    .session-label {
        font-size: 1.2em;
        margin: 0;
        display: flex;
        align-items: center;
        gap: 0.4rem;
    }

    .timer-controls {
        display: flex;
        gap: 1rem;
        align-items: center;
    }

    .profile-picker {
        margin-top: 1.5rem;
        transition: opacity 0.2s ease;
    }

    .logo img {
        width: 50px;
        height: 50px;
        object-fit: contain;
    }

    /* Shared pill button style (edit + extend) */
    .edit-btn {
        display: flex;
        align-items: center;
        justify-content: center;
        background-color: var(--theme-surface, #2d2d2d);
        color: var(--theme-text, #ffffff);
        border: 1px solid transparent;
        border-radius: 100px;
        padding: 0.3em 0.4em;
        font-size: 0.45em;
        cursor: pointer;
        box-shadow: 0 4px 8px var(--theme-shadow, rgba(0, 0, 0, 0.3));
        transition: all 0.3s ease;
        transform: translateY(1px);
    }

    .edit-btn:hover:not(:disabled) {
        border-color: var(--theme-primary, #36b7bd);
        background-color: var(--theme-button, #1a1a1a);
        transform: translateY(0px);
    }

    .edit-btn:active {
        transform: translateY(1px);
    }

    .edit-btn:disabled {
        opacity: 0.5;
        cursor: not-allowed;
        transform: none;
    }

    .extend-btn {
        font-size: 0.55em;
    }

    .name-input {
        background: transparent;
        border: none;
        border-bottom: 2px solid var(--theme-primary, #36b7bd);
        color: var(--theme-text, #ffffff);
        font-size: 1.7em;
        font-weight: bold;
        font-family: inherit;
        text-align: center;
        width: 16ch;
        outline: none;
        margin-top: 15vh;
    }

    .add-modal {
        width: 400px;
    }

    .ext-input {
        font-size: 2rem;
        font-weight: 700;
        width: 5ch;
    }

    .add-tabs {
        display: flex;
        background: var(--theme-background, #1a1a1a);
        border-radius: 12px;
        padding: 4px;
        width: 100%;
        gap: 4px;
    }

    .add-tab {
        flex: 1;
        border: none;
        border-radius: 10px;
        background: transparent;
        color: var(--theme-textSecondary, #b3b3b3);
        font-size: 0.85rem;
        font-weight: 600;
        font-family: inherit;
        padding: 0.45rem 0;
        cursor: pointer;
        transition: background 0.2s, color 0.2s;
    }

    .add-tab.active {
        background: var(--theme-surface, #2d2d2d);
        color: var(--theme-text, #f6f6f6);
        box-shadow: 0 1px 4px rgba(0, 0, 0, 0.25);
    }

    .add-row {
        display: flex;
        align-items: center;
        gap: 0.5rem;
        width: 100%;
    }

    .add-minutes {
        width: 4.5rem;
    }

    .add-name {
        flex: 1;
    }

    .modal-unit {
        color: var(--theme-textSecondary, #b3b3b3);
        font-size: 0.9rem;
        font-weight: 600;
        margin-right: 0.25rem;
    }

    @media (max-height: 372px) {
        .profile-picker {
            opacity: 0;
            pointer-events: none;
        }

        
    }

    @media (max-height: 450px) {
        .credit {
            opacity: 0 !important;
            pointer-events: none !important;
        }
    }

    /* ── Credit footer ───────────────────────────────────────────────────── */
    .credit {
        position: fixed;
        bottom: 1.2rem;
        left: 50%;
        transform: translateX(-50%) translateY(0);
        display: flex;
        align-items: center;
        gap: 0.35em;
        font-size: 0.8rem;
        color: var(--theme-textSecondary, #b3b3b3);
        text-decoration: none;
        opacity: 0.30;
        transition: opacity 0.3s ease, transform 0.3s ease;
        white-space: nowrap;
        pointer-events: auto;
    }

    .credit:hover {
        opacity: 1;
        transform: translateX(-50%) translateY(-3px);
    }

    .credit-icon {
        width: 14px;
        height: 14px;
        object-fit: contain;
        opacity: 0.8;
        border-radius: 3px;
    }
</style>
