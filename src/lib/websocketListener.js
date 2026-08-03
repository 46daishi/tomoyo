let socket = null;
let reconnectHandle = null;
let shouldReconnect = false;
let currentUrl = null;
let currentOnChange = null;

const RECONNECT_DELAY_MS = 3000;

function connect() {
    if (!currentUrl) return;

    socket = new WebSocket(currentUrl);

    socket.onopen = () => {
        console.log('[websocket] connected to', currentUrl);
    };

    socket.onmessage = (event) => {
        // Most texthooker tools send plain text; some send JSON like
        // { "text": "...", ... } — handle both without erroring on plain text.
        let text = event.data;
        try {
            const parsed = JSON.parse(event.data);
            if (typeof parsed === 'object' && parsed !== null && 'text' in parsed) {
                text = parsed.text;
            }
        } catch {
            // not JSON — event.data is already the raw text, nothing to do
        }

        if (typeof text === 'string' && text.trim()) {
            currentOnChange(text);
        }
    };

    socket.onclose = () => {
        socket = null;
        if (shouldReconnect) {
            reconnectHandle = setTimeout(connect, RECONNECT_DELAY_MS);
        }
    };

    socket.onerror = (err) => {
        console.warn('[websocket] connection error:', err);
        // onclose fires right after onerror for a failed connection, so
        // reconnect scheduling is handled there, not duplicated here.
    };
}

export function startWebsocketListener(url, onChange) {
    stopWebsocketListener(); // clean up any previous connection first

    currentUrl = url;
    currentOnChange = onChange;
    shouldReconnect = true;
    connect();
}

export function stopWebsocketListener() {
    shouldReconnect = false;
    clearTimeout(reconnectHandle);
    reconnectHandle = null;

    if (socket) {
        socket.onclose = null; // prevent the reconnect-on-close logic from firing during a deliberate stop
        socket.close();
        socket = null;
    }
}