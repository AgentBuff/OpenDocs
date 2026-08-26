//! Presentation v5 transaction adapter.
//!
//! This module is deliberately a narrow HTTP adapter around
//! [`PresentationEngine`].  It accepts only typed semantic commands, persists
//! immutable Artifact snapshots, and shares the canonical revision,
//! idempotency and event-outbox guarantees with Document.  It does not expose
//! renderer gestures, JSON patches, or a second editable deck model.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use oo_presentation::{
    PresentationCommand, PresentationCommandBatch, PresentationEngine, PresentationEngineError,
    PresentationMutation,
};
use oo_protocol::{
    ArtifactCommandEnvelope, CommandRecord, CommitResult, DomainEventRecord, EntityRef,
    HistoryAction, Invalidation, MutationRecord, PresentationHistoryOperation, TransactionOrigin,
    CURRENT_PROTOCOL_VERSION, PRESENTATION_HISTORY_TYPE_ID,
};
use oo_schema::{ArtifactEnvelope, ArtifactPayload};

use crate::artifact_routes::{artifact_for, load_artifact};
use crate::auth::CurrentUser;
use crate::db::{self, ArtifactKind};
use crate::document_support::TransactionCommit;
use crate::error::{AppError, ConflictDetails};
use crate::request_context;
use crate::AppState;

