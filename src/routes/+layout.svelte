<script>
    import "../app.css";
    import { onMount, onDestroy } from "svelte";
    import { page } from "$app/stores";
    import { initializeTheme } from "$lib/stores/themes.js";
    import { discordRPC } from "$lib/rpc.js";
    import { discordEnabled } from "$lib/stores/discordSettings.js";
    import { loadSettings } from "$lib/settings.js";
    import { presenceState } from "$lib/stores/presence.svelte.js";
    import {
        PRESENCE_DEFAULTS,
        PRESENCE_DETAILS,
        PRESENCE_ICONS,
    } from "$lib/defaults/discord.js";

    let { children } = $props();

    // Guard flag to prevent effects from running during initial async setup
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
        return {};
    }

    /** @param {string} path */
    async function setPresence(path) {
        // Guard against updating when disabled or not connected yet
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

        // 1. Fetch settings first
        const settings = await loadSettings();
        discordEnabled.set(settings.discord_rpc_enabled);

        // 2. Connect synchronously if enabled on boot
        if (settings.discord_rpc_enabled) {
            await enableDiscord($page.url.pathname);
        }

        // 3. Enable reactive effects AFTER initial connection setup is complete
        isInitialized = true;
    });

    onDestroy(() => {
        if (discordRPC.connected) {
            discordRPC.disconnect().catch(console.warn);
        }
    });

    // Handle toggling Discord RPC on/off (e.g. from Settings UI)
    $effect(() => {
        if (!isInitialized) return;

        const enabled = $discordEnabled;

        if (enabled && !discordRPC.connected) {
            enableDiscord($page.url.pathname);
        } else if (!enabled && discordRPC.connected) {
            discordRPC.disconnect().catch(console.warn);
        }
    });

    // Update presence reactively when route or media title changes
    $effect(() => {
        if (!isInitialized) return;

        const currentPath = $page.url.pathname;
        const _title = presenceState.mediaTitle; // Explicit reactive dependency

        if ($discordEnabled && discordRPC.connected) {
            setPresence(currentPath);
        }
    });
</script>

{@render children()}