CREATE TABLE document_snapshots (
    document_id  TEXT NOT NULL,
    version      INTEGER NOT NULL,
    snapshot_key TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    PRIMARY KEY (document_id, version),
    UNIQUE (snapshot_key),
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
);

INSERT INTO document_snapshots (document_id, version, snapshot_key, created_at)
SELECT id, version, snapshot_key, updated_at
FROM documents;

CREATE INDEX idx_document_snapshots_document
    ON document_snapshots(document_id, version DESC);
