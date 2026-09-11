ALTER TABLE artifact_review_threads
    ADD COLUMN accept_transaction_id TEXT NOT NULL DEFAULT '';

UPDATE artifact_review_threads
SET accept_transaction_id = lower(hex(randomblob(16)))
WHERE accept_transaction_id = '';
