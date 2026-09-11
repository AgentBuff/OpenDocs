//! Mindmap transaction adapter.
//!
//! The module stays a narrow HTTP adapter around [`MindmapEngine`]: typed
//! semantic commands, immutable snapshots, verified asset references and
//! server-authoritative durable history, plus the shared revision/idempotency/
//! event-outbox guarantees. No renderer gestures and no JSON patches.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use oo_mindmap::{MindmapCommand, MindmapCommandBatch, MindmapEngine, MindmapEngineError};
use oo_protocol::{
    ArtifactCommandEnvelope, CommandRecord, CommitResult, DomainEventRecord, EntityRef,
    HistoryAction, Invalidation, MindmapHistoryOperation, MutationRecord, TransactionOrigin,
    CURRENT_PROTOCOL_VERSION, MINDMAP_HISTORY_TYPE_ID,
};
use oo_schema::{ArtifactEnvelope, ArtifactPayload, AssetReferenceSource, MindmapModel};

use crate::artifact_routes::{artifact_for, load_artifact};
use crate::auth::CurrentUser;
use crate::db::{self, ArtifactKind};
use crate::document_support::TransactionCommit;
use crate::error::AppError;
use crate::transaction_kernel;
use crate::AppState;

/// Applies a typed mindmap command batch under the canonical transaction
/// guarantees shared with Document and Presentation.
pub async fn submit_transaction(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<TransactionCommit>, AppError> {
    let prepared = transaction_kernel::prepare(&headers, &id, &body)?;
    let history = mindmap_history_operation(&prepared.envelope)?;

    let _write_guard = state.write_lock.lock().await;
    let (meta, snapshot) =
        transaction_kernel::load_target(&state, &user, &id, ArtifactKind::Mindmap).await?;
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
    if let Some(history) = history {
        return submit_history_transaction(
            &state,
            &user,
            &id,
            &transaction,
            expected_version,
            &transaction_id,
            history,
        )
        .await;
    }

    let ArtifactPayload::Mindmap(model) = snapshot.payload else {
        return Err(AppError::Internal(
            "Mindmap 记录包含非 Mindmap Artifact".into(),
        ));
    };
    let commands_json = serde_json::to_string(&transaction.commands)
        .map_err(|error| AppError::Internal(format!("事务记录序列化失败：{error}")))?;
    let commands = transaction
        .commands
        .iter()
        .cloned()
        .map(mindmap_command_from_record)
        .collect::<Result<Vec<_>, _>>()?;
    let mut engine =
        MindmapEngine::new(model, transaction.base_revision).map_err(mindmap_engine_error)?;
    let change_set = engine
        .execute(MindmapCommandBatch {
            base_revision: transaction.base_revision,
            commands,
        })
        .map_err(mindmap_engine_error)?;
    let revision = change_set.revision;

    let artifact = artifact_for(
        &id,
        revision,
        ArtifactPayload::Mindmap(engine.model().clone()),
    )?;
    let bytes = serde_json::to_vec(&artifact)
        .map_err(|error| AppError::Internal(format!("序列化 Mindmap Artifact 失败：{error}")))?;
    let version = i64::try_from(revision)
        .map_err(|_| AppError::Internal("Mindmap revision 超出服务端范围".into()))?;
    let snapshot_key = crate::document_support::artifact_snapshot_key(&id, version);
    state.store.put(&snapshot_key, &bytes).await?;

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
    let referenced_assets = engine.model().asset_references();

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
    let (can_undo, can_redo) = db::artifact_history_state(&state.pool, &id).await?;
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
        can_undo,
        can_redo,
    }))
}

fn mindmap_history_operation(
    transaction: &ArtifactCommandEnvelope,
) -> Result<Option<MindmapHistoryOperation>, AppError> {
    let has_history = transaction
        .commands
        .iter()
        .any(|record| record.type_id == MINDMAP_HISTORY_TYPE_ID);
    if !has_history {
        if matches!(
            transaction.origin,
            TransactionOrigin::Undo | TransactionOrigin::Redo
        ) {
            return Err(AppError::BadRequest(
                "Undo/Redo transaction 必须使用 mindmap.history command".into(),
            ));
        }
        return Ok(None);
    }
    if transaction.commands.len() != 1 {
        return Err(AppError::BadRequest(
            "mindmap.history transaction 只能包含一个 command".into(),
        ));
    }
    let operation =
        MindmapHistoryOperation::from_record(&transaction.commands[0]).map_err(|error| {
            AppError::BadRequest(format!("Mindmap history operation 无效：{error}"))
        })?;
    let expected_origin = match operation.action {
        HistoryAction::Undo => TransactionOrigin::Undo,
        HistoryAction::Redo => TransactionOrigin::Redo,
    };
    if transaction.origin != expected_origin {
        return Err(AppError::BadRequest(
            "history action 与 transaction origin 不一致".into(),
        ));
    }
    Ok(Some(operation))
}

