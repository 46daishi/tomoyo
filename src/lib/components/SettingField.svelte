<script>
    import SelectInput from '$lib/components/SelectInput.svelte';
    import ThemeGrid from '$lib/components/ThemeGrid.svelte';
    import HotkeyInput from '$lib/components/HotkeyInput.svelte';
    import MultiSelectInput from './MultiSelectInput.svelte';
    import { coverSrc } from '$lib/db.js';

    let { option, settings = $bindable(), onChange, onAction } = $props();

    function displayValue(val) {
        if (option.percent) return `${Math.round(val * 100)}%`;
        return `${val}${option.unit ?? ''}`;
    }
</script>

<div class="setting-row" class:sub-row={option.subRow}>
    {#if option.type === 'placeholder'}
        <div class="placeholder-tab">
            {#if option.text}<p>{option.text}</p>{/if}
            {#if option.sub}<p class="placeholder-sub">{option.sub}</p>{/if}
        </div>
    {:else}
        <div class="setting-label">
            <span>{option.label}</span>
            {#if option.type === 'slider'}
                <span class="setting-value">{displayValue(settings[option.key])}</span>
            {/if}
        </div>

        {#if option.type === 'theme'}
            <ThemeGrid />
        {:else if option.type === 'select'}
            <SelectInput
                options={option.options}
                value={settings[option.key]}
                on:change={(e) => onChange(option.key, e.target.value)}
            />
        {:else if option.type === 'slider'}
            <input
                type="range"
                min={option.min}
                max={option.max}
                step={option.step ?? 1}
                bind:value={settings[option.key]}
                oninput={() => onChange(option.key, settings[option.key])}
            />
        {:else if option.type === 'number'}
            <input
                type="number"
                class="modal-input settings-number-input"
                min={option.min}
                max={option.max}
                bind:value={settings[option.key]}
                oninput={() => onChange(option.key, settings[option.key])}
            />
        {:else if option.type === 'text'}
            <input
                class="modal-input"
                placeholder={option.placeholder ?? ''}
                bind:value={settings[option.key]}
                oninput={() => onChange(option.key, settings[option.key])}
            />
        {:else if option.type === 'checkbox'}
            <label class="switch">
                <input
                    type="checkbox"
                    bind:checked={settings[option.key]}
                    onchange={() => onChange(option.key, settings[option.key])}
                />
                <span class="switch-track"></span>
            </label>
        {:else if option.type === 'hotkey'}
            <HotkeyInput
                value={settings[option.key]}
                on:change={(e) => onChange(option.key, e.detail)}
            />
        {:else if option.type === 'actions'}
            <div class="setting-actions">
                {#each option.buttons as btn}
                    <button class="modal-btn" onclick={() => onAction(btn.action)}>{btn.label}</button>
                {/each}
            </div>
        {:else if option.type === 'image'}
            <button
                class="profile-picker"
                title="Choose a profile picture"
                onclick={() => onAction('pick_profile_picture')}
            >
                {#if settings[option.key]}
                    <img src={coverSrc(settings[option.key])} alt="" />
                {:else}
                    <span>+ Picture</span>
                {/if}
            </button>
        {:else if option.type === 'multi_select'}
            <MultiSelectInput
                options={option.options}
                values={settings[option.key] ?? []}
                onChange={(next) => onChange(option.key, next)}
            />
        {/if}
    {/if}
</div>

<style>
    .setting-row {
        display: flex;
        align-items: center;
        justify-content: space-between;
        gap: 1.5rem;
        padding-bottom: 1.1rem;
        border-bottom: 1px solid var(--theme-border, #404040);
    }

    .setting-row.sub-row {
        margin-left: 1.5rem;
        padding: 0.7rem 0.9rem 1.1rem;
        background: color-mix(in srgb, var(--theme-surface, #2d2d2d) 50%, transparent);
        border-radius: 8px;
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

    .profile-picker {
        flex-shrink: 0;
        width: 72px;
        height: 72px;
        border-radius: 50%;
        border: 2px dashed var(--theme-border, #404040);
        background: none;
        display: flex;
        align-items: center;
        justify-content: center;
        cursor: pointer;
        overflow: hidden;
        padding: 0;
        color: var(--theme-textSecondary, #b3b3b3);
        font-family: inherit;
        font-size: 0.8rem;
        transition: border-color 0.15s ease, color 0.15s ease;
    }

    .profile-picker:hover {
        border-color: var(--theme-primary, #36b7bd);
        color: var(--theme-primary, #36b7bd);
    }

    .profile-picker img {
        width: 100%;
        height: 100%;
        object-fit: cover;
    }

    .settings-number-input {
        width: 5rem;
        text-align: center;
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

    
</style>