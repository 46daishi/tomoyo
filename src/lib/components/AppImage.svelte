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
    <img src={src} {alt} {...rest} />
{/if}
