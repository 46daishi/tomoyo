export function createSessionStore() {
    let running = $state(false);
    let seconds = $state(0);
    let timerHandle = null;

    function toggle() {
        if (running) {
            clearInterval(timerHandle);
            timerHandle = null;
            running = false;
            seconds = 0;
        } else {
            running = true;
            timerHandle = setInterval(() => {
                seconds += 1;
            }, 1000);
        }
    }

    function formatTime(totalSeconds) {
        const h = Math.floor(totalSeconds / 3600);
        const m = Math.floor((totalSeconds % 3600) / 60);
        const s = totalSeconds % 60;
        const pad = (n) => String(n).padStart(2, '0');
        return `${pad(h)}:${pad(m)}:${pad(s)}`;
    }

    function destroy() {
        if (timerHandle) clearInterval(timerHandle);
    }

    return {
        get running() { return running; },
        get seconds() { return seconds; },
        get formattedTime() { return formatTime(seconds); },
        toggle,
        destroy
    };
}