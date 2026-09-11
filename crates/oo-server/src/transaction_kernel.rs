//! Shared transport and persistence kernel for Artifact transactions.
//!
//! Domain adapters still decode commands, execute their canonical engine and
//! build invalidations/assets. This module owns only the cross-Artifact HTTP,
//! authorization, idempotency, conflict and candidate-commit invariants.

use axum::http::HeaderMap;
use oo_protocol::{ArtifactCommandEnvelope, CommitResult, DomainEventRecord, Invalidation};
use oo_schema::{ArtifactEnvelope, AssetReference};

use crate::artifact_routes::{authorize, load_artifact, Role};
use crate::auth::CurrentUser;
use crate::db::{self, ArtifactKind, ArtifactMeta};
use crate::document_support::TransactionCommit;
use crate::error::{AppError, ConflictDetails};
use crate::{request_context, AppState};

pub struct PreparedTransaction {
    pub envelope: ArtifactCommandEnvelope,
    pub transaction_id: String,
    pub expected_version: i64,
    pub expected_revision: u64,
}

pub fn prepare(
    headers: &HeaderMap,
    path_id: &str,
    body: &str,
) -> Result<PreparedTransaction, AppError> {
    let envelope: ArtifactCommandEnvelope = serde_json::from_str(body)
        .map_err(|error| AppError::BadRequest(format!("事务 JSON 无效：{error}")))?;
    envelope
        .validate()
        .map_err(|error| AppError::BadRequest(format!("事务协议无效：{error}")))?;
    if envelope.artifact_id != path_id {
        return Err(AppError::BadRequest(
            "事务 artifactId 必须与请求路径一致".into(),
        ));
    }
    let context = request_context::transaction(headers, &envelope)?;
    if let Some(request_id) = context.request_id.as_deref() {
        tracing::debug!(
            request_id,
            artifact_id = path_id,
            "accepted transaction request id"
        );
    }
    let expected_revision = context.expected_revision;
    let expected_version = i64::try_from(expected_revision)
        .map_err(|_| AppError::BadRequest("事务 baseRevision 超出服务端范围".into()))?;
    Ok(PreparedTransaction {
        envelope,
        transaction_id: context.transaction_id,
        expected_version,
        expected_revision,
    })
}

pub async fn load_target(
    state: &AppState,
    user: &CurrentUser,
    artifact_id: &str,
    expected_kind: ArtifactKind,
) -> Result<(ArtifactMeta, ArtifactEnvelope), AppError> {
    authorize(state, user, artifact_id, Role::Editor).await?;
    let (meta, snapshot) = load_artifact(state, user, artifact_id).await?;
    if meta.kind != expected_kind {
        return Err(AppError::UnsupportedArtifact(meta.kind));
    }
    Ok((meta, snapshot))
}

pub async fn replay_if_committed<F>(
    state: &AppState,
    artifact_id: &str,
    meta: &ArtifactMeta,
    transaction_id: &str,
    invalidation: F,
) -> Result<Option<TransactionCommit>, AppError>
where
    F: FnOnce(&[String], bool) -> Invalidation,
{
    let Some(record) =
        db::get_artifact_transaction(&state.pool, artifact_id, transaction_id).await?
    else {
        return Ok(None);
    };
    let revision = u64::try_from(record.version)
        .map_err(|_| AppError::Internal("事务 revision 超出协议范围".into()))?;
    let (can_undo, can_redo) = db::artifact_history_state(&state.pool, artifact_id).await?;
    Ok(Some(TransactionCommit {
        result: CommitResult {
            protocol_version: oo_protocol::CURRENT_PROTOCOL_VERSION,
            artifact_id: artifact_id.into(),
            transaction_id: record.transaction_id,
            revision,
            invalidation: invalidation(&record.changed_entities, record.structure_changed),
            mutations: Vec::new(),
            events: Vec::new(),
        },
        document: meta.clone(),
        can_undo,
        can_redo,
    }))
}

pub async fn require_current_revision(
    state: &AppState,
    meta: &ArtifactMeta,
    artifact_id: &str,
    requested_revision: u64,
) -> Result<(), AppError> {
    if meta.version
        == i64::try_from(requested_revision)
            .map_err(|_| AppError::BadRequest("事务 baseRevision 超出服务端范围".into()))?
    {
        return Ok(());
    }
    Err(AppError::VersionConflictDetails(ConflictDetails {
        artifact_id: artifact_id.into(),
        requested_revision,
        current_revision: u64::try_from(meta.version)
            .map_err(|_| AppError::Internal("Artifact revision 超出协议范围".into()))?,
        changed_entities: db::changed_entities_since(&state.pool, artifact_id, requested_revision)
            .await?,
    }))
}

pub fn stamp_events(events: &mut [DomainEventRecord], principal_id: &str) {
    crate::events::stamp_actor(events, principal_id);
}

pub async fn commit_candidate(
    state: &AppState,
    snapshot_key: &str,
    commit: db::ArtifactTransactionCommit<'_>,
    references: &[AssetReference],
) -> Result<ArtifactMeta, AppError> {
    match db::commit_artifact_transaction_with_assets(&state.pool, commit, references).await {
        Ok(Some(meta)) => Ok(meta),
        Ok(None) => {
            discard_candidate_snapshot(state, snapshot_key).await;
            Err(AppError::VersionConflict)
        }
        Err(error) => {
            discard_candidate_snapshot(state, snapshot_key).await;
            Err(error.into())
        }
    }
}

pub async fn commit_history_candidate(
    state: &AppState,
    snapshot_key: &str,
    commit: db::ArtifactHistoryCommit<'_>,
    references: &[AssetReference],
) -> Result<ArtifactMeta, AppError> {
    match db::commit_artifact_history_with_assets(&state.pool, commit, references).await {
        Ok(Some(meta)) => Ok(meta),
        Ok(None) => {
            discard_candidate_snapshot(state, snapshot_key).await;
            Err(AppError::VersionConflict)
        }
        Err(error) => {
            discard_candidate_snapshot(state, snapshot_key).await;
            Err(error.into())
        }
    }
}

pub async fn discard_candidate_snapshot(state: &AppState, snapshot_key: &str) {
    if let Err(error) = state.store.delete(snapshot_key).await {
        tracing::warn!(snapshot_key, error = %error, "无法清理未提交的 Artifact snapshot");
    }
}
