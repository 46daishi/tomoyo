ALTER TABLE word_sentences ADD COLUMN session_id INTEGER REFERENCES sessions(id) ON DELETE SET NULL;
ALTER TABLE lookup_events ADD COLUMN session_id INTEGER REFERENCES sessions(id) ON DELETE SET NULL;

CREATE INDEX idx_word_sentences_session ON word_sentences(session_id);
CREATE INDEX idx_lookup_events_session ON lookup_events(session_id);
