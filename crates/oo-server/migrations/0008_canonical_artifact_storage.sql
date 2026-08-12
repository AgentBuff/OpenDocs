-- Canonical multi-Artifact metadata registry.
-- The preceding migrations created Document-specific tables.  This migration
-- performs the one-way schema cutover: existing rows are copied as
-- kind='document', then the old tables are removed.  Runtime SQL must only use
-- the artifact_* tables below; no compatibility views or dual writes exist.

CREATE TABLE artifacts (
    id           TEXT PRIMARY KEY,
    kind         TEXT NOT NULL CHECK (kind IN ('document', 'spreadsheet', 'presentation', 'mindmap', 'whiteboard')),
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

CREATE INDEX idx_artifacts_owner ON artifacts(owner_id, updated_at DESC);

INSERT INTO artifacts (id, kind, title, owner_id, size, version, source_key, snapshot_key, starred, created_at, updated_at)
SELECT id, 'document', title, owner_id, size, version, source_key, snapshot_key, starred, created_at, updated_at
FROM documents;

CREATE TABLE artifact_snapshots (
    artifact_id  TEXT NOT NULL,
    version      INTEGER NOT NULL,
    snapshot_key TEXT NOT NULL,
    created_at   TEXT NOT NULL,
    PRIMARY KEY (artifact_id, version),
    UNIQUE (snapshot_key),
    FOREIGN KEY (artifact_id) REFERENCES artifacts(id) ON DELETE CASCADE
);

INSERT INTO artifact_snapshots (artifact_id, version, snapshot_key, created_at)
SELECT document_id, version, snapshot_key, created_at
FROM document_snapshots;

CREATE INDEX idx_artifact_snapshots_artifact
    ON artifact_snapshots(artifact_id, version DESC);

CREATE TABLE artifact_transactions (
    artifact_id         TEXT NOT NULL,
    transaction_id       TEXT NOT NULL,
    author_id            TEXT NOT NULL,
    base_version        INTEGER NOT NULL,
    version             INTEGER NOT NULL,
    changed_entities_json TEXT NOT NULL,
    structure_changed   INTEGER NOT NULL,
    origin              TEXT NOT NULL DEFAULT 'local',
    commands_json       TEXT NOT NULL DEFAULT '[]',
    created_at          TEXT NOT NULL,
    PRIMARY KEY (artifact_id, transaction_id),
    UNIQUE (artifact_id, version),
    FOREIGN KEY (artifact_id) REFERENCES artifacts(id) ON DELETE CASCADE
);

INSERT INTO artifact_transactions (
    artifact_id, transaction_id, author_id, base_version, version,
    changed_entities_json, structure_changed, origin, commands_json, created_at
)
SELECT document_id, transaction_id, author_id, base_version, version,
       changed_blocks_json, structure_changed, origin, commands_json, created_at
FROM document_transactions;

CREATE INDEX idx_artifact_transactions_replay
    ON artifact_transactions(artifact_id, version);

CREATE TABLE artifact_history (
    history_id          INTEGER PRIMARY KEY AUTOINCREMENT,
    artifact_id         TEXT NOT NULL,
    transaction_id       TEXT NOT NULL,
    before_snapshot_key TEXT NOT NULL,
    after_snapshot_key  TEXT NOT NULL,
    changed_entities_json TEXT NOT NULL,
    structure_changed   INTEGER NOT NULL,
    is_undone           INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL,
    UNIQUE (artifact_id, transaction_id),
    FOREIGN KEY (artifact_id) REFERENCES artifacts(id) ON DELETE CASCADE
);

INSERT INTO artifact_history (
    history_id, artifact_id, transaction_id, before_snapshot_key,
    after_snapshot_key, changed_entities_json, structure_changed, is_undone, created_at
)
SELECT history_id, document_id, transaction_id, before_snapshot_key,
       after_snapshot_key, changed_blocks_json, structure_changed, is_undone, created_at
FROM document_history;

CREATE INDEX idx_artifact_history_undo
    ON artifact_history(artifact_id, is_undone, history_id DESC);

CREATE INDEX idx_artifact_history_redo
    ON artifact_history(artifact_id, is_undone, history_id ASC);

CREATE TABLE artifact_event_outbox (
    event_id       TEXT PRIMARY KEY,
    artifact_id    TEXT NOT NULL,
    transaction_id TEXT NOT NULL,
    revision       INTEGER NOT NULL,
    type_id        TEXT NOT NULL,
    payload_json   TEXT NOT NULL,
    status         TEXT NOT NULL DEFAULT 'pending'
                   CHECK (status IN ('pending', 'processing', 'delivered')),
    attempts       INTEGER NOT NULL DEFAULT 0,
    available_at   TEXT NOT NULL,
    claimed_by     TEXT,
    lease_until    TEXT,
    delivered_at   TEXT,
    last_error     TEXT,
    created_at     TEXT NOT NULL,
    UNIQUE (artifact_id, transaction_id, event_id),
    FOREIGN KEY (artifact_id) REFERENCES artifacts(id) ON DELETE CASCADE
);

INSERT INTO artifact_event_outbox (
    event_id, artifact_id, transaction_id, revision, type_id, payload_json,
    status, attempts, available_at, claimed_by, lease_until, delivered_at,
    last_error, created_at
)
SELECT event_id, document_id, transaction_id, revision, type_id, payload_json,
       status, attempts, available_at, claimed_by, lease_until, delivered_at,
       last_error, created_at
FROM domain_event_outbox;

CREATE INDEX idx_artifact_event_outbox_pending
    ON artifact_event_outbox(status, available_at, created_at, event_id);

CREATE INDEX idx_artifact_event_outbox_lease
    ON artifact_event_outbox(status, lease_until, created_at, event_id);

CREATE INDEX idx_artifact_event_outbox_transaction
    ON artifact_event_outbox(artifact_id, transaction_id, revision);

DROP TABLE domain_event_outbox;
DROP TABLE document_history;
DROP TABLE document_transactions;
DROP TABLE document_snapshots;
DROP TABLE documents;
