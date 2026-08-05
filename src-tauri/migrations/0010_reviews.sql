CREATE TABLE review_sessions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_type TEXT NOT NULL,          -- 'word' | 'sentence'
    media_id INTEGER REFERENCES media(id) ON DELETE SET NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER,
    last_updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE review_log (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id INTEGER REFERENCES review_sessions(id) ON DELETE SET NULL,
    review_type TEXT NOT NULL,           -- 'word' | 'sentence'
    item_key TEXT NOT NULL,              -- words.id (as text) or sentence_text
    media_id INTEGER REFERENCES media(id) ON DELETE SET NULL,
    reviewed_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX idx_review_sessions_media ON review_sessions(media_id);
CREATE INDEX idx_review_log_session ON review_log(session_id);
CREATE INDEX idx_review_log_item ON review_log(review_type, item_key);
CREATE INDEX idx_review_log_media ON review_log(media_id);