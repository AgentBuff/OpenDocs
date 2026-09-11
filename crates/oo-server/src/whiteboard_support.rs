//! Whiteboard transaction adapter.
//!
//! Same narrow-adapter contract as [`crate::mindmap_support`]: typed semantic
//! commands only, immutable snapshots, shared revision/idempotency/outbox
//! guarantees, actor-attributed events. No renderer gestures and no JSON
//! patches; camera state is engine-owned per the Whiteboard model.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use oo_protocol::{
    CommandRecord, CommitResult, DomainEventRecord, EntityRef, Invalidation, MutationRecord,
    TransactionOrigin, CURRENT_PROTOCOL_VERSION,
};
use oo_schema::{ArtifactPayload, AssetReferenceSource};
use oo_whiteboard::{
    WhiteboardCommand, WhiteboardCommandBatch, WhiteboardEngine, WhiteboardEngineError,
};

use crate::artifact_routes::artifact_for;
use crate::auth::CurrentUser;
use crate::db::{self, ArtifactKind};
use crate::document_support::TransactionCommit;
use crate::error::AppError;
use crate::transaction_kernel;
use crate::AppState;

pub async fn submit_transaction(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<TransactionCommit>, AppError> {
    let prepared = transaction_kernel::prepare(&headers, &id, &body)?;

    let _write_guard = state.write_lock.lock().await;
    let (meta, snapshot) =
        transaction_kernel::load_target(&state, &user, &id, ArtifactKind::Whiteboard).await?;
    if let Some(replay) = transaction_kernel::replay_if_committed(
        &state,
        &id,
        &meta,
        &prepared.transaction_id,
        invalidation_from_keys,
    )
    .await?
    {
        return Ok(Json(replay));
    }
    transaction_kernel::require_current_revision(&state, &meta, &id, prepared.expected_revision)
        .await?;
    let transaction = prepared.envelope;
    let transaction_id = prepared.transaction_id;
    let expected_version = prepared.expected_version;

    let ArtifactPayload::Whiteboard(model) = snapshot.payload else {
        return Err(AppError::Internal(
            "Whiteboard 记录包含非 Whiteboard Artifact".into(),
        ));
    };
    let commands_json = serde_json::to_string(&transaction.commands)
        .map_err(|error| AppError::Internal(format!("事务记录序列化失败：{error}")))?;
    let commands = transaction
        .commands
        .iter()
        .cloned()
        .map(whiteboard_command_from_record)
        .collect::<Result<Vec<_>, _>>()?;
    let mut engine =
        WhiteboardEngine::new(model, transaction.base_revision).map_err(whiteboard_engine_error)?;
    let change_set = engine
        .execute(WhiteboardCommandBatch {
            base_revision: transaction.base_revision,
            commands,
        })
        .map_err(whiteboard_engine_error)?;
    let revision = change_set.revision;

    let artifact = artifact_for(
        &id,
        revision,
        ArtifactPayload::Whiteboard(engine.model().clone()),
    )?;
    let bytes = serde_json::to_vec(&artifact)
        .map_err(|error| AppError::Internal(format!("序列化 Whiteboard Artifact 失败：{error}")))?;
    let version = i64::try_from(revision)
        .map_err(|_| AppError::Internal("Whiteboard revision 超出服务端范围".into()))?;
    let snapshot_key = crate::document_support::artifact_snapshot_key(&id, version);
    state.store.put(&snapshot_key, &bytes).await?;
    let referenced_assets = engine.model().asset_references();

    let blobs = db::get_artifact_blob_keys(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Artifact {id} 不存在")))?;
    let changed_entities = entity_keys(&change_set.invalidation);
    let mut events = change_set
        .mutations
        .iter()
        .enumerate()
        .map(|(index, mutation)| {
            let MutationRecord { type_id, payload } = mutation
                .to_record()
                .map_err(|error| AppError::Internal(format!("Mutation 序列化失败：{error}")))?;
            Ok(DomainEventRecord {
                event_id: format!("{transaction_id}:{revision}:{index}"),
                type_id,
                payload,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    transaction_kernel::stamp_events(&mut events, &user.id);

    let committed = transaction_kernel::commit_candidate(
        &state,
        &snapshot_key,
        db::ArtifactTransactionCommit {
            id: &id,
            expected_version,
            snapshot_key: &snapshot_key,
            transaction_id: &transaction_id,
            author_id: &user.id,
            client_actor_id: &transaction.actor_id,
            changed_entities: &changed_entities,
            structure_changed: change_set.invalidation.structure_changed,
            origin: transaction_origin_name(transaction.origin),
            commands_json: &commands_json,
            before_snapshot_key: &blobs.snapshot_key,
            events: &events,
        },
        &referenced_assets,
    )
    .await?;
    // Whiteboard has no server-authoritative history yet; affordances stay
    // false instead of pretending an undo path exists.
    Ok(Json(TransactionCommit {
        result: CommitResult {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            artifact_id: id,
            transaction_id,
            revision,
            invalidation: change_set.invalidation,
            mutations: change_set
                .mutations
                .iter()
                .map(|mutation| {
                    mutation.to_record().map_err(|error| {
                        AppError::Internal(format!("Mutation 序列化失败：{error}"))
                    })
                })
                .collect::<Result<Vec<_>, AppError>>()?,
            events,
        },
        document: committed,
        can_undo: false,
        can_redo: false,
    }))
}

fn whiteboard_command_from_record(record: CommandRecord) -> Result<WhiteboardCommand, AppError> {
    let command: WhiteboardCommand = serde_json::from_value(record.payload)
        .map_err(|error| AppError::BadRequest(format!("Whiteboard command 无效：{error}")))?;
    let expected_type_id = match &command {
        WhiteboardCommand::AddElement { .. } => "whiteboard.addElement",
        WhiteboardCommand::UpdateElement { .. } => "whiteboard.updateElement",
        WhiteboardCommand::DeleteElement { .. } => "whiteboard.deleteElement",
        WhiteboardCommand::SetCamera { .. } => "whiteboard.setCamera",
        WhiteboardCommand::PanCamera { .. } => "whiteboard.panCamera",
        WhiteboardCommand::ZoomCamera { .. } => "whiteboard.zoomCamera",
    };
    if record.type_id != expected_type_id {
        return Err(AppError::BadRequest(format!(
            "命令 typeId {} 与 payload 不符，应为 {expected_type_id}",
            record.type_id
        )));
    }
    Ok(command)
}

fn whiteboard_engine_error(error: WhiteboardEngineError) -> AppError {
    match error {
        WhiteboardEngineError::RevisionConflict { .. } => AppError::VersionConflict,
        other => AppError::BadRequest(format!("Whiteboard 事务无法应用：{other}")),
    }
}

fn entity_keys(invalidation: &Invalidation) -> Vec<String> {
    invalidation
        .changed_entities
        .iter()
        .map(|entity| format!("{}\u{1f}{}", entity.entity_type, entity.entity_id))
        .collect()
}

fn invalidation_from_keys(keys: &[String], structure_changed: bool) -> Invalidation {
    let changed_entities = keys
        .iter()
        .filter_map(|key| {
            let (entity_type, entity_id) = key.split_once('\u{1f}')?;
            Some(EntityRef {
                entity_type: entity_type.into(),
                entity_id: entity_id.into(),
            })
        })
        .collect();
    Invalidation {
        changed_entities,
        changed_containers: vec![EntityRef {
            entity_type: "whiteboard.scene".into(),
            entity_id: "scene".into(),
        }],
        structure_changed,
    }
}

fn transaction_origin_name(origin: TransactionOrigin) -> &'static str {
    match origin {
        TransactionOrigin::Local => "local",
        TransactionOrigin::Remote => "remote",
        TransactionOrigin::Undo => "undo",
        TransactionOrigin::Redo => "redo",
        TransactionOrigin::Import => "import",
        TransactionOrigin::System => "system",
    }
}
