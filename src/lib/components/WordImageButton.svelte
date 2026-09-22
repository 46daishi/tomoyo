<script>
    import { pickWordImage, pasteWordImageFromClipboard, coverSrc } from '$lib/db';
    import { updateWordImage } from '$lib/dictionary.js';
    import { ICONS } from '$lib/icons';

    import { onDestroy, onMount } from 'svelte';

    let { wordId, imagePath = null, onSaved, disabled = false, small = false, fixedPreview = false } = $props();

    let busy = $state(false);
    let error = $state(null);
    let errorTimer = null;
    let wrapEl = $state(null);
    let portalEl = null;
    let previewPoller = null;

    // Native <select> preselects its first option, which would swallow the
    // first choice's change event. Deselect everything so picking any option
    // (including the first) fires onchange.
    function deselect(node) {
        node.selectedIndex = -1;
    }

    function showError(message) {
        error = message;
        clearTimeout(errorTimer);
        errorTimer = setTimeout(() => (error = null), 2500);
    }

    async function savePath(path) {
        await updateWordImage({ wordId, imagePath: path });
        onSaved?.(path);
    }

    async function handlePickFile() {
        busy = true;
        try {
            const path = await pickWordImage();
            if (path) await savePath(path);
        } catch (e) {
            console.error('word image pick failed:', e);
            showError('Could not pick image.');
        } finally {
            busy = false;
        }
    }

    async function handlePaste() {
        busy = true;
        try {
            const path = await pasteWordImageFromClipboard();
            if (path) await savePath(path);
            else showError('No image in clipboard.');
        } catch (e) {
            console.error('word image paste failed:', e);
            showError('Could not read clipboard image.');
        } finally {
            busy = false;
        }
    }

    async function handleRemove() {
        busy = true;
        try {
            await savePath(null);
        } catch (e) {
            console.error('word image remove failed:', e);
            showError('Could not remove image.');
        } finally {
            busy = false;
        }
    }

    function handleSelect(value) {
        if (value === 'file') handlePickFile();
        else if (value === 'paste') handlePaste();
        else if (value === 'remove') handleRemove();
    }

    // The lookup tooltip is a scroll container and carries a fly transition,
    // so a position:fixed descendant ends up trapped by it and clipped away.
    // Portal the enlarged preview straight onto <body> instead: nothing above
    // it can clip or reposition it. JS only writes coordinates and the
    // visibility class; the fade/flip visuals stay in CSS.
    function ensurePortal() {
        if (!portalEl) {
            portalEl = document.createElement('div');
            portalEl.className = 'word-image-preview-portal';
            portalEl.setAttribute('aria-hidden', 'true');
            document.body.appendChild(portalEl);
        }
        return portalEl;
    }

    function syncPortalImage() {
        if (!fixedPreview) return;
        const portal = ensurePortal();
        portal.replaceChildren();
        if (!imagePath) return;
        const img = document.createElement('img');
        img.src = coverSrc(imagePath);
        img.alt = '';
        portal.appendChild(img);
    }

    function positionPortal() {
        if (!fixedPreview || !wrapEl) return;
        const portal = ensurePortal();
        const rect = wrapEl.getBoundingClientRect();
        const width = 224;
        const left = Math.max(8, Math.min(rect.left, window.innerWidth - width - 8));
        const flip = rect.top >= 300; // room above -> open upwards
        portal.style.left = `${left}px`;
        portal.style.top = `${flip ? rect.top : rect.bottom}px`;
        portal.classList.toggle('flip', flip);
    }

    function showPortal(show) {
        if (!fixedPreview) return;
        ensurePortal().classList.toggle('visible', show && !!imagePath);
    }

    onMount(() => {
        if (!fixedPreview) return;
        syncPortalImage();
        positionPortal();
        previewPoller = setInterval(positionPortal, 250);
    });

    $effect(() => {
        if (fixedPreview) syncPortalImage();
    });

    onDestroy(() => {
        clearTimeout(errorTimer);
        clearInterval(previewPoller);
        portalEl?.remove();
        portalEl = null;
    });
</script>

<div
    class="word-image-wrap"
    bind:this={wrapEl}
    onmouseenter={() => showPortal(true)}
    onmouseleave={() => showPortal(false)}
