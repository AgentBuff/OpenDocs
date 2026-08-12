CREATE TABLE documents (
    id           TEXT PRIMARY KEY,
    title        TEXT NOT NULL,
    owner_id     TEXT NOT NULL,
    size         INTEGER NOT NULL,
    version      INTEGER NOT NULL DEFAULT 1,
    source_key   TEXT NOT NULL,
    snapshot_key TEXT NOT NULL,
    starred      INTEGER NOT NULL DEFAULT 0,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);

CREATE INDEX idx_documents_owner ON documents(owner_id, updated_at DESC);

CREATE TABLE document_transactions (
    document_id         TEXT NOT NULL,
    transaction_id      TEXT NOT NULL,
    author_id           TEXT NOT NULL,
    base_version        INTEGER NOT NULL,
    version             INTEGER NOT NULL,
    changed_blocks_json TEXT NOT NULL,
    structure_changed   INTEGER NOT NULL,
    created_at          TEXT NOT NULL,
    PRIMARY KEY (document_id, transaction_id),
    UNIQUE (document_id, version),
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
);

CREATE INDEX idx_document_transactions_replay
    ON document_transactions(document_id, version);
