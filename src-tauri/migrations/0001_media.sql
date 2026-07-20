CREATE TABLE media (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    title TEXT NOT NULL,
    cover_path TEXT,
    status TEXT NOT NULL DEFAULT 'active',   -- active | paused | completed | dropped
    color TEXT NOT NULL DEFAULT '#89b4fa',   -- used in stats
    tag TEXT,                                 -- e.g. media type / source tag
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX idx_media_status ON media(status);