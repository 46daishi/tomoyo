CREATE TABLE lookup_events (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    media_id INTEGER REFERENCES media(id) ON DELETE SET NULL,
    word_id INTEGER REFERENCES words(id) ON DELETE SET NULL,
    surface_text TEXT NOT NULL,
    looked_up_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    media_id INTEGER REFERENCES media(id) ON DELETE SET NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    moji_read INTEGER NOT NULL DEFAULT 0,
    sentences_read INTEGER NOT NULL DEFAULT 0
);

CREATE INDEX idx_lookup_events_media ON lookup_events(media_id);
CREATE INDEX idx_lookup_events_word ON lookup_events(word_id);
CREATE INDEX idx_lookup_events_surface ON lookup_events(surface_text);
CREATE INDEX idx_sessions_media ON sessions(media_id);