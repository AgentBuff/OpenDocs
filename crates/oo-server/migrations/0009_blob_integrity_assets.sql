-- Content-addressed integrity registry. The object store remains the byte
-- source of truth; SQLite stores the checksum/size needed to detect torn
-- writes, disk corruption and stale pointers after restart.
CREATE TABLE artifact_blob_integrity (
    object_key   TEXT PRIMARY KEY,
    artifact_id  TEXT NOT NULL,
    object_kind  TEXT NOT NULL CHECK (object_kind IN ('source', 'snapshot', 'asset')),
    checksum     TEXT NOT NULL,
    size         INTEGER NOT NULL CHECK (size >= 0),
    verified_at  TEXT NOT NULL,
    FOREIGN KEY (artifact_id) REFERENCES artifacts(id) ON DELETE CASCADE
);

CREATE INDEX idx_blob_integrity_artifact ON artifact_blob_integrity(artifact_id, object_kind);

CREATE TABLE artifact_assets (
    artifact_id  TEXT NOT NULL,
    asset_id     TEXT NOT NULL,
    object_key   TEXT NOT NULL UNIQUE,
    content_type TEXT NOT NULL,
    file_name    TEXT NOT NULL,
    checksum     TEXT NOT NULL,
    size         INTEGER NOT NULL CHECK (size >= 0),
    ref_count    INTEGER NOT NULL DEFAULT 0 CHECK (ref_count >= 0),
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL,
    PRIMARY KEY (artifact_id, asset_id),
    FOREIGN KEY (artifact_id) REFERENCES artifacts(id) ON DELETE CASCADE
);

CREATE INDEX idx_artifact_assets_gc ON artifact_assets(ref_count, updated_at);
