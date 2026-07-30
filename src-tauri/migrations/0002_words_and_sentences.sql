CREATE TABLE words (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    spelling TEXT NOT NULL,
    reading TEXT NOT NULL,
    definitions TEXT NOT NULL,      -- JSON array of strings
    word_type TEXT,
    notes TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE word_tags (
    word_id INTEGER NOT NULL REFERENCES words(id) ON DELETE CASCADE,
    tag TEXT NOT NULL,
    PRIMARY KEY (word_id, tag)
);

CREATE TABLE word_sentences (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    word_id INTEGER NOT NULL REFERENCES words(id) ON DELETE CASCADE,
    sentence_text TEXT NOT NULL,
    highlight_start INTEGER NOT NULL,
    highlight_end INTEGER NOT NULL,
    media_id INTEGER REFERENCES media(id) ON DELETE SET NULL,
    translation TEXT,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE TABLE sentences (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    sentence_text TEXT NOT NULL,
    tag TEXT,
    translation TEXT,
    media_id INTEGER REFERENCES media(id) ON DELETE SET NULL,
    created_at INTEGER NOT NULL DEFAULT (unixepoch())
);

CREATE INDEX idx_words_spelling_reading ON words(spelling, reading);
CREATE INDEX idx_word_sentences_word_id ON word_sentences(word_id);
CREATE INDEX idx_sentences_media_id ON sentences(media_id);