-- C4 access control, first slice: per-artifact collaborators.
--
-- The owner remains recorded on `artifacts.owner_id` and always keeps full
-- control; this table only grants the two delegated roles. There is
-- deliberately no workspace/role table yet — that lands with the full C4
-- product layer. All deletions cascade with the artifact.

CREATE TABLE artifact_collaborators (
    artifact_id TEXT NOT NULL,
    user_id     TEXT NOT NULL,
    role        TEXT NOT NULL CHECK (role IN ('editor', 'viewer')),
    created_at  TEXT NOT NULL,
    PRIMARY KEY (artifact_id, user_id),
    FOREIGN KEY (artifact_id) REFERENCES artifacts(id) ON DELETE CASCADE
);

CREATE INDEX idx_artifact_collaborators_user ON artifact_collaborators(user_id);
