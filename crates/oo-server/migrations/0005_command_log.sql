-- The transaction journal stores the semantic command batch that produced a
-- snapshot.  The previous `operations_json` name belonged to the removed
-- operation-log design and must not remain in the canonical schema.
ALTER TABLE document_transactions RENAME COLUMN operations_json TO commands_json;
