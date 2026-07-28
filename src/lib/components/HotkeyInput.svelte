<script>
    import { createEventDispatcher } from 'svelte';

    let { value = '' } = $props();
    let capturing = $state(false);
    const dispatch = createEventDispatcher();

    function startCapture() {
        capturing = true;
    }

    function handleKeydown(e) {
        e.preventDefault();
        capturing = false;
        dispatch('change', e.code);
    }

    function handleBlur() {
        capturing = false;
    }
</script>

<button
    class="modal-input hotkey-input"
    class:capturing
    onclick={startCapture}
    onkeydown={capturing ? handleKeydown : undefined}
    onblur={handleBlur}
>
    {capturing ? 'Press a key…' : value}
</button>

<style>
    .hotkey-input {
        width: 10rem;
        text-align: center;
        cursor: pointer;
        font-family: inherit;
    }

    .hotkey-input.capturing {
        border-color: var(--theme-primary, #36b7bd);
        color: var(--theme-primary, #36b7bd);
    }
</style>