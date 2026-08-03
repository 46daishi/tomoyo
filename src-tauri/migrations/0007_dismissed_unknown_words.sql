CREATE TABLE dismissed_unknown_words (
    surface_text TEXT PRIMARY KEY,
    dismissed_at INTEGER NOT NULL DEFAULT (unixepoch())
);