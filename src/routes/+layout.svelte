<script>
    import "../app.css";
    import { onMount, onDestroy } from "svelte";
    import { page } from "$app/stores";
    import { get } from "svelte/store";
    import { initializeTheme } from "$lib/stores/themes.js";
    import { discordRPC } from "$lib/rpc.js";
    import { presenceState } from "$lib/stores/presence.svelte.js";
    import {
        PRESENCE_DEFAULTS,
        PRESENCE_DETAILS,
        PRESENCE_ICONS,
    } from "$lib/defaults/discord.js";

    let discordEnabled = true;
    let isConnecting = false;
    let unsubPage = () => {};

    /** @param {string} path */
    function routePresence(path) {
        if (path.startsWith("/media")) {
            return {
                details: PRESENCE_DETAILS.mediaDetails,
                smallImage: PRESENCE_ICONS.immersionIcon,
                status: presenceState.mediaTitle ?? 'Default Title',
            };
        }
        return {};
    }

    /** @param {string} path */
    async function setPresence(path) {
        if (!discordEnabled) return;
        try {
            await discordRPC.updatePresence({
                ...PRESENCE_DEFAULTS,
                endTimestamp: undefined,
                ...routePresence(path),
            });
        } catch (e) {
            console.warn("Discord presence update failed:", e);
        }
    }

    /** Connect to Discord RPC and set presence for the current route. */
    async function enableDiscord(path) {
        if (isConnecting) return;
        isConnecting = true;
        try {
            await discordRPC.connect();
            await setPresence(path);
        } catch (e) {
            console.warn("Discord RPC connect failed:", e);
        } finally {
            isConnecting = false;
        }
    }

    // Reactively update Discord RPC whenever mediaTitle or page URL changes
    $effect(() => {
        const currentPath = $page.url.pathname;
        const _title = presenceState.mediaTitle; // Subscribes to changes

        if (discordEnabled) {
            setPresence(currentPath);
        }
    });

    onMount(async () => {
        initializeTheme();
        const currentPath = get(page).url.pathname;

        if (discordEnabled) {
            await enableDiscord(currentPath);
        }
    });

    onDestroy(() => {
        unsubPage();
    });
</script>

<slot />