>
    <button
        type="button"
        class="word-image-btn"
        class:has-image={!!imagePath}
        class:small={small}
        disabled={disabled || busy}
        onclick={(e) => e.stopPropagation()}
    >
        {#if imagePath}
            <img src={coverSrc(imagePath)} alt="" />
        {:else}
            <span class="word-image-icon">{@html ICONS.image}</span>
        {/if}
    </button>
    <select
        class="word-image-native"
        aria-label="Word image options"
        disabled={disabled || busy}
        use:deselect
        onchange={(e) => {
            const value = e.currentTarget.value;
            e.currentTarget.selectedIndex = -1;
            e.currentTarget.blur();
            handleSelect(value);
        }}
    >
        <option value="file">Choose file…</option>
        <option value="paste">Paste from clipboard</option>
        {#if imagePath}
            <option value="remove">Remove</option>
        {/if}
    </select>
    {#if imagePath && !fixedPreview}
        <div class="word-image-preview" aria-hidden="true">
            <img src={coverSrc(imagePath)} alt="" />
        </div>
    {/if}
    {#if error}
        <div class="word-image-error-bubble">{error}</div>
    {/if}
</div>

<style>
    .word-image-wrap {
        position: relative;
        display: inline-flex;
        flex-shrink: 0;
        vertical-align: middle;
    }

    .word-image-btn {
        width: 2.5rem;
        height: 2.5rem;
        border-radius: 8px;
        border: 1.5px dashed var(--theme-border, #404040);
        background: transparent;
        color: var(--theme-textSecondary, #b3b3b3);
        cursor: pointer;
        padding: 0;
        overflow: hidden;
        display: flex;
        align-items: center;
        justify-content: center;
        font-size: 1rem;
        transition: border-color 0.15s ease, color 0.15s ease;
        font-family: "Symbols Nerd Font";
    }

    .word-image-btn:hover:not(:disabled) {
        border-color: var(--theme-primary, #36b7bd);
        color: var(--theme-primary, #36b7bd);
    }

    .word-image-btn:disabled {
        opacity: 0.5;
        cursor: default;
    }

    .word-image-btn.has-image {
        border-style: solid;
    }

    .word-image-btn.small {
        width: 1.95rem;
        height: 1.95rem;
        border-radius: 7px;
        font-size: 0.8rem;
        border-width: 1px;
    }

    .word-image-btn img {
        width: 100%;
        height: 100%;
        object-fit: cover;
        display: block;
    }

    /* Enlarged preview: appears after holding hover briefly, vanishes on
       unhover. pointer-events none so it never steals the hover itself. */
    .word-image-preview {
        position: absolute;
        z-index: 60;
        bottom: calc(100% + 8px);
        left: 0;
        width: 14rem;
        max-width: min(70vw, 20rem);
        border-radius: 12px;
        overflow: hidden;
        border: 1px solid var(--theme-border, #404040);
        box-shadow: 0 12px 32px rgba(0, 0, 0, 0.5);
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 95%, #000);
        opacity: 0;
        transform: scale(0.92);
        transform-origin: bottom left;
        pointer-events: none;
        transition:
            opacity 0.18s ease 0.5s,
            transform 0.18s ease 0.5s;
    }

    .word-image-preview img {
        width: 100%;
        max-height: 16rem;
        object-fit: contain;
        display: block;
    }

    .word-image-wrap:hover .word-image-preview {
        opacity: 1;
        transform: scale(1);
    }

    .word-image-error-bubble {
        position: absolute;
        z-index: 60;
        top: calc(100% + 4px);
        left: 0;
        white-space: nowrap;
        font-size: 0.78rem;
        color: #f38ba8;
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 95%, #000);
        border: 1px solid var(--theme-border, #404040);
        border-radius: 8px;
        padding: 0.3rem 0.6rem;
        box-shadow: 0 8px 24px rgba(0, 0, 0, 0.4);
        pointer-events: none;
    }

    /* Invisible native select overlaying the button (same pattern as the
       mine button): a click opens the system dropdown, onchange fires. */
    .word-image-native {
        position: absolute;
        top: 0;
        left: 0;
        width: 100%;
        height: 100%;
        margin: 0;
        opacity: 0;
        cursor: pointer;
        border: none;
        background: transparent;
    }
</style>
