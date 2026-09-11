CREATE TABLE artifact_review_threads (
    thread_id TEXT PRIMARY KEY NOT NULL,
    artifact_id TEXT NOT NULL,
    kind TEXT NOT NULL CHECK (kind IN ('comment', 'suggestion')),
    state TEXT NOT NULL CHECK (state IN ('open', 'resolved', 'accepted', 'rejected')),
    author_id TEXT NOT NULL,
    anchor_json TEXT NOT NULL,
    base_revision INTEGER NOT NULL CHECK (base_revision >= 0),
    suggestion_json TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (artifact_id) REFERENCES artifacts(id) ON DELETE CASCADE
);

CREATE INDEX artifact_review_threads_artifact_updated
    ON artifact_review_threads (artifact_id, updated_at, thread_id);

CREATE TABLE artifact_review_messages (
    message_id TEXT PRIMARY KEY NOT NULL,
    thread_id TEXT NOT NULL,
    author_id TEXT NOT NULL,
    body TEXT NOT NULL,
    mentions_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (thread_id) REFERENCES artifact_review_threads(thread_id) ON DELETE CASCADE
);

CREATE INDEX artifact_review_messages_thread_created
    ON artifact_review_messages (thread_id, created_at, message_id);