async fn submit_history_transaction(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
    transaction: &ArtifactCommandEnvelope,
    expected_version: i64,
    transaction_id: &str,
    history: MindmapHistoryOperation,
) -> Result<Json<TransactionCommit>, AppError> {
    let expected_undone = matches!(history.action, HistoryAction::Redo);
    let entry = db::get_artifact_history(&state.pool, id, expected_undone)
        .await?
        .ok_or_else(|| {
            AppError::BadRequest(match history.action {
                HistoryAction::Undo => "没有可撤销的 Mindmap 事务".into(),
                HistoryAction::Redo => "没有可重做的 Mindmap 事务".into(),
            })
        })?;
    let source = db::get_artifact_transaction(&state.pool, id, &entry.transaction_id)
        .await?
        .ok_or_else(|| AppError::Internal("Mindmap history 缺少原始事务记录".into()))?;
    let source_base_revision = u64::try_from(source.base_version)
        .map_err(|_| AppError::Internal("Mindmap history base revision 超出范围".into()))?;
    let command_records: Vec<CommandRecord> =
        serde_json::from_str(&source.commands_json).map_err(|error| {
            AppError::Internal(format!("Mindmap history commands JSON 无效：{error}"))
        })?;
    let commands = command_records
        .into_iter()
        .map(mindmap_command_from_record)
        .collect::<Result<Vec<_>, _>>()?;
    if commands.is_empty() {
        return Err(AppError::BadRequest(
            "该 Mindmap 历史条目不包含可重放的语义 command".into(),
        ));
    }
    let before =
        load_mindmap_from_snapshot_key(state, user, id, &entry.before_snapshot_key).await?;
    let mut engine =
        MindmapEngine::new(before, source_base_revision).map_err(mindmap_engine_error)?;
    engine
        .execute(MindmapCommandBatch {
            base_revision: source_base_revision,
            commands,
        })
        .map_err(mindmap_engine_error)?;
    let change_set = match history.action {
        HistoryAction::Undo => engine
            .undo(source_base_revision + 1)
            .map_err(mindmap_engine_error)?,
        HistoryAction::Redo => {
            engine
                .undo(source_base_revision + 1)
                .map_err(mindmap_engine_error)?;
            engine
                .redo(source_base_revision + 2)
                .map_err(mindmap_engine_error)?
        }
    };
    let revision = u64::try_from(
        expected_version
            .checked_add(1)
            .ok_or_else(|| AppError::Internal("Mindmap revision 溢出".into()))?,
    )
    .map_err(|_| AppError::Internal("Mindmap revision 超出协议范围".into()))?;
    let artifact = artifact_for(
        id,
        revision,
        ArtifactPayload::Mindmap(engine.model().clone()),
    )?;
    let bytes = serde_json::to_vec(&artifact)
        .map_err(|error| AppError::Internal(format!("序列化 Mindmap Artifact 失败：{error}")))?;
    let snapshot_key = crate::document_support::artifact_snapshot_key(id, revision as i64);
    state.store.put(&snapshot_key, &bytes).await?;

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
    events.push(DomainEventRecord {
        event_id: format!("{transaction_id}:{revision}:history"),
        type_id: "mindmap.historyApplied".into(),
        payload: serde_json::json!({
            "action": match history.action {
                HistoryAction::Undo => "undo",
                HistoryAction::Redo => "redo",
            },
            "historyId": entry.history_id,
            "sourceTransactionId": entry.transaction_id,
            "actorId": user.id,
        }),
    });
    let commands_json = serde_json::to_string(&transaction.commands)
        .map_err(|error| AppError::Internal(format!("事务记录序列化失败：{error}")))?;
    let referenced_assets = engine.model().asset_references();
    let committed = transaction_kernel::commit_history_candidate(
        state,
        &snapshot_key,
        db::ArtifactHistoryCommit {
            id,
            expected_version,
            snapshot_key: &snapshot_key,
            transaction_id,
            author_id: &user.id,
            client_actor_id: &transaction.actor_id,
            changed_entities: &changed_entities,
            structure_changed: change_set.invalidation.structure_changed,
            origin: transaction_origin_name(transaction.origin),
            commands_json: &commands_json,
            history_id: entry.history_id,
            expected_undone,
            next_undone: !expected_undone,
            events: &events,
        },
        &referenced_assets,
    )
    .await?;
    let (can_undo, can_redo) = db::artifact_history_state(&state.pool, id).await?;
    Ok(Json(TransactionCommit {
        result: CommitResult {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            artifact_id: id.into(),
            transaction_id: transaction_id.into(),
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
        can_undo,
        can_redo,
    }))
}

async fn load_mindmap_from_snapshot_key(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
    snapshot_key: &str,
) -> Result<MindmapModel, AppError> {
    let (meta, _) = load_artifact(state, user, id).await?;
    if meta.kind != ArtifactKind::Mindmap {
        return Err(AppError::UnsupportedArtifact(meta.kind));
    }
    let bytes = state.store.get(snapshot_key).await?;
    let artifact: ArtifactEnvelope = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::Internal(format!("历史 Mindmap Artifact JSON 无效：{error}")))?;
    artifact
        .validate()
        .map_err(|error| AppError::Internal(format!("历史 Mindmap Artifact 无效：{error}")))?;
    if artifact.artifact_id != id {
        return Err(AppError::Internal("历史 Mindmap Artifact id 不一致".into()));
    }
    match artifact.payload {
        ArtifactPayload::Mindmap(model) => Ok(model),
        _ => Err(AppError::Internal(
            "历史 snapshot 不是 Mindmap Artifact".into(),
        )),
    }
}

fn mindmap_command_from_record(record: CommandRecord) -> Result<MindmapCommand, AppError> {
    let command: MindmapCommand = serde_json::from_value(record.payload)
        .map_err(|error| AppError::BadRequest(format!("Mindmap command 无效：{error}")))?;
    let expected_type_id = command.type_id();
    if record.type_id != expected_type_id {
        return Err(AppError::BadRequest(format!(
            "命令 typeId {} 与 payload 不符，应为 {expected_type_id}",
            record.type_id
        )));
    }
    Ok(command)
}

fn mindmap_engine_error(error: MindmapEngineError) -> AppError {
    match error {
        MindmapEngineError::RevisionConflict { .. } => AppError::VersionConflict,
        other => AppError::BadRequest(format!("Mindmap 事务无法应用：{other}")),
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
            entity_type: "mindmap.graph".into(),
            entity_id: "graph".into(),
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
