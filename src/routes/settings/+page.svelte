<script>
    import { onMount } from 'svelte';
    import { invoke } from '@tauri-apps/api/core';
    import { open, save, confirm } from '@tauri-apps/plugin-dialog';
    import ActionButton from '$lib/components/ActionButton.svelte';
    import { ICONS } from '$lib/icons';
    import { loadSettings, saveSettings } from '$lib/settings.js';
    import { SETTINGS_SCHEMA } from '$lib/settings.js';
    import SettingField from '$lib/components/SettingField.svelte';
    import { pickProfilePicture, closeDb } from '$lib/db.js';
    import { discordEnabled } from '$lib/stores/discordSettings.js';

    let settings = $state(null);
    let activeTab = $state(SETTINGS_SCHEMA[0].id);
    let saveTimeout = null;

    onMount(async () => {
        settings = await loadSettings();
    });

    function queueSave() {
        clearTimeout(saveTimeout);
        saveTimeout = setTimeout(() => {
            saveSettings(settings);
        }, 500);
    }

    function handleChange(key, value) {
        settings[key] = value;
        queueSave();
        if (key === 'discord_rpc_enabled') {
                discordEnabled.set(value);
        }
    }

    function handleAction(action) {
        if (action === 'export') {
            handleExport();
        } else if (action === 'import') {
            handleImport();
        } else if (action === 'pick_profile_picture') {
            pickProfilePicture().then((path) => {
                if (path) {
                    settings.profile_picture = path;
                    queueSave();
                }
            });
        }
    }

    async function handleExport() {
        const dest = await save({
            title: 'Export database',
            defaultPath: 'tomoyo-backup.db',
            filters: [{ name: 'SQLite database', extensions: ['db', 'sqlite'] }],
        });
        if (!dest) return;
        try {
            await closeDb();
            await invoke('export_database', { dest });
        } catch (e) {
            alert(`Export failed: ${e}`);
        }
    }

    async function handleImport() {
        const source = await open({
            multiple: false,
            title: 'Import database',
            filters: [{ name: 'SQLite database', extensions: ['db', 'sqlite'] }],
        });
        if (!source) return;

        const yes = await confirm(
            'Importing will replace all current data with the contents of the selected database. This cannot be undone. Continue?',
            { title: 'Import database', kind: 'warning' }
        );
        if (!yes) return;

        try {
            await closeDb();
            await invoke('import_database', { source });
        } catch (e) {
            alert(`Import failed: ${e}`);
            return;
        }

        await invoke('restart_app');
    }

    let activeTabSchema = $derived(SETTINGS_SCHEMA.find((t) => t.id === activeTab));
</script>

<main class="page settings-page">
    <div class="settings-header">
        <ActionButton icon={ICONS.back} variant="primary" size="small" onAction={() => history.back()} />
        <h1>Settings</h1>
    </div>

    {#if settings}
        <div class="settings-layout">
            <nav class="settings-tabs">
                {#each SETTINGS_SCHEMA as tab}
                    <button
                        class="settings-tab"
                        class:active={activeTab === tab.id}
                        onclick={() => (activeTab = tab.id)}
                    >
                        {tab.label}
                    </button>
                {/each}
            </nav>

            <div class="settings-content">
                {#each activeTabSchema.options as option (option.key)}
                    {#if !option.showIf || option.showIf(settings)}
                        <SettingField {option} bind:settings onChange={handleChange} onAction={handleAction} />
                    {/if}
                {/each}
            </div>
        </div>
    {:else}
        <p>Loading settings…</p>
    {/if}
</main>

<style>
    .settings-page {
        padding: 2rem;
        box-sizing: border-box;
        height: 100vh;
        overflow: hidden; /* the page itself no longer scrolls as a whole */
    }

    .settings-header {
        display: flex;
        align-items: center;
        gap: 1rem;
        margin-bottom: 1.5rem;
    }

    .settings-header h1 {
        font-size: 1.5rem;
        margin: 0;
    }
    
    .settings-layout {
        display: flex;
        gap: 2rem;
        max-width: 1800px;
        height: calc(100vh - 2rem - 1.5rem - 2.5rem); /* viewport minus top/bottom page padding and the header's height+margin */
        box-sizing: border-box;
    }

    .settings-tabs {
        display: flex;
        flex-direction: column;
        gap: 0.3rem;
        flex-shrink: 0;
        width: 160px;
    }

    .settings-tab {
        text-align: left;
        background: none;
        border: none;
        border-radius: 8px;
        padding: 0.6rem 0.8rem;
        color: var(--theme-textSecondary, #b3b3b3);
        font-size: 0.9rem;
        font-weight: 600;
        font-family: inherit;
        cursor: pointer;
        transition: background 0.15s ease, color 0.15s ease;
    }

    .settings-tab:hover {
        background: color-mix(in srgb, var(--theme-textSecondary, #b3b3b3) 8%, transparent);
        color: var(--theme-text, #f6f6f6);
    }

    .settings-tab.active {
        background: var(--theme-primary, #36b7bd);
        color: #fff;
    }

    
    .settings-content {
        flex: 1;
        display: flex;
        flex-direction: column;
        gap: 1.1rem;
        min-width: 0;
        height: 100%;
        overflow-y: auto;
        padding-right: 1.5rem; /* more room now, so the scrollbar doesn't crowd the content */
        padding-right: 2.5rem; /* pushes the scrollbar further from the content itself */
    }
    
    .settings-content::-webkit-scrollbar {
        width: 6px;
    }
    
    .settings-content::-webkit-scrollbar-track {
        background: transparent;
    }
    
    .settings-content::-webkit-scrollbar-thumb {
        background: var(--theme-border, #404040);
        border-radius: 3px;
    }
    
    .settings-content::-webkit-scrollbar-thumb:hover {
        background: var(--theme-textSecondary, #b3b3b3);
    }
    
    .theme-grid {
            align-items: flex-start; /* grid is taller than a single-line label now */
        }
</style>