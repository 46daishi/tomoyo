<script>
    import { page } from '$app/state';
    import { goto } from '$app/navigation';
    import { fly } from 'svelte/transition';

    import AppImage from '$lib/components/AppImage.svelte';
    import Toast from '$lib/components/Toast.svelte';
    import { ICONS } from '$lib/icons';
    import { session } from '$lib/stores/session.svelte.js';

    let path = $derived(page.url.pathname.replace(/\/+$/, '') || '/');
    let onActiveMedia = $derived(
        session.mediaId != null && path === `/media/${session.mediaId}`
    );
    let visible = $derived(session.active && !onActiveMedia);
    let label = $derived(
        session.mediaTitle ?? (session.mediaId != null ? `Media #${session.mediaId}` : 'Reading')
    );

    $effect(() => {
        if (!session.notice) return;
        const timeout = setTimeout(() => session.clearNotice(), 3200);
        return () => clearTimeout(timeout);
    });

    function openMedia() {
        if (session.mediaId == null) return;
        goto(`/media/${session.mediaId}`);
    }
</script>

{#if visible}
    <div
        class="session-pill"
        class:paused={session.paused}
        transition:fly={{ y: 10, duration: 180 }}
    >
        <button
            type="button"
            class="pill-main"
            onclick={openMedia}
            title="Return to the reading session"
        >
            <span
                class="pill-cover"
                style={`--cover-color: ${session.mediaColor ?? 'var(--theme-primary, #36b7bd)'}`}
                aria-hidden="true"
            >
                <span class="pill-cover-icon nf">{ICONS.book}</span>
                {#if session.mediaCoverPath}
                    <AppImage
                        path={session.mediaCoverPath}
                        alt=""
                        class="pill-cover-img"
                        draggable="false"
                    />
                {/if}
            </span>
            <span class="pill-text">
                <span class="pill-title">{label}</span>
                <span class="pill-time">
                    {session.formattedTime}{session.paused ? ' · paused' : ''}
                </span>
            </span>
        </button>

        <div class="pill-actions">
            <button
                type="button"
                class="pill-btn"
                onclick={() => session.togglePlayback()}
                title={session.running ? 'Pause session' : 'Resume session'}
                aria-label={session.running ? 'Pause session' : 'Resume session'}
            >
                <span class="nf" aria-hidden="true">{ICONS[session.running ? 'pause' : 'play']}</span>
            </button>
            <button
                type="button"
                class="pill-btn"
                onclick={() => session.stop()}
                title="Finish session"
                aria-label="Finish session"
            >
                <span class="nf" aria-hidden="true">{ICONS.check}</span>
            </button>
        </div>
    </div>
{/if}

<Toast message={session.notice} />

<style>
    .session-pill {
        position: fixed;
        left: 1rem;
        bottom: 1rem;
        z-index: 1500;
        display: flex;
        align-items: center;
        gap: 0.2rem;
        max-width: min(calc(100vw - 2rem), 360px);
        padding: 0.3rem;
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 92%, #000);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 14px;
        box-shadow: 0 6px 18px rgba(0, 0, 0, 0.35);
    }

    .pill-main {
        display: flex;
        align-items: center;
        gap: 0.6rem;
        min-width: 0;
        padding: 0.25rem 0.45rem 0.25rem 0.25rem;
        border: none;
        border-radius: 10px;
        background: transparent;
        color: inherit;
        font-family: inherit;
        text-align: left;
        cursor: pointer;
        transition: background-color 0.15s ease;
    }

    .pill-main:hover {
        background: color-mix(in srgb, var(--theme-text, #f6f6f6) 8%, transparent);
    }

    .pill-cover {
        position: relative;
        display: grid;
        place-items: center;
        flex: none;
        width: 34px;
        height: 34px;
        overflow: hidden;
        border-radius: 9px;
        background: var(--cover-color);
        color: #ffffff;
    }

    .pill-cover-icon {
        font-size: 14px;
        opacity: 0.9;
    }

    :global(.pill-cover-img) {
        position: absolute;
        inset: 0;
        width: 100%;
        height: 100%;
        object-fit: cover;
    }

    .pill-text {
        display: flex;
        flex-direction: column;
        align-items: flex-start;
        min-width: 0;
        line-height: 1.2;
    }

    .pill-title {
        max-width: 180px;
        overflow: hidden;
        text-overflow: ellipsis;
        white-space: nowrap;
        color: var(--theme-text, #f6f6f6);
        font-size: 0.82rem;
        font-weight: 600;
    }

    .pill-time {
        margin-top: 0.15rem;
        color: var(--theme-textSecondary, #b3b3b3);
        font-size: 0.72rem;
        font-variant-numeric: tabular-nums;
    }

    .session-pill.paused .pill-time {
        opacity: 0.7;
    }

    .pill-actions {
        display: flex;
        gap: 2px;
        margin-left: 2px;
        padding-left: 4px;
        border-left: 1px solid var(--theme-border, #404040);
    }

    .pill-btn {
        display: grid;
        place-items: center;
        width: 30px;
        height: 30px;
        padding: 0;
        border: none;
        border-radius: 8px;
        background: transparent;
        color: var(--theme-textSecondary, #b3b3b3);
        cursor: pointer;
        transition: background-color 0.15s ease, color 0.15s ease;
    }

    .pill-btn:hover {
        background: color-mix(in srgb, var(--theme-text, #f6f6f6) 10%, transparent);
        color: var(--theme-text, #f6f6f6);
    }

    .pill-btn .nf {
        font-size: 13px;
    }

    :global(body.mini-mode) .session-pill {
        display: none;
    }
</style>
