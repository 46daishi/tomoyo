<script>
    import "../app.css";
    import { onMount, onDestroy } from "svelte";
    import { page } from "$app/stores";
    import { afterNavigate } from "$app/navigation";
    import { initializeTheme } from "$lib/stores/themes.js";
    import { discordRPC } from "$lib/rpc.js";
    import { discordEnabled } from "$lib/stores/discordSettings.js";
    import { loadSettings } from "$lib/settings.js";
    import { presenceState } from "$lib/stores/presence.svelte.js";
    import { recoverDanglingSessions } from '$lib/sessions.js';
    import {
        PRESENCE_DEFAULTS,
        PRESENCE_DETAILS,
        PRESENCE_ICONS,
    } from "$lib/defaults/discord.js";

    let { children } = $props();
    let isInitialized = $state(false);

    /** @param {string} path */
    function routePresence(path) {
        if (path.startsWith("/media")) {
            return {
                details: PRESENCE_DETAILS.mediaDetails,
                smallImage: PRESENCE_ICONS.immersionIcon,
                status: presenceState.mediaTitle ?? "Default Title",
            };
        }
        if (path.startsWith("/settings")) {
            return {
                details: PRESENCE_DETAILS.settingsDetails,
            };
        }
        if (path.startsWith("/dictionary")){
          return {
            details: PRESENCE_DETAILS.dictionaryDetails,
            smallImage: PRESENCE_ICONS.dictionaryIcon,
          }
        }
        return {};
    }

    /** @param {string} path */
    async function setPresence(path) {
        if (!$discordEnabled || !discordRPC.connected) return;

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

    async function enableDiscord(path) {
        if (discordRPC.connected) {
            await setPresence(path);
            return;
        }

        try {
            await discordRPC.connect();
            await setPresence(path);
        } catch (e) {
            console.warn("Discord RPC connect failed:", e);
        }
    }

    onMount(async () => {
        initializeTheme();
        await recoverDanglingSessions();

        // 1. Fetch settings and update store
        const settings = await loadSettings();
        discordEnabled.set(settings.discord_rpc_enabled);

        // 2. Mark initialized so reactive effect takes over (prevents duplicate call)
        isInitialized = true;
    });

    onDestroy(() => {
        if (discordRPC.connected) {
            discordRPC.disconnect().catch(console.warn);
        }
    });

    // Update presence on page navigation
    afterNavigate((navigation) => {
        if (!isInitialized) return;
        const targetPath = navigation.to?.url.pathname ?? $page.url.pathname;
        if ($discordEnabled && discordRPC.connected) {
            setPresence(targetPath);
        }
    });

    // Single reactive driver for toggles, startup, and title changes
    $effect(() => {
        if (!isInitialized) return;

        const enabled = $discordEnabled;
        const _title = presenceState.mediaTitle;
        const currentPath = $page.url.pathname;

        if (enabled && !discordRPC.connected) {
            enableDiscord(currentPath);
        } else if (!enabled && discordRPC.connected) {
            discordRPC.disconnect().catch(console.warn);
        } else if (enabled && discordRPC.connected) {
            setPresence(currentPath);
        }
    });
</script>

{@render children()}