/// Applies a real v5 Presentation command batch. Undo/redo reconstruct the
/// P2 typed journal from the original durable semantic command record, then
/// persist the resulting immutable snapshot as a normal history transition.
pub async fn submit_transaction(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<TransactionCommit>, AppError> {
    let transaction: ArtifactCommandEnvelope = serde_json::from_str(&body)
        .map_err(|error| AppError::BadRequest(format!("事务 JSON 无效：{error}")))?;
    transaction
        .validate()
        .map_err(|error| AppError::BadRequest(format!("事务协议无效：{error}")))?;
    if transaction.artifact_id != id {
        return Err(AppError::BadRequest(
            "事务 artifactId 必须与请求路径一致".into(),
        ));
    }
    let request = request_context::transaction(&headers, &transaction)?;
    let transaction_id = request.transaction_id;
    let expected_version = i64::try_from(request.expected_revision)
        .map_err(|_| AppError::BadRequest("事务 baseRevision 超出服务端范围".into()))?;
    let history = presentation_history_operation(&transaction)?;

    let _write_guard = state.write_lock.lock().await;
    let (meta, snapshot) = load_artifact(&state, &user, &id).await?;
    if meta.kind != ArtifactKind::Presentation {
        return Err(AppError::UnsupportedArtifact(meta.kind));
    }
    if let Some(record) = db::get_artifact_transaction(&state.pool, &id, &transaction_id).await? {
        let revision = u64::try_from(record.version)
            .map_err(|_| AppError::Internal("事务 revision 超出协议范围".into()))?;
        let (can_undo, can_redo) = db::artifact_history_state(&state.pool, &id).await?;
        return Ok(Json(TransactionCommit {
            result: CommitResult {
                protocol_version: CURRENT_PROTOCOL_VERSION,
                artifact_id: id,
                transaction_id: record.transaction_id,
                revision,
                invalidation: presentation_invalidation_from_keys(
                    &record.changed_entities,
                    record.structure_changed,
                ),
                mutations: Vec::new(),
                events: Vec::new(),
            },
            document: meta,
            can_undo,
            can_redo,
        }));
    }
    if meta.version != expected_version {
        return Err(AppError::VersionConflictDetails(ConflictDetails {
            artifact_id: id,
            requested_revision: request.expected_revision,
            current_revision: u64::try_from(meta.version)
                .map_err(|_| AppError::Internal("Artifact revision 超出协议范围".into()))?,
            changed_entities: Vec::new(),
        }));
    }
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
    let ArtifactPayload::Presentation(deck) = snapshot.payload else {
        return Err(AppError::Internal(
            "Presentation 记录包含非 Presentation Artifact".into(),
        ));
    };
    let commands_json = serde_json::to_string(&transaction.commands)
        .map_err(|error| AppError::Internal(format!("事务记录序列化失败：{error}")))?;
    let commands = transaction
        .commands
        .iter()
        .cloned()
        .map(presentation_command_from_record)
        .collect::<Result<Vec<_>, _>>()?;
    // A Deck asset reference must be anchored to the verified binary metadata
    // stored for this artifact. The engine owns Deck consistency; the server
    // owns blob identity and must reject a client-supplied digest/MIME claim.
    validate_registered_assets(&state, &id, &commands).await?;
    let mut engine = PresentationEngine::new(deck, transaction.base_revision)
        .map_err(presentation_engine_error)?;
    let change_set = engine
        .execute(PresentationCommandBatch {
            base_revision: transaction.base_revision,
            commands,
        })
        .map_err(presentation_engine_error)?;
    let revision = change_set.revision;
    let snapshot_key =
        persist_candidate_snapshot(&state, &id, revision, engine.deck().clone()).await?;
    let blobs = db::get_artifact_blob_keys(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Artifact {id} 不存在")))?;
    let referenced_asset_ids = presentation_asset_ids(engine.deck());
    let changed_entities = presentation_entity_keys(&change_set.invalidation);
    let events = presentation_events(
        &transaction_id,
        revision,
        &change_set.mutations,
        &change_set.dirty_thumbnail_ids,
        &user.id,
    )?;
    let committed = match db::commit_artifact_transaction_with_assets(
        &state.pool,
        db::ArtifactTransactionCommit {
            id: &id,
            expected_version,
            snapshot_key: &snapshot_key,
            transaction_id: &transaction_id,
            author_id: &user.id,
            changed_entities: &changed_entities,
            structure_changed: change_set.invalidation.structure_changed,
            origin: transaction_origin_name(transaction.origin),
            commands_json: &commands_json,
            before_snapshot_key: &blobs.snapshot_key,
            events: &events,
        },
        &referenced_asset_ids,
    )
    .await
    {
        Ok(Some(meta)) => meta,
        Ok(None) => {
            discard_candidate_snapshot(&state, &snapshot_key).await;
            return Err(AppError::VersionConflict);
        }
        Err(error) => {
            discard_candidate_snapshot(&state, &snapshot_key).await;
            return Err(error.into());
        }
    };
    let (can_undo, can_redo) = db::artifact_history_state(&state.pool, &id).await?;
    Ok(Json(TransactionCommit {
        result: CommitResult {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            artifact_id: id,
            transaction_id,
            revision,
            invalidation: change_set.invalidation,
            mutations: mutation_records(&change_set.mutations)?,
            events,
        },
        document: committed,
        can_undo,
        can_redo,
    }))
}

async fn persist_candidate_snapshot(
    state: &AppState,
    artifact_id: &str,
    revision: u64,
    deck: oo_schema::presentation_v5::Deck,
) -> Result<String, AppError> {
    let artifact = artifact_for(artifact_id, revision, ArtifactPayload::Presentation(deck))?;
    let bytes = serde_json::to_vec(&artifact).map_err(|error| {
        AppError::Internal(format!("序列化 Presentation Artifact 失败：{error}"))
    })?;
    let version = i64::try_from(revision)
        .map_err(|_| AppError::Internal("Presentation revision 超出服务端范围".into()))?;
    let snapshot_key = crate::document_support::artifact_snapshot_key(artifact_id, version);
    state.store.put(&snapshot_key, &bytes).await?;
    Ok(snapshot_key)
}

async fn discard_candidate_snapshot(state: &AppState, snapshot_key: &str) {
    if let Err(error) = state.store.delete(snapshot_key).await {
        tracing::warn!(snapshot_key, error = %error, "无法清理未提交的 Presentation snapshot");
    }
}

/// Validate the Presentation-scoped history intent before any command reaches
/// the normal v5 command decoder. History remains an intent-only command: its
/// inverses are recovered from the server-owned semantic transaction record.
fn presentation_history_operation(
    transaction: &ArtifactCommandEnvelope,
) -> Result<Option<PresentationHistoryOperation>, AppError> {
    let has_history = transaction
        .commands
        .iter()
        .any(|record| record.type_id == PRESENTATION_HISTORY_TYPE_ID);
    if !has_history {
        if matches!(
            transaction.origin,
            TransactionOrigin::Undo | TransactionOrigin::Redo
        ) {
            return Err(AppError::BadRequest(
                "Undo/Redo transaction 必须使用 presentation.history command".into(),
            ));
        }
        return Ok(None);
    }
    if transaction.commands.len() != 1 {
        return Err(AppError::BadRequest(
            "presentation.history transaction 只能包含一个 command".into(),
        ));
    }
    let record = &transaction.commands[0];
    if record.type_id != PRESENTATION_HISTORY_TYPE_ID {
        return Err(AppError::BadRequest(
            "presentation.history transaction 不能混入普通 command".into(),
        ));
    }
    let operation = PresentationHistoryOperation::from_record(record).map_err(|error| {
        AppError::BadRequest(format!("Presentation history operation 无效：{error}"))
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

/// Reconstructs the P2 typed journal from the recorded semantic command batch
/// and applies its undo/redo transition. This avoids client supplied inverse
/// mutations while still retaining durable, restart-safe history selection in
/// the Artifact database.
async fn submit_history_transaction(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
    transaction: &ArtifactCommandEnvelope,
    expected_version: i64,
    transaction_id: &str,
    history: PresentationHistoryOperation,
) -> Result<Json<TransactionCommit>, AppError> {
    let expected_undone = matches!(history.action, HistoryAction::Redo);
    let entry = db::get_artifact_history(&state.pool, id, expected_undone)
        .await?
        .ok_or_else(|| {
            AppError::BadRequest(match history.action {
                HistoryAction::Undo => "没有可撤销的 Presentation 事务".into(),
                HistoryAction::Redo => "没有可重做的 Presentation 事务".into(),
            })
        })?;
    let source = db::get_artifact_transaction(&state.pool, id, &entry.transaction_id)
        .await?
        .ok_or_else(|| AppError::Internal("Presentation history 缺少原始事务记录".into()))?;
    let source_base_revision = u64::try_from(source.base_version)
        .map_err(|_| AppError::Internal("Presentation history base revision 超出范围".into()))?;
    let command_records: Vec<CommandRecord> =
        serde_json::from_str(&source.commands_json).map_err(|error| {
            AppError::Internal(format!("Presentation history commands JSON 无效：{error}"))
        })?;
    let commands = command_records
        .into_iter()
        .map(presentation_command_from_record)
        .collect::<Result<Vec<_>, _>>()?;
    if commands.is_empty() {
        return Err(AppError::BadRequest(
            "该 Presentation 历史条目不包含可重放的语义 command".into(),
        ));
    }
    let before =
        load_presentation_from_snapshot_key(state, user, id, &entry.before_snapshot_key).await?;
    let mut engine =
        PresentationEngine::new(before, source_base_revision).map_err(presentation_engine_error)?;
    engine
        .execute(PresentationCommandBatch {
            base_revision: source_base_revision,
            commands,
        })
        .map_err(presentation_engine_error)?;
    let change_set = match history.action {
        HistoryAction::Undo => engine
            .undo(source_base_revision + 1)
            .map_err(presentation_engine_error)?,
        HistoryAction::Redo => {
            engine
                .undo(source_base_revision + 1)
                .map_err(presentation_engine_error)?;
            engine
                .redo(source_base_revision + 2)
                .map_err(presentation_engine_error)?
        }
    };
    let revision = u64::try_from(
        expected_version
            .checked_add(1)
            .ok_or_else(|| AppError::Internal("Presentation revision 溢出".into()))?,
    )
    .map_err(|_| AppError::Internal("Presentation revision 超出协议范围".into()))?;
    let snapshot_key =
        persist_candidate_snapshot(state, id, revision, engine.deck().clone()).await?;
    let referenced_asset_ids = presentation_asset_ids(engine.deck());
    let changed_entities = presentation_entity_keys(&change_set.invalidation);
    let mut events = presentation_events(
        transaction_id,
        revision,
        &change_set.mutations,
        &change_set.dirty_thumbnail_ids,
        &user.id,
    )?;
    events.push(DomainEventRecord {
        event_id: format!("{transaction_id}:{revision}:history"),
        type_id: "presentation.historyApplied".into(),
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
    let committed = match db::commit_artifact_history_with_assets(
        &state.pool,
        db::ArtifactHistoryCommit {
            id,
            expected_version,
            snapshot_key: &snapshot_key,
            transaction_id,
            author_id: &user.id,
            changed_entities: &changed_entities,
            structure_changed: change_set.invalidation.structure_changed,
            origin: transaction_origin_name(transaction.origin),
            commands_json: &commands_json,
            history_id: entry.history_id,
            expected_undone,
            next_undone: !expected_undone,
            events: &events,
        },
        &referenced_asset_ids,
    )
    .await
    {
        Ok(Some(meta)) => meta,
        Ok(None) => {
            discard_candidate_snapshot(state, &snapshot_key).await;
            return Err(AppError::VersionConflict);
        }
        Err(error) => {
            discard_candidate_snapshot(state, &snapshot_key).await;
            return Err(error.into());
        }
    };
    let (can_undo, can_redo) = db::artifact_history_state(&state.pool, id).await?;
    Ok(Json(TransactionCommit {
        result: CommitResult {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            artifact_id: id.into(),
            transaction_id: transaction_id.into(),
            revision,
            invalidation: change_set.invalidation,
            mutations: mutation_records(&change_set.mutations)?,
            events,
        },
        document: committed,
        can_undo,
        can_redo,
    }))
}

async fn load_presentation_from_snapshot_key(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
    snapshot_key: &str,
) -> Result<oo_schema::presentation_v5::Deck, AppError> {
    // Re-check ownership before reading an immutable object key supplied by
    // the server history row.
    let (meta, _) = load_artifact(state, user, id).await?;
    if meta.kind != ArtifactKind::Presentation {
        return Err(AppError::UnsupportedArtifact(meta.kind));
    }
    let bytes = state.store.get(snapshot_key).await?;
    let artifact: ArtifactEnvelope = serde_json::from_slice(&bytes).map_err(|error| {
        AppError::Internal(format!("历史 Presentation Artifact JSON 无效：{error}"))
    })?;
    artifact
        .validate()
        .map_err(|error| AppError::Internal(format!("历史 Presentation Artifact 无效：{error}")))?;
    if artifact.artifact_id != id {
        return Err(AppError::Internal(
            "历史 Presentation Artifact id 不一致".into(),
        ));
    }
    match artifact.payload {
        ArtifactPayload::Presentation(deck) => Ok(deck),
        _ => Err(AppError::Internal(
            "历史 snapshot 不是 Presentation Artifact".into(),
        )),
    }
}

fn presentation_asset_ids(deck: &oo_schema::presentation_v5::Deck) -> Vec<String> {
    let mut ids = deck
        .assets
        .iter()
        .map(|asset| asset.asset_id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids.dedup();
    ids
}

async fn validate_registered_assets(
    state: &AppState,
    artifact_id: &str,
    commands: &[PresentationCommand],
) -> Result<(), AppError> {
    for command in commands {
        let PresentationCommand::RegisterAsset { asset } = command else {
            continue;
        };
        let stored = db::get_artifact_asset(&state.pool, artifact_id, &asset.asset_id)
            .await?
            .ok_or_else(|| {
                AppError::BadRequest(format!("Presentation asset {} 尚未上传", asset.asset_id))
            })?;
        if stored.checksum != asset.digest || stored.content_type != asset.mime_type {
            return Err(AppError::BadRequest(format!(
                "Presentation asset {} 的 digest 或 MIME 与已验证二进制不一致",
                asset.asset_id
            )));
        }
    }
    Ok(())
}

fn presentation_command_from_record(
    record: CommandRecord,
) -> Result<PresentationCommand, AppError> {
    let command: PresentationCommand = serde_json::from_value(record.payload)
        .map_err(|error| AppError::BadRequest(format!("Presentation command 无效：{error}")))?;
    let expected = match &command {
        PresentationCommand::RegisterAsset { .. } => "presentation.registerAsset",
        PresentationCommand::SetPageSpec { .. } => "presentation.setPageSpec",
        PresentationCommand::CreateMaster { .. } => "presentation.createMaster",
        PresentationCommand::UpdateMaster { .. } => "presentation.updateMaster",
        PresentationCommand::DeleteMaster { .. } => "presentation.deleteMaster",
        PresentationCommand::CreateLayout { .. } => "presentation.createLayout",
        PresentationCommand::UpdateLayout { .. } => "presentation.updateLayout",
        PresentationCommand::DeleteLayout { .. } => "presentation.deleteLayout",
        PresentationCommand::CreateSlide { .. } => "presentation.createSlide",
        PresentationCommand::DeleteSlide { .. } => "presentation.deleteSlide",
        PresentationCommand::MoveSlide { .. } => "presentation.moveSlide",
        PresentationCommand::DuplicateSlide { .. } => "presentation.duplicateSlide",
        PresentationCommand::InsertNode { .. } => "presentation.insertNode",
        PresentationCommand::DeleteNode { .. } => "presentation.deleteNode",
        PresentationCommand::MoveNode { .. } => "presentation.moveNode",
        PresentationCommand::ReorderNode { .. } => "presentation.reorderNode",
        PresentationCommand::GroupNodes { .. } => "presentation.groupNodes",
        PresentationCommand::UngroupNodes { .. } => "presentation.ungroupNodes",
        PresentationCommand::SetNodeTransform { .. } => "presentation.setNodeTransform",
        PresentationCommand::SetNodeLocked { .. } => "presentation.setNodeLocked",
        PresentationCommand::AlignNodes { .. } => "presentation.alignNodes",
        PresentationCommand::DistributeNodes { .. } => "presentation.distributeNodes",
        PresentationCommand::SetShapeStyle { .. } => "presentation.setShapeStyle",
        PresentationCommand::SetShapeGeometry { .. } => "presentation.setShapeGeometry",
        PresentationCommand::SetChartSpec { .. } => "presentation.setChartSpec",
        PresentationCommand::SetConnectorEndpoints { .. } => "presentation.setConnectorEndpoints",
        PresentationCommand::SetTableCellContent { .. } => "presentation.setTableCellContent",
        PresentationCommand::SetTableCellStyle { .. } => "presentation.setTableCellStyle",
        PresentationCommand::InsertTableRows { .. } => "presentation.insertTableRows",
        PresentationCommand::InsertTableColumns { .. } => "presentation.insertTableColumns",
        PresentationCommand::DeleteTableRow { .. } => "presentation.deleteTableRow",
        PresentationCommand::DeleteTableColumn { .. } => "presentation.deleteTableColumn",
        PresentationCommand::MergeTableCells { .. } => "presentation.mergeTableCells",
        PresentationCommand::SplitTableCell { .. } => "presentation.splitTableCell",
        PresentationCommand::SetTextContent { .. } => "presentation.setTextContent",
        PresentationCommand::SetTextFrame { .. } => "presentation.setTextFrame",
        PresentationCommand::SetImageConfig { .. } => "presentation.setImageConfig",
        PresentationCommand::SetMediaConfig { .. } => "presentation.setMediaConfig",
        PresentationCommand::SetSlideNotes { .. } => "presentation.setSlideNotes",
        PresentationCommand::SetSlideBackground { .. } => "presentation.setSlideBackground",
        PresentationCommand::SetSlideLayout { .. } => "presentation.setSlideLayout",
        PresentationCommand::SetTheme { .. } => "presentation.setTheme",
        PresentationCommand::SetSlideTransition { .. } => "presentation.setSlideTransition",
        PresentationCommand::UpsertAnimation { .. } => "presentation.upsertAnimation",
        PresentationCommand::DeleteAnimation { .. } => "presentation.deleteAnimation",
        PresentationCommand::MoveAnimation { .. } => "presentation.moveAnimation",
    };
    if record.type_id != expected {
        return Err(AppError::BadRequest(format!(
            "Presentation 不支持 command typeId：{}",
            record.type_id
        )));
    }
    Ok(command)
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

fn mutation_records(mutations: &[PresentationMutation]) -> Result<Vec<MutationRecord>, AppError> {
    mutations
        .iter()
        .map(|mutation| {
            mutation.to_record().map_err(|error| {
                AppError::Internal(format!("Presentation mutation 序列化失败：{error}"))
            })
        })
        .collect()
}

fn presentation_events(
    transaction_id: &str,
    revision: u64,
    mutations: &[PresentationMutation],
    dirty_thumbnail_ids: &[String],
    actor_id: &str,
) -> Result<Vec<DomainEventRecord>, AppError> {
    let mut events = mutations
        .iter()
        .enumerate()
        .map(|(index, mutation)| {
            let record = mutation.to_record().map_err(|error| {
                AppError::Internal(format!("Presentation event 序列化失败：{error}"))
            })?;
            let MutationRecord { type_id, payload } = record;
            Ok(DomainEventRecord {
                event_id: format!("{transaction_id}:{revision}:{index}"),
                type_id,
                payload,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    // Thumbnail workers consume this explicit, read-only consequence instead
    // of reverse-engineering write commands. The IDs originate in
    // DeckProjection::invalidate and never grant a rendering cache write-back.
    events.extend(
        dirty_thumbnail_ids
            .iter()
            .enumerate()
            .map(|(index, slide_id)| DomainEventRecord {
                event_id: format!("{transaction_id}:{revision}:thumbnail:{index}"),
                type_id: "presentation.thumbnailInvalidated".into(),
                payload: serde_json::json!({ "slideId": slide_id }),
            }),
    );
    crate::events::stamp_actor(&mut events, actor_id);
    Ok(events)
}

// SQLite's generic transaction summary predates polymorphic entity refs and
// stores strings. Keep that legacy storage detail inside this adapter; public
// commit invalidations remain typed `EntityRef` values.
fn presentation_entity_keys(invalidation: &Invalidation) -> Vec<String> {
    invalidation
        .changed_entities
        .iter()
        .map(|entity| format!("{}\u{1f}{}", entity.entity_type, entity.entity_id))
        .collect()
}

fn presentation_invalidation_from_keys(keys: &[String], structure_changed: bool) -> Invalidation {
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
            entity_type: "presentation.deck".into(),
            entity_id: "deck".into(),
        }],
        structure_changed,
    }
}

fn presentation_engine_error(error: PresentationEngineError) -> AppError {
    match error {
        PresentationEngineError::RevisionConflict { .. } => AppError::VersionConflict,
        other => AppError::BadRequest(format!("Presentation 事务无法应用：{other}")),
    }
}
