import {
    startSession,
    endSession,
    recordSentenceRead,
    logSentenceRead,
    touchSession
} from '$lib/sessions.js';

// Module-level singleton: the session survives route navigation but lives only
// for the lifetime of the app window. App-restart recovery is handled by
// recoverDanglingSessions() in the root layout.
/** @type {number | null} */
let activeId = $state(null);
/** @type {string | null} */
let mediaTitle = $state(null);
/** @type {string | null} */
let mediaCoverPath = $state(null);
/** @type {string | null} */
let mediaColor = $state(null);

/** @type {number | null} */
let sessionId = $state(null);
let running = $state(false);
let seconds = $state(0);
// Bumped once session writes have committed, so stats views can reload
// aggregates without racing the fire-and-forget session SQL below.
let statsVersion = $state(0);

let currentText = $state('');
/** @type {string[]} */
let historyEntries = $state([]);
let historyIndex = $state(0);

/** @type {string | null} */
let notice = $state(null);

let baseSeconds = 0;
let runStartedAt = 0;
/** @type {ReturnType<typeof setInterval> | null} */
let tickHandle = null;
let lastTouchAt = 0;
let starting = false;
let startToken = 0;

// Session writes are fire-and-forget while reading, but the end-of-session
// stats query must see all of them. Chain them so `finishSession` can queue
// its `endSession` behind the final sentence writes and refresh only after
// everything has committed.
let writeChain = Promise.resolve();

/**
 * @param {() => Promise<unknown>} write
 * @returns {Promise<unknown>} settles when this write has run
 */
function queueWrite(write) {
    const next = writeChain.then(write);
    writeChain = next.then(
        () => {},
        () => {}
    );
    return next;
}

/** @param {number} totalSeconds */
function formatTime(totalSeconds) {
    const hours = Math.floor(totalSeconds / 3600);
    const minutes = Math.floor((totalSeconds % 3600) / 60);
    const secs = totalSeconds % 60;
    if (hours > 0) {
        return `${hours}:${String(minutes).padStart(2, '0')}:${String(secs).padStart(2, '0')}`;
    }
    return `${minutes}:${String(secs).padStart(2, '0')}`;
}

function computeElapsed() {
    if (!running) return seconds;
    return baseSeconds + Math.floor((Date.now() - runStartedAt) / 1000);
}

function tick() {
    seconds = computeElapsed();
    if (sessionId !== null && seconds - lastTouchAt >= 60) {
        lastTouchAt = seconds;
        void touchSession(sessionId).catch(() => {});
    }
}

function beginTicking() {
    runStartedAt = Date.now();
    running = true;
    lastTouchAt = baseSeconds;
    if (tickHandle) clearInterval(tickHandle);
    tickHandle = setInterval(tick, 1000);
    seconds = baseSeconds;
}

function haltTicking() {
    if (running) {
        seconds = computeElapsed();
        baseSeconds = seconds;
    }
    running = false;
    if (tickHandle) {
        clearInterval(tickHandle);
        tickHandle = null;
    }
}

function resetReading() {
    currentText = '';
    historyEntries = [];
    historyIndex = 0;
}

function finishSession(/** @type {string | null} */ reason = null) {
    const sid = sessionId;
    const finalSeconds = computeElapsed();
    haltTicking();
    sessionId = null;
    seconds = 0;
    baseSeconds = 0;
    resetReading();
    if (sid !== null) {
        // Queue the end behind the session's pending sentence writes, then
        // refresh stats: querying any earlier misses this session's reading
        // time (`ended_at` still NULL) and its final sentence's moji/events.
        void queueWrite(() => endSession(sid, finalSeconds))
            .then(() => {
                statsVersion += 1;
            })
            .catch((err) => {
                console.warn('Failed to end reading session:', err);
            });
    }
    if (reason) notice = reason;
}

/**
 * @param {number} id
 * @param {Record<string, any> | null} [media]
 */
function activate(id, media = null) {
    const next = Number(id);
    if (!Number.isFinite(next)) return;

    if (next !== activeId) {
        if (sessionId !== null) {
            const label = mediaTitle || (activeId != null ? `Media #${activeId}` : 'this media');
            finishSession(`Reading session ended for ${label}`);
        } else {
            haltTicking();
            resetReading();
        }
        if (starting) {
            startToken += 1;
            starting = false;
        }
        activeId = next;
        mediaTitle = null;
        mediaCoverPath = null;
        mediaColor = null;
    } else {
        activeId = next;
    }

    // Ignore stale media objects while the route param is still switching.
    if (media && Number(media.id) === next) {
        if (media.title != null) mediaTitle = media.title;
        mediaCoverPath = media.cover_path ?? null;
        mediaColor = media.color ?? null;
    }
}

async function start() {
    if (sessionId !== null || starting) return;
    const id = activeId;
    if (id == null || !Number.isFinite(id)) return;

    const token = ++startToken;
    starting = true;
    /** @type {number | null} */
    let startedId = null;
    try {
        startedId = await startSession(id);
    } catch (err) {
        console.warn('Failed to start reading session:', err);
    }

    if (token !== startToken) {
        if (startedId != null) {
            void endSession(startedId, 0).catch(() => {});
        }
        return;
    }

    starting = false;
    if (startedId == null) return;
    if (sessionId !== null) {
        void endSession(startedId, 0).catch(() => {});
        return;
    }

    sessionId = startedId;
    baseSeconds = 0;
    seconds = 0;
    beginTicking();
    // The session row exists now; let media stats pick it up as last active.
    statsVersion += 1;
}

function stop() {
    if (sessionId === null && !starting) return;
    startToken += 1;
    starting = false;
    finishSession();
}

function pause() {
    if (sessionId === null || !running) return;
    haltTicking();
    void touchSession(sessionId).catch(() => {});
}

function resume() {
    if (sessionId === null || running) return;
    beginTicking();
    void touchSession(sessionId).catch(() => {});
}

function togglePlayback() {
    if (sessionId === null) {
        void start();
        return;
    }
    if (running) pause();
    else resume();
}

/**
 * Replaces the displayed sentence. Returns false when there is no active
 * session or when the text is unchanged (e.g. clipboard re-read after the
 * sentence window remounts or a paused session resumes).
 */
/**
 * @param {string} text
 * @param {Record<string, any> | null} [settings]
 */
function pushText(text, settings) {
    if (sessionId === null || !text || text === currentText) return false;

    if (settings?.history_enabled && currentText) {
        const span = settings.history_span ?? 50;
        historyEntries = [currentText, ...historyEntries].slice(0, span);
    }

    currentText = text;
    historyIndex = 0;

    const sid = sessionId;
    const mid = activeId;
    void queueWrite(() => recordSentenceRead(sid, text.length)).catch((err) => {
        console.warn('Failed to record sentence read:', err);
    });
    if (mid != null) {
        void queueWrite(() => logSentenceRead({
            sessionId: sid,
            mediaId: mid,
            sentenceText: text
        })).catch((err) => {
            console.warn('Failed to log sentence read:', err);
        });
    }
    return true;
}

/** @param {number | null} max */
function pruneHistory(max) {
    if (max == null) return;
    if (historyEntries.length > max) {
        historyEntries = historyEntries.slice(0, max);
    }
    if (historyIndex > historyEntries.length) {
        historyIndex = historyEntries.length;
    }
}

/** @param {number} [max] */
function historyPrev(max = 50) {
    const newIndex = Math.max(historyIndex - 1, 0);
    if (newIndex === historyIndex) return false;
    historyIndex = newIndex;
    return true;
}

/** @param {number} [max] */
function historyNext(max = 50) {
    const maxIndex = Math.min(historyEntries.length, max);
    const newIndex = Math.min(historyIndex + 1, maxIndex);
    if (newIndex === historyIndex) return false;
    historyIndex = newIndex;
    return true;
}

function clearNotice() {
    notice = null;
}

export const session = {
    get active() {
        return sessionId !== null;
    },
    get running() {
        return running;
    },
    get paused() {
        return sessionId !== null && !running;
    },
    get sessionId() {
        return sessionId;
    },
    get statsVersion() {
        return statsVersion;
    },
    get seconds() {
        return seconds;
    },
    get formattedTime() {
        return formatTime(seconds);
    },
    get mediaId() {
        return activeId;
    },
    get mediaTitle() {
        return mediaTitle;
    },
    get mediaCoverPath() {
        return mediaCoverPath;
    },
    get mediaColor() {
        return mediaColor;
    },
    get currentText() {
        return currentText;
    },
    get historyEntries() {
        return historyEntries;
    },
    get historyIndex() {
        return historyIndex;
    },
    get displayedText() {
        return historyIndex === 0
            ? currentText
            : (historyEntries[historyIndex - 1] ?? currentText);
    },
    get notice() {
        return notice;
    },
    activate,
    start,
    stop,
    pause,
    resume,
    togglePlayback,
    pushText,
    pruneHistory,
    historyPrev,
    historyNext,
    clearNotice
};
