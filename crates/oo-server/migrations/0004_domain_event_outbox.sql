-- Durable post-commit event outbox.  Rows are inserted in the same SQLite
-- transaction as the artifact pointer and transaction idempotency record.
-- Delivery is deliberately separate: a consumer may retry a pending row
-- without re-applying the document command.
CREATE TABLE domain_event_outbox (
    event_id       TEXT PRIMARY KEY,
    document_id    TEXT NOT NULL,
    transaction_id TEXT NOT NULL,
    revision       INTEGER NOT NULL,
    type_id        TEXT NOT NULL,
    payload_json   TEXT NOT NULL,
    status         TEXT NOT NULL DEFAULT 'pending'
                   CHECK (status IN ('pending', 'delivered')),
    attempts       INTEGER NOT NULL DEFAULT 0,
    available_at   TEXT NOT NULL,
    delivered_at   TEXT,
    last_error     TEXT,
    created_at     TEXT NOT NULL,
    UNIQUE (document_id, transaction_id, event_id),
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
);

CREATE INDEX idx_domain_event_outbox_pending
    ON domain_event_outbox(status, available_at, created_at, event_id);

CREATE INDEX idx_domain_event_outbox_transaction
    ON domain_event_outbox(document_id, transaction_id, revision);
