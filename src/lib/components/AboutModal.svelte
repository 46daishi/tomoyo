<script>
    import { ICONS } from '$lib/icons.js';
    import { openUrl } from '@tauri-apps/plugin-opener';

    let { show = $bindable(false) } = $props();

    const VERSION = '0.1.0';
    const VERSION_DATE = 'August 10, 2026';
    const LINKS = [
        { label: 'tomoyo on GitHub', url: 'https://github.com/46daishi/tomoyo', icon: ICONS.github, iconClass: 'github-icon' },
        { label: '46dai X (Twitter)', url: 'https://x.com/46daishi', icon: ICONS.twitter },
    ];

    function close() {
        show = false;
    }

    async function openLink(url) {
        try {
            await openUrl(url);
        } catch (err) {
            console.error('Failed to open link:', err);
        }
    }
</script>

{#if show}
    <div class="modal-overlay" onclick={close}>
        <div class="modal about-modal" onclick={(e) => e.stopPropagation()}>
            <div class="about-header">
                <h3 class="modal-title">tomoyo</h3>
                <span class="credit">Made by <strong>46dai</strong></span>
            </div>

            <div class="about-body">
                <img class="about-logo" src="/tomoyo_full.png" alt="tomoyo" />

                <div class="about-info">
                    <div class="about-version">
                        <span class="version-label">Version</span>
                        <div class="version-row">
                            <span class="version-value">{VERSION}</span>
                            <span class="version-date">{VERSION_DATE}</span>
                        </div>
                    </div>
                    <div class="about-links">
                        {#each LINKS as link (link.url)}
                            <div class="link-row">
                                <span class="link-icon {link.iconClass}">{link.icon}</span>
                                <button class="about-link" onclick={() => openLink(link.url)}>
                                    {link.label}
                                </button>
                            </div>
                        {/each}
                    </div>
                </div>
            </div>

            <div class="modal-actions">
                <button class="modal-btn" onclick={close}>Close</button>
            </div>
        </div>
    </div>
{/if}

<style>
    .about-modal {
        width: 480px;
        max-width: min(480px, 90vw);
        align-items: stretch;
        gap: 1.1rem;
    }

    .about-modal .modal-title {
        text-align: left;
        padding-bottom: 1rem;
        border-bottom: 1px solid color-mix(in srgb, var(--theme-border, #404040) 70%, transparent);
    }

    .about-header {
        display: flex;
        align-items: baseline;
        justify-content: space-between;
        gap: 1rem;
        padding-bottom: 1rem;
        border-bottom: 1px solid color-mix(in srgb, var(--theme-border, #404040) 70%, transparent);
    }

    .about-header .modal-title {
        padding-bottom: 0;
        border-bottom: none;
    }

    .credit {
        font-size: 0.82rem;
        color: var(--theme-textSecondary, #b3b3b3);
        letter-spacing: 0.01em;
        white-space: nowrap;
    }

    .credit strong {
        color: var(--theme-text, #f6f6f6);
        font-weight: 700;
    }

    .about-body {
        display: flex;
        gap: 1.5rem;
        align-items: center;
    }

    .about-logo {
        width: 96px;
        flex-shrink: 0;
        border-radius: 12px;
    }

    .about-info {
        display: flex;
        flex-direction: column;
        gap: 1.25rem;
        min-width: 0;
    }

    .about-version {
        display: grid;
        grid-template-columns: auto 1fr;
        grid-template-rows: auto auto;
        column-gap: 1rem;
        row-gap: 0.15rem;
    }

    .version-label {
        font-size: 0.7rem;
        text-transform: uppercase;
        letter-spacing: 0.06em;
        color: var(--theme-textSecondary, #b3b3b3);
        font-weight: 600;
    }

    .version-row {
        grid-column: 1 / -1;
        display: flex;
        align-items: baseline;
        gap: 0.6rem;
    }

    .version-value {
        font-size: 1.15rem;
        font-weight: 700;
        color: var(--theme-text, #f6f6f6);
    }

    .version-date {
        font-size: 0.8rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .about-links {
        display: flex;
        flex-direction: column;
        gap: 0.55rem;
    }

    .link-row {
        display: flex;
        align-items: center;
        gap: 0.7rem;
    }

    .link-icon {
        font-family: "Symbols Nerd Font";
        font-size: 1.3rem;
        width: 1.5rem;
        flex-shrink: 0;
        text-align: center;
        line-height: 1;
    }

    .link-icon.github-icon {
        font-size: 1.55rem;
    }

    .about-link {
        background: none;
        border: none;
        padding: 0;
        font: inherit;
        cursor: pointer;
        color: var(--theme-primary, #36b7bd);
        font-weight: 600;
        font-size: 0.95rem;
        transition: color 0.15s ease;
    }

    .about-link:hover {
        color: var(--theme-primaryHover, #17a4ab);
        text-decoration: underline;
    }
</style>
