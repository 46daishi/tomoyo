<script>
    import { loadImageSrc } from '$lib/db.js';

    let { path, alt = '', ...rest } = $props();

    let src = $state(/** @type {string | null} */ (null));

    $effect(() => {
        src = null;
        if (!path) return;
        let alive = true;
        loadImageSrc(path).then((url) => {
            if (alive) src = url;
        });
        return () => {
            alive = false;
        };
    });
</script>

{#if src}
    <!-- No entrance animation on purpose: on compositors that strobe
       transitions, a fade per image is worse than an instant paint.
       loading="lazy" keeps offscreen grid images out of the page-load
       decode burst (the "lags on page load" spike). -->
    <img src={src} {alt} {...rest} loading="lazy" decoding="async" />
{/if}
