//! Spreadsheet transaction adapter.
//!
//! Same narrow-adapter contract as [`crate::mindmap_support`]: typed semantic
//! commands only, immutable snapshots, shared revision/idempotency/outbox
//! guarantees, actor-attributed events. Spreadsheet mutations do not carry
//! their own wire type ids, so this adapter defines the stable event/mutation
//! projection for the grid domain.

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::Json;
use oo_protocol::{
    ArtifactCommandEnvelope, CommandRecord, CommitResult, DomainEventRecord, EntityRef,
    HistoryAction, Invalidation, MutationRecord, SpreadsheetHistoryOperation, TransactionOrigin,
    CURRENT_PROTOCOL_VERSION, SPREADSHEET_HISTORY_TYPE_ID,
};
use oo_schema::{ArtifactEnvelope, ArtifactPayload, AssetReferenceSource};
use oo_spreadsheet::{
    SpreadsheetCommand, SpreadsheetCommandBatch, SpreadsheetEngine, SpreadsheetEngineError,
    SpreadsheetMutation,
};

use crate::artifact_routes::{artifact_for, load_artifact};
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
    let history = spreadsheet_history_operation(&prepared.envelope)?;

    let _write_guard = state.write_lock.lock().await;
    let (meta, snapshot) =
        transaction_kernel::load_target(&state, &user, &id, ArtifactKind::Spreadsheet).await?;
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

    let ArtifactPayload::Spreadsheet(model) = snapshot.payload else {
        return Err(AppError::Internal(
            "Spreadsheet 记录包含非 Spreadsheet Artifact".into(),
        ));
    };
    let commands_json = serde_json::to_string(&transaction.commands)
        .map_err(|error| AppError::Internal(format!("事务记录序列化失败：{error}")))?;
    let commands = transaction
        .commands
        .iter()
        .cloned()
        .map(spreadsheet_command_from_record)
        .collect::<Result<Vec<_>, _>>()?;
    let mut engine = SpreadsheetEngine::new(model, transaction.base_revision)
        .map_err(spreadsheet_engine_error)?;
    let change_set = engine
        .execute(SpreadsheetCommandBatch {
            base_revision: transaction.base_revision,
            commands,
        })
        .map_err(spreadsheet_engine_error)?;
    let revision = change_set.revision;

    let artifact = artifact_for(
        &id,
        revision,
        ArtifactPayload::Spreadsheet(engine.model().clone()),
    )?;
    let bytes = serde_json::to_vec(&artifact).map_err(|error| {
        AppError::Internal(format!("序列化 Spreadsheet Artifact 失败：{error}"))
    })?;
    let version = i64::try_from(revision)
        .map_err(|_| AppError::Internal("Spreadsheet revision 超出服务端范围".into()))?;
    let snapshot_key = crate::document_support::artifact_snapshot_key(&id, version);
    state.store.put(&snapshot_key, &bytes).await?;
    let referenced_assets = engine.model().asset_references();

    let blobs = db::get_artifact_blob_keys(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Artifact {id} 不存在")))?;
    let changed_entities = entity_keys(&change_set.invalidation);
    let mutation_records = change_set
        .mutations
        .iter()
        .map(mutation_record)
        .collect::<Result<Vec<_>, AppError>>()?;
    let mut events = change_set
        .mutations
        .iter()
        .enumerate()
        .map(|(index, mutation)| {
            Ok(DomainEventRecord {
                event_id: format!("{transaction_id}:{revision}:{index}"),
                type_id: event_type_id(mutation).into(),
                payload: event_payload(mutation),
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
    // Server-authoritative history: the durable semantic command record makes
    // the committed transaction replayable, so the affordances reflect the
    // real replayable history state.
    let (can_undo, can_redo) = db::artifact_history_state(&state.pool, &id).await?;
    Ok(Json(TransactionCommit {
        result: CommitResult {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            artifact_id: id,
            transaction_id,
            revision,
            invalidation: change_set.invalidation,
            mutations: mutation_records,
            events,
        },
        document: committed,
        can_undo,
        can_redo,
    }))
}

/// Validates the Spreadsheet-scoped history intent before any command reaches
/// the normal grid command decoder. History remains an intent-only command:
/// its inverse mutations are recovered from the server-owned semantic
/// transaction record, never from a client payload.
fn spreadsheet_history_operation(
    transaction: &ArtifactCommandEnvelope,
) -> Result<Option<SpreadsheetHistoryOperation>, AppError> {
    let has_history = transaction
        .commands
        .iter()
        .any(|record| record.type_id == SPREADSHEET_HISTORY_TYPE_ID);
    if !has_history {
        if matches!(
            transaction.origin,
            TransactionOrigin::Undo | TransactionOrigin::Redo
        ) {
            return Err(AppError::BadRequest(
                "Undo/Redo transaction 必须使用 spreadsheet.history command".into(),
            ));
        }
        return Ok(None);
    }
    if transaction.commands.len() != 1 {
        return Err(AppError::BadRequest(
            "spreadsheet.history transaction 只能包含一个 command".into(),
        ));
    }
    let record = &transaction.commands[0];
    if record.type_id != SPREADSHEET_HISTORY_TYPE_ID {
        return Err(AppError::BadRequest(
            "spreadsheet.history transaction 不能混入普通 command".into(),
        ));
    }
    let operation = SpreadsheetHistoryOperation::from_record(record).map_err(|error| {
        AppError::BadRequest(format!("Spreadsheet history operation 无效：{error}"))
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

/// Reconstructs the grid engine from the pre-transaction snapshot and the
/// durable semantic command record, then applies the undo/redo transition.
/// The resulting immutable snapshot is persisted as a normal history
/// transition, so restarts never lose the undo position.
async fn submit_history_transaction(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
    transaction: &ArtifactCommandEnvelope,
    expected_version: i64,
    transaction_id: &str,
    history: SpreadsheetHistoryOperation,
) -> Result<Json<TransactionCommit>, AppError> {
    let expected_undone = matches!(history.action, HistoryAction::Redo);
    let entry = db::get_artifact_history(&state.pool, id, expected_undone)
        .await?
        .ok_or_else(|| {
            AppError::BadRequest(match history.action {
                HistoryAction::Undo => "没有可撤销的 Spreadsheet 事务".into(),
                HistoryAction::Redo => "没有可重做的 Spreadsheet 事务".into(),
            })
        })?;
    let source = db::get_artifact_transaction(&state.pool, id, &entry.transaction_id)
        .await?
        .ok_or_else(|| AppError::Internal("Spreadsheet history 缺少原始事务记录".into()))?;
    let source_base_revision = u64::try_from(source.base_version)
        .map_err(|_| AppError::Internal("Spreadsheet history base revision 超出范围".into()))?;
    let command_records: Vec<CommandRecord> =
        serde_json::from_str(&source.commands_json).map_err(|error| {
            AppError::Internal(format!("Spreadsheet history commands JSON 无效：{error}"))
        })?;
    let commands = command_records
        .into_iter()
        .map(spreadsheet_command_from_record)
        .collect::<Result<Vec<_>, _>>()?;
    if commands.is_empty() {
        return Err(AppError::BadRequest(
            "该 Spreadsheet 历史条目不包含可重放的语义 command".into(),
        ));
    }
    let before =
        load_spreadsheet_from_snapshot_key(state, user, id, &entry.before_snapshot_key).await?;
    let mut engine =
        SpreadsheetEngine::new(before, source_base_revision).map_err(spreadsheet_engine_error)?;
    engine
        .execute(SpreadsheetCommandBatch {
            base_revision: source_base_revision,
            commands,
        })
        .map_err(spreadsheet_engine_error)?;
    let change_set = match history.action {
        HistoryAction::Undo => engine.undo().map_err(spreadsheet_engine_error)?,
        HistoryAction::Redo => {
            engine.undo().map_err(spreadsheet_engine_error)?;
            engine.redo().map_err(spreadsheet_engine_error)?
        }
    };
    let revision = u64::try_from(
        expected_version
            .checked_add(1)
            .ok_or_else(|| AppError::Internal("Spreadsheet revision 溢出".into()))?,
    )
    .map_err(|_| AppError::Internal("Spreadsheet revision 超出协议范围".into()))?;
    let artifact = artifact_for(
        id,
        revision,
        ArtifactPayload::Spreadsheet(engine.model().clone()),
    )?;
    let bytes = serde_json::to_vec(&artifact).map_err(|error| {
        AppError::Internal(format!("序列化 Spreadsheet Artifact 失败：{error}"))
    })?;
    let version = i64::try_from(revision)
        .map_err(|_| AppError::Internal("Spreadsheet revision 超出服务端范围".into()))?;
    let snapshot_key = crate::document_support::artifact_snapshot_key(id, version);
    state.store.put(&snapshot_key, &bytes).await?;
    let referenced_assets = engine.model().asset_references();

    let changed_entities = entity_keys(&change_set.invalidation);
    let mutation_records = change_set
        .mutations
        .iter()
        .map(mutation_record)
        .collect::<Result<Vec<_>, AppError>>()?;
    let mut events = change_set
        .mutations
        .iter()
        .enumerate()
        .map(|(index, mutation)| {
            Ok(DomainEventRecord {
                event_id: format!("{transaction_id}:{revision}:{index}"),
                type_id: event_type_id(mutation).into(),
                payload: event_payload(mutation),
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    events.push(DomainEventRecord {
        event_id: format!("{transaction_id}:{revision}:history"),
        type_id: "spreadsheet.historyApplied".into(),
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
    transaction_kernel::stamp_events(&mut events, &user.id);
    let commands_json = serde_json::to_string(&transaction.commands)
        .map_err(|error| AppError::Internal(format!("事务记录序列化失败：{error}")))?;
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
            mutations: mutation_records,
            events,
        },
        document: committed,
        can_undo,
        can_redo,
    }))
}

async fn load_spreadsheet_from_snapshot_key(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
    snapshot_key: &str,
) -> Result<oo_schema::SpreadsheetModel, AppError> {
    // Re-check ownership before reading an immutable object key supplied by
    // the server history row.
    let (meta, _) = load_artifact(state, user, id).await?;
    if meta.kind != ArtifactKind::Spreadsheet {
        return Err(AppError::UnsupportedArtifact(meta.kind));
    }
    let bytes = state.store.get(snapshot_key).await?;
    let artifact: ArtifactEnvelope = serde_json::from_slice(&bytes).map_err(|error| {
        AppError::Internal(format!("历史 Spreadsheet Artifact JSON 无效：{error}"))
    })?;
    artifact
        .validate()
        .map_err(|error| AppError::Internal(format!("历史 Spreadsheet Artifact 无效：{error}")))?;
    if artifact.artifact_id != id {
        return Err(AppError::Internal(
            "历史 Spreadsheet Artifact id 不一致".into(),
        ));
    }
    match artifact.payload {
        ArtifactPayload::Spreadsheet(model) => Ok(model),
        _ => Err(AppError::Internal(
            "历史 snapshot 不是 Spreadsheet Artifact".into(),
        )),
    }
}

fn spreadsheet_command_from_record(record: CommandRecord) -> Result<SpreadsheetCommand, AppError> {
    if record.type_id == SPREADSHEET_HISTORY_TYPE_ID {
        // The typeId exists so clients can discover the history affordance;
        // it is an intent that `submit_history_transaction` resolves from the
        // durable transaction log, never a grid command payload.
        return Err(AppError::BadRequest(
            "spreadsheet.history 只能作为事务中的唯一 intent command 提交".into(),
        ));
    }
    let command: SpreadsheetCommand = serde_json::from_value(record.payload)
        .map_err(|error| AppError::BadRequest(format!("Spreadsheet command 无效：{error}")))?;
    let expected_type_id = match &command {
        SpreadsheetCommand::CreateSheet { .. } => "spreadsheet.createSheet",
        SpreadsheetCommand::RenameSheet { .. } => "spreadsheet.renameSheet",
        SpreadsheetCommand::DeleteSheet { .. } => "spreadsheet.deleteSheet",
        SpreadsheetCommand::SetCell { .. } => "spreadsheet.setCell",
        SpreadsheetCommand::SetCellStyle { .. } => "spreadsheet.setCellStyle",
        SpreadsheetCommand::SetRowLayout { .. } => "spreadsheet.setRowLayout",
        SpreadsheetCommand::SetSheetMetadata { .. } => "spreadsheet.setSheetMetadata",
        SpreadsheetCommand::ClearCell { .. } => "spreadsheet.clearCell",
        SpreadsheetCommand::InsertRows { .. } => "spreadsheet.insertRows",
        SpreadsheetCommand::DeleteRows { .. } => "spreadsheet.deleteRows",
        SpreadsheetCommand::InsertColumns { .. } => "spreadsheet.insertColumns",
        SpreadsheetCommand::DeleteColumns { .. } => "spreadsheet.deleteColumns",
        SpreadsheetCommand::MergeCells { .. } => "spreadsheet.mergeCells",
        SpreadsheetCommand::UnmergeCells { .. } => "spreadsheet.unmergeCells",
        SpreadsheetCommand::SortRange { .. } => "spreadsheet.sortRange",
        SpreadsheetCommand::FormatRange { .. } => "spreadsheet.formatRange",
        SpreadsheetCommand::ClearRange { .. } => "spreadsheet.clearRange",
        SpreadsheetCommand::ReplaceRange { .. } => "spreadsheet.replaceRange",
        SpreadsheetCommand::PasteRange { .. } => "spreadsheet.pasteRange",
        SpreadsheetCommand::FillRange { .. } => "spreadsheet.fillRange",
        SpreadsheetCommand::SetFreezePane { .. } => "spreadsheet.setFreezePane",
        SpreadsheetCommand::SetAutoFilter { .. } => "spreadsheet.setAutoFilter",
        SpreadsheetCommand::UpsertFilterColumn { .. } => "spreadsheet.upsertFilterColumn",
        SpreadsheetCommand::ClearFilterColumn { .. } => "spreadsheet.clearFilter",
        SpreadsheetCommand::SetCalculationMode { .. } => "spreadsheet.setCalculationMode",
        SpreadsheetCommand::SetRowDimensions { .. } => "spreadsheet.setRowDimensions",
        SpreadsheetCommand::SetColumnDimensions { .. } => "spreadsheet.setColumnDimensions",
        SpreadsheetCommand::UpsertConditionalFormat { .. } => "spreadsheet.upsertConditionalFormat",
        SpreadsheetCommand::DeleteConditionalFormat { .. } => "spreadsheet.deleteConditionalFormat",
        SpreadsheetCommand::UpsertDataValidation { .. } => "spreadsheet.upsertDataValidation",
        SpreadsheetCommand::DeleteDataValidation { .. } => "spreadsheet.deleteDataValidation",
    };
    if record.type_id != expected_type_id {
        return Err(AppError::BadRequest(format!(
            "命令 typeId {} 与 payload 不符，应为 {expected_type_id}",
            record.type_id
        )));
    }
    Ok(command)
}

/// Stable wire identity for a grid mutation. The engine keeps mutations as
/// inverse-capable structs; the public protocol surface names them here.
fn event_type_id(mutation: &SpreadsheetMutation) -> &'static str {
    match mutation {
        SpreadsheetMutation::SheetChanged { .. } => "spreadsheet.sheetChanged",
        SpreadsheetMutation::CellChanged { .. } => "spreadsheet.cellChanged",
        SpreadsheetMutation::RangeChanged { .. } => "spreadsheet.rangeChanged",
        SpreadsheetMutation::PaneChanged { .. } => "spreadsheet.paneChanged",
        SpreadsheetMutation::FilterChanged { .. } => "spreadsheet.filterChanged",
        SpreadsheetMutation::FilterColumnsChanged { .. } => "spreadsheet.filterColumnsChanged",
        SpreadsheetMutation::CalculationModeChanged { .. } => "spreadsheet.calculationModeChanged",
        SpreadsheetMutation::NamedRangesChanged { .. } => "spreadsheet.namedRangesChanged",
        SpreadsheetMutation::ActiveSheetChanged { .. } => "spreadsheet.activeSheetChanged",
        SpreadsheetMutation::RowDimensionsChanged { .. } => "spreadsheet.rowDimensionsChanged",
        SpreadsheetMutation::ColumnDimensionsChanged { .. } => {
            "spreadsheet.columnDimensionsChanged"
        }
        SpreadsheetMutation::ConditionalFormatUpserted { .. } => {
            "spreadsheet.conditionalFormatUpserted"
        }
        SpreadsheetMutation::ConditionalFormatRemoved { .. } => {
            "spreadsheet.conditionalFormatRemoved"
        }
        SpreadsheetMutation::DataValidationUpserted { .. } => "spreadsheet.dataValidationUpserted",
        SpreadsheetMutation::DataValidationRemoved { .. } => "spreadsheet.dataValidationRemoved",
    }
}

fn event_payload(mutation: &SpreadsheetMutation) -> serde_json::Value {
    match mutation {
        SpreadsheetMutation::SheetChanged { sheet_id, .. } => serde_json::json!({
            "sheetId": sheet_id,
        }),
        SpreadsheetMutation::CellChanged { address, after, .. } => serde_json::json!({
            "sheetId": address.sheet_id,
            "row": address.row,
            "column": address.column,
            "after": after,
        }),
        SpreadsheetMutation::RangeChanged {
            sheet_id, range, ..
        } => serde_json::json!({
            "sheetId": sheet_id,
            "range": range,
        }),
        SpreadsheetMutation::PaneChanged {
            sheet_id, after, ..
        } => serde_json::json!({
            "sheetId": sheet_id,
            "after": after,
        }),
        SpreadsheetMutation::FilterChanged {
            sheet_id, after, ..
        } => serde_json::json!({
            "sheetId": sheet_id,
            "after": after,
        }),
        SpreadsheetMutation::FilterColumnsChanged {
            sheet_id, after, ..
        } => serde_json::json!({
            "sheetId": sheet_id,
            "after": after,
        }),
        SpreadsheetMutation::CalculationModeChanged { after, .. } => serde_json::json!({
            "after": after,
        }),
        SpreadsheetMutation::NamedRangesChanged { after, .. } => serde_json::json!({
            "after": after,
        }),
        SpreadsheetMutation::ActiveSheetChanged { after, .. } => serde_json::json!({
            "after": after,
        }),
        SpreadsheetMutation::RowDimensionsChanged {
            sheet_id, after, ..
        } => serde_json::json!({
            "sheetId": sheet_id,
            "after": after,
        }),
        SpreadsheetMutation::ColumnDimensionsChanged {
            sheet_id, after, ..
        } => serde_json::json!({
            "sheetId": sheet_id,
            "after": after,
        }),
        SpreadsheetMutation::ConditionalFormatUpserted { sheet_id, rule, .. } => {
            serde_json::json!({
                "sheetId": sheet_id,
                "ruleId": rule.id,
            })
        }
        SpreadsheetMutation::ConditionalFormatRemoved {
            sheet_id, rule_id, ..
        } => serde_json::json!({
            "sheetId": sheet_id,
            "ruleId": rule_id,
        }),
        SpreadsheetMutation::DataValidationUpserted { sheet_id, rule, .. } => serde_json::json!({
            "sheetId": sheet_id,
            "ruleId": rule.id,
        }),
        SpreadsheetMutation::DataValidationRemoved {
            sheet_id, rule_id, ..
        } => serde_json::json!({
            "sheetId": sheet_id,
            "ruleId": rule_id,
        }),
    }
}

fn mutation_record(mutation: &SpreadsheetMutation) -> Result<MutationRecord, AppError> {
    Ok(MutationRecord {
        type_id: event_type_id(mutation).into(),
        payload: serde_json::to_value(mutation)
            .map_err(|error| AppError::Internal(format!("Mutation 序列化失败：{error}")))?,
    })
}

fn spreadsheet_engine_error(error: SpreadsheetEngineError) -> AppError {
    match error {
        SpreadsheetEngineError::RevisionConflict { .. } => AppError::VersionConflict,
        other => AppError::BadRequest(format!("Spreadsheet 事务无法应用：{other}")),
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
            entity_type: "spreadsheet.workbook".into(),
            entity_id: "workbook".into(),
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
