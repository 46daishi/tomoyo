export function initMiniMode(settings, onMiniModeChange) {
    let miniMode = false;
    let resizeDebounceHandle = null;

    function applyMiniModeClasses(active) {
        document.documentElement.classList.toggle('mini-mode', active);
        document.body.classList.toggle('mini-mode', active);
    }

    function applyMiniModeTransparency(transparency) {
        const colorWeight = Math.round((1 - transparency) * 100);
        document.documentElement.style.setProperty('--mini-color-weight', `${colorWeight}%`);
    }

    function checkWindowSize() {
        if (settings?.mini_mode_enabled === false) {
            if (miniMode) {
                miniMode = false;
                applyMiniModeClasses(false);
                onMiniModeChange(false);
            }
            return;
        }

        const h = window.innerHeight;
        const enterHeight = settings?.mini_mode_enter_height ?? 200;
        const exitHeight = settings?.mini_mode_exit_height ?? 300;

        if (!miniMode && h <= enterHeight) {
            miniMode = true;
            applyMiniModeClasses(true);
            onMiniModeChange(true);
        } else if (miniMode && h >= exitHeight) {
            miniMode = false;
            applyMiniModeClasses(false);
            onMiniModeChange(false);
        }
    }

    function handleResize() {
        clearTimeout(resizeDebounceHandle);
        resizeDebounceHandle = setTimeout(checkWindowSize, 50);
    }

    applyMiniModeTransparency(settings.mini_mode_transparency);
    checkWindowSize();
    window.addEventListener('resize', handleResize);

    return {
        destroy() {
            window.removeEventListener('resize', handleResize);
            clearTimeout(resizeDebounceHandle);
        }
    };
}