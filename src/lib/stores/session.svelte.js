import { startSession, endSession, recordSentenceRead, logSentenceRead } from '$lib/sessions.js';

export function createSessionStore(mediaId) {
    let running = $state(false);
    let seconds = $state(0);
    let timerHandle = null;
    let sessionId = null;

    async function toggle() {
        if (running) {
            clearInterval(timerHandle);
            timerHandle = null;
            running = false;
            seconds = 0;
            await endSession(sessionId);
            sessionId = null;
        } else {
            running = true;
            sessionId = await startSession(mediaId);
            timerHandle = setInterval(() => {
                seconds += 1;
            }, 1000);
        }
    }

    async function recordSentence(sentenceText) {
        if (sessionId) {
            await recordSentenceRead(sessionId, sentenceText.length);
            await logSentenceRead({ sessionId, mediaId, sentenceText });
        }
    }

    function formatTime(totalSeconds) {
        const h = Math.floor(totalSeconds / 3600);
        const m = Math.floor((totalSeconds % 3600) / 60);
        const s = totalSeconds % 60;
        const pad = (n) => String(n).padStart(2, '0');
        return `${pad(h)}:${pad(m)}:${pad(s)}`;
    }

    async function destroy() {
        if (timerHandle) clearInterval(timerHandle);
        if (running && sessionId) {
            await endSession(sessionId);
        }
    }

    return {
        get running() { return running; },
        get seconds() { return seconds; },
        get formattedTime() { return formatTime(seconds); },
        toggle,
        recordSentence,
        destroy
    };
}