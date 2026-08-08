CREATE TABLE sentence_read_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER REFERENCES sessions(id) ON DELETE SET NULL,
    media_id INTEGER REFERENCES media(id) ON DELETE SET NULL,
    sentence_text TEXT NOT NULL,
    read_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX idx_sentence_read_events_media_time ON sentence_read_events(media_id, read_at DESC);