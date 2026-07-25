<script>
    import { onMount } from 'svelte';
    import { goto } from '$app/navigation';
    import ActionButton from '$lib/components/ActionButton.svelte';
    import SelectInput from '$lib/components/SelectInput.svelte';
    import { ICONS } from '$lib/icons';
    import { loadSettings, saveSettings } from '$lib/settings.js';
    import ThemeGrid from '$lib/components/ThemeGrid.svelte';

    let settings = $state(null);
    let activeTab = $state('general');
    let saveTimeout = null;

    const TABS = [
        { id: 'general', label: 'General' },
        { id: 'lookup', label: 'Lookup' },
        { id: 'input', label: 'Input & History' },
        { id: 'mini', label: 'Mini Mode' },
        { id: 'dictionaries', label: 'Dictionaries' },
        { id: 'review', label: 'Review' },
    ];

    const FONT_OPTIONS = [
        { value: 'Noto Sans JP', label: 'Noto Sans JP' },
        { value: 'M PLUS 1p', label: 'M PLUS 1p' },
        { value: 'Zen Kaku Gothic New', label: 'Zen Kaku Gothic New' },
    ];

    const LOOKUP_MODE_OPTIONS = [
        { value: 'click', label: 'Click' },
        { value: 'hover', label: 'Hover' },
        { value: 'hotkey', label: 'Hotkey' },
    ];

    const INPUT_MODE_OPTIONS = [
        { value: 'clipboard', label: 'Clipboard' },
        { value: 'websocket', label: 'Websocket' },
    ];

    const REVIEW_MODE_OPTIONS = [
        { value: 'normal', label: 'Normal' },
        { value: 'flashcard', label: 'Flashcard' },
    ];

    onMount(async () => {
        settings = await loadSettings();
    });

    // Debounced autosave — fires ~500ms after the last change, rather than
    // writing to disk on every single keystroke/toggle.
    function queueSave() {
        clearTimeout(saveTimeout);
        saveTimeout = setTimeout(() => {
            saveSettings(settings);
        }, 500);
    }

    function update(key, value) {
        settings[key] = value;
        queueSave();
    }
</script>

<main class="page settings-page">
    <div class="settings-header">
        <ActionButton icon={ICONS.back} variant="primary" size="small" onAction={() => history.back()} />
        <h1>Settings</h1>
    </div>

    {#if settings}
        <div class="settings-layout">
            <nav class="settings-tabs">
                {#each TABS as tab}
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
                {#if activeTab === 'general'}
                    
                    <div class="setting-row theme-row">
                        <div class="setting-label">
                            <span>Theme</span>
                        </div>
                        <ThemeGrid />
                    </div>

                    <div class="setting-row">
                        <div class="setting-label"><span>Font family</span></div>
                        <SelectInput options={FONT_OPTIONS} value={settings.font_family} on:change={(e) => update('font_family', e.target.value)} />
                    </div>

                    <div class="setting-row">
                        <div class="setting-label">
                            <span>Font size</span>
                            <span class="setting-value">{settings.font_size}px</span>
                        </div>
                        <input type="range" min="14" max="32" bind:value={settings.font_size} oninput={queueSave} />
                    </div>

                    <div class="setting-row">
                        <div class="setting-label"><span>Resume last session on startup</span></div>
                        <label class="switch">
                            <input type="checkbox" bind:checked={settings.resume_last_session} onchange={queueSave} />
                            <span class="switch-track"></span>
                        </label>
                    </div>

                    <div class="setting-row">
                        <div class="setting-label"><span>Discord Rich Presence</span></div>
                        <label class="switch">
                            <input type="checkbox" bind:checked={settings.discord_rpc_enabled} onchange={queueSave} />
                            <span class="switch-track"></span>
                        </label>
                    </div>

                    <div class="setting-row">
                        <div class="setting-label"><span>Data</span></div>
                        <div class="setting-actions">
                            <button class="modal-btn">Export</button>
                            <button class="modal-btn">Import</button>
                        </div>
                    </div>
                {/if}

                {#if activeTab === 'lookup'}
                    <div class="setting-row">
                        <div class="setting-label"><span>Lookup trigger</span></div>
                        <SelectInput options={LOOKUP_MODE_OPTIONS} value={settings.lookup_mode} on:change={(e) => update('lookup_mode', e.target.value)} />
                    </div>

                    <div class="setting-row">
                        <div class="setting-label"><span>Cycle key</span></div>
                        <input class="modal-input settings-key-input" bind:value={settings.cycle_key} oninput={queueSave} />
                    </div>

                    <div class="setting-row">
                        <div class="setting-label"><span>Limit lookups per hour</span></div>
                        <label class="switch">
                            <input type="checkbox" bind:checked={settings.lookup_limit_enabled} onchange={queueSave} />
                            <span class="switch-track"></span>
                        </label>
                    </div>

                    {#if settings.lookup_limit_enabled}
                        <div class="setting-row sub-row">
                            <div class="setting-label">
                                <span>Max lookups / hour</span>
                                <span class="setting-value">{settings.lookup_limit_per_hour}</span>
                            </div>
                            <input type="range" min="5" max="200" bind:value={settings.lookup_limit_per_hour} oninput={queueSave} />
                        </div>
                    {/if}

                    <div class="setting-row">
                        <div class="setting-label"><span>Highlight words on hover</span></div>
                        <label class="switch">
                            <input type="checkbox" bind:checked={settings.word_highlight_enabled} onchange={queueSave} />
                            <span class="switch-track"></span>
                        </label>
                    </div>

                    <div class="setting-row">
                        <div class="setting-label"><span>Show related entries in tooltip</span></div>
                        <label class="switch">
                            <input type="checkbox" bind:checked={settings.show_related_entries} onchange={queueSave} />
                            <span class="switch-track"></span>
                        </label>
                    </div>

                    <div class="setting-row">
                        <div class="setting-label"><span>Track commonly looked-up words not in dictionary</span></div>
                        <label class="switch">
                            <input type="checkbox" bind:checked={settings.track_unknown_words} onchange={queueSave} />
                            <span class="switch-track"></span>
                        </label>
                    </div>
                {/if}

                {#if activeTab === 'input'}
                    <div class="setting-row">
                        <div class="setting-label"><span>Input source</span></div>
                        <SelectInput options={INPUT_MODE_OPTIONS} value={settings.input_mode} on:change={(e) => update('input_mode', e.target.value)} />
                    </div>

                    {#if settings.input_mode === 'websocket'}
                        <div class="setting-row sub-row">
                            <div class="setting-label"><span>Websocket address</span></div>
                            <input class="modal-input" bind:value={settings.websocket_address} oninput={queueSave} />
                        </div>
                    {/if}

                    <div class="setting-row">
                        <div class="setting-label">
                            <span>Japanese detection sensitivity</span>
                            <span class="setting-value">{Math.round(settings.jp_detection_threshold * 100)}%</span>
                        </div>
                        <input type="range" min="0.1" max="0.9" step="0.05" bind:value={settings.jp_detection_threshold} oninput={queueSave} />
                    </div>

                    <div class="setting-row">
                        <div class="setting-label"><span>Enable history</span></div>
                        <label class="switch">
                            <input type="checkbox" bind:checked={settings.history_enabled} onchange={queueSave} />
                            <span class="switch-track"></span>
                        </label>
                    </div>

                    {#if settings.history_enabled}
                        <div class="setting-row sub-row">
                            <div class="setting-label">
                                <span>History span</span>
                                <span class="setting-value">{settings.history_span} sentences</span>
                            </div>
                            <input type="range" min="10" max="200" step="10" bind:value={settings.history_span} oninput={queueSave} />
                        </div>
                    {/if}
                {/if}

                {#if activeTab === 'mini'}
                    <div class="setting-row">
                        <div class="setting-label"><span>Enable mini mode</span></div>
                        <label class="switch">
                            <input type="checkbox" bind:checked={settings.mini_mode_enabled} onchange={queueSave} />
                            <span class="switch-track"></span>
                        </label>
                    </div>

                    {#if settings.mini_mode_enabled}
                        <div class="setting-row sub-row">
                            <div class="setting-label"><span>Auto-trigger by window size</span></div>
                            <label class="switch">
                                <input type="checkbox" bind:checked={settings.mini_mode_auto_trigger} onchange={queueSave} />
                                <span class="switch-track"></span>
                            </label>
                        </div>

                        {#if settings.mini_mode_auto_trigger}
                            <div class="setting-row sub-row">
                                <div class="setting-label"><span>Enter threshold (W × H)</span></div>
                                <div class="setting-actions">
                                    <input type="number" class="modal-input settings-number-input" bind:value={settings.mini_mode_enter_width} oninput={queueSave} />
                                    <span class="setting-x">×</span>
                                    <input type="number" class="modal-input settings-number-input" bind:value={settings.mini_mode_enter_height} oninput={queueSave} />
                                </div>
                            </div>

                            <div class="setting-row sub-row">
                                <div class="setting-label"><span>Exit threshold (W × H)</span></div>
                                <div class="setting-actions">
                                    <input type="number" class="modal-input settings-number-input" bind:value={settings.mini_mode_exit_width} oninput={queueSave} />
                                    <span class="setting-x">×</span>
                                    <input type="number" class="modal-input settings-number-input" bind:value={settings.mini_mode_exit_height} oninput={queueSave} />
                                </div>
                            </div>
                        {/if}

                        <div class="setting-row sub-row">
                            <div class="setting-label">
                                <span>Transparency</span>
                                <span class="setting-value">{Math.round(settings.mini_mode_transparency * 100)}%</span>
                            </div>
                            <input type="range" min="0" max="0.9" step="0.05" bind:value={settings.mini_mode_transparency} oninput={queueSave} />
                        </div>
                    {/if}
                {/if}

                {#if activeTab === 'dictionaries'}
                    <div class="placeholder-tab">
                        <p>Dictionary management is coming soon.</p>
                        <p class="placeholder-sub">JMdict is currently built in and always active.</p>
                    </div>
                {/if}

                {#if activeTab === 'review'}
                    <div class="setting-row">
                        <div class="setting-label"><span>Default review mode</span></div>
                        <SelectInput options={REVIEW_MODE_OPTIONS} value={settings.default_review_mode} on:change={(e) => update('default_review_mode', e.target.value)} />
                    </div>

                    <div class="setting-row">
                        <div class="setting-label"><span>Estimate media difficulty from lookup frequency</span></div>
                        <label class="switch">
                            <input type="checkbox" bind:checked={settings.difficulty_estimation_enabled} onchange={queueSave} />
                            <span class="switch-track"></span>
                        </label>
                    </div>

                    <div class="placeholder-tab">
                        <p class="placeholder-sub">Full review/flashcard functionality is coming soon.</p>
                    </div>
                {/if}
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
        max-width: 900px;
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
    }

    .setting-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 1.5rem;
        padding-bottom: 1.1rem;
        border-bottom: 1px solid var(--theme-border, #404040);
    }

    .setting-row.sub-row {
        padding-left: 1rem;
        border-left: 2px solid color-mix(in srgb, var(--theme-primary, #36b7bd) 40%, transparent);
    }

    .setting-label {
        display: flex;
        flex-direction: column;
        gap: 0.2rem;
        font-size: 0.9rem;
        color: var(--theme-text, #f6f6f6);
    }

    .setting-value {
        font-size: 0.78rem;
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .setting-actions {
        display: flex;
        align-items: center;
        gap: 0.5rem;
    }

    .setting-x {
        color: var(--theme-textSecondary, #b3b3b3);
    }

    .settings-number-input {
        width: 5rem;
        text-align: center;
    }

    .settings-key-input {
        width: 10rem;
    }

    input[type="range"] {
        width: 180px;
        accent-color: var(--theme-primary, #36b7bd);
    }

    /* ── Toggle switch ── */
    .switch {
        position: relative;
        display: inline-block;
        width: 42px;
        height: 24px;
        flex-shrink: 0;
    }

    .switch input {
        opacity: 0;
        width: 0;
        height: 0;
    }

    .switch-track {
        position: absolute;
        inset: 0;
        background: var(--theme-border, #404040);
        border-radius: 100px;
        cursor: pointer;
        transition: background 0.2s ease;
    }

    .switch-track::before {
        content: "";
        position: absolute;
        width: 18px;
        height: 18px;
        left: 3px;
        top: 3px;
        background: #fff;
        border-radius: 50%;
        transition: transform 0.2s ease;
    }

    .switch input:checked + .switch-track {
        background: var(--theme-primary, #36b7bd);
    }

    .switch input:checked + .switch-track::before {
        transform: translateX(18px);
    }

    .placeholder-tab {
        color: var(--theme-textSecondary, #b3b3b3);
        font-size: 0.9rem;
        padding: 1rem 0;
    }

    .placeholder-sub {
        font-size: 0.8rem;
        opacity: 0.7;
    }

    .theme-row {
        align-items: flex-start; /* grid is taller than a single-line label now */
    }
</style>