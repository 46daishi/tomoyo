<script>
    let { value = $bindable('') } = $props();
    let capturing = $state(false);

    function startCapture() {
        capturing = true;
    }

    function handleKeydown(e) {
        e.preventDefault();
        value = e.code; // e.g. "ShiftLeft", "KeyA", "F4"
        capturing = false;
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