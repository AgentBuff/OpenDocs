-- Server-authoritative history.  The browser sends only a history intent;
-- before/after snapshot keys stay private to the server and let a request be
-- resolved in O(1) without rebuilding the whole document journal.
ALTER TABLE document_transactions ADD COLUMN origin TEXT NOT NULL DEFAULT 'local';
ALTER TABLE document_transactions ADD COLUMN operations_json TEXT NOT NULL DEFAULT '[]';

CREATE TABLE document_history (
    history_id          INTEGER PRIMARY KEY AUTOINCREMENT,
    document_id         TEXT NOT NULL,
    transaction_id      TEXT NOT NULL,
    before_snapshot_key TEXT NOT NULL,
    after_snapshot_key  TEXT NOT NULL,
    changed_blocks_json TEXT NOT NULL,
    structure_changed   INTEGER NOT NULL,
    is_undone           INTEGER NOT NULL DEFAULT 0,
    created_at          TEXT NOT NULL,
    UNIQUE (document_id, transaction_id),
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
);

CREATE INDEX idx_document_history_undo
    ON document_history(document_id, is_undone, history_id DESC);

CREATE INDEX idx_document_history_redo
    ON document_history(document_id, is_undone, history_id ASC);
