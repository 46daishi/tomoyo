<script>
    import "../app.css";
    import { onMount, onDestroy } from "svelte";
    import { initializeTheme } from "$lib/stores/themes.js";

    onMount(async () => {
        initializeTheme();
    });

    onDestroy(async () => {
    });
</script>

<slot />