-- Keep the authenticated author distinct from the client-local actor/session
-- identifier. author_id is always supplied by server authentication;
-- client_actor_id is audit metadata and never grants authority.
ALTER TABLE artifact_transactions
ADD COLUMN client_actor_id TEXT NOT NULL DEFAULT 'legacy-client';
