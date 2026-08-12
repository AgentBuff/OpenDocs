-- Native claim/lease state for durable event delivery.
--
-- A consumer owns an event only while status=processing, claimed_by matches
-- its stable worker id, and lease_until is in the future. Expired leases are
-- returned to pending by the next claim operation. The table is rebuilt here
-- because SQLite cannot alter a CHECK constraint in place.
CREATE TABLE domain_event_outbox_new (
    event_id       TEXT PRIMARY KEY,
    document_id    TEXT NOT NULL,
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
    UNIQUE (document_id, transaction_id, event_id),
    FOREIGN KEY (document_id) REFERENCES documents(id) ON DELETE CASCADE
);

INSERT INTO domain_event_outbox_new (
    event_id, document_id, transaction_id, revision, type_id, payload_json,
    status, attempts, available_at, delivered_at, last_error, created_at
)
SELECT event_id, document_id, transaction_id, revision, type_id, payload_json,
       status, attempts, available_at, delivered_at, last_error, created_at
FROM domain_event_outbox;

DROP TABLE domain_event_outbox;
ALTER TABLE domain_event_outbox_new RENAME TO domain_event_outbox;

CREATE INDEX idx_domain_event_outbox_pending
    ON domain_event_outbox(status, available_at, created_at, event_id);

CREATE INDEX idx_domain_event_outbox_lease
    ON domain_event_outbox(status, lease_until, created_at, event_id);

CREATE INDEX idx_domain_event_outbox_transaction
    ON domain_event_outbox(document_id, transaction_id, revision);
