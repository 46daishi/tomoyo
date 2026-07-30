CREATE TABLE lookup_events_new (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    media_id INTEGER REFERENCES media(id) ON DELETE SET NULL,
    word_id INTEGER,
    surface_text TEXT NOT NULL,
    looked_up_at INTEGER NOT NULL DEFAULT (unixepoch())
);

INSERT INTO lookup_events_new (id, media_id, word_id, surface_text, looked_up_at)
SELECT id, media_id, word_id, surface_text, looked_up_at FROM lookup_events;

DROP TABLE lookup_events;
ALTER TABLE lookup_events_new RENAME TO lookup_events;

CREATE INDEX idx_lookup_events_media ON lookup_events(media_id);
CREATE INDEX idx_lookup_events_word ON lookup_events(word_id);
CREATE INDEX idx_lookup_events_surface ON lookup_events(surface_text);