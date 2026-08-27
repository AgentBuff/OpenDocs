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
    Invalidation, MutationRecord, TransactionOrigin, CURRENT_PROTOCOL_VERSION,
};
use oo_schema::ArtifactPayload;
use oo_spreadsheet::{
    SpreadsheetCommand, SpreadsheetCommandBatch, SpreadsheetEngine, SpreadsheetEngineError,
    SpreadsheetMutation,
};

use crate::artifact_routes::{artifact_for, authorize, load_artifact, Role};
use crate::auth::CurrentUser;
use crate::db::{self, ArtifactKind};
use crate::document_support::TransactionCommit;
use crate::error::{AppError, ConflictDetails};
use crate::request_context;
use crate::AppState;

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

    let _write_guard = state.write_lock.lock().await;
    // 事务是写路径：editor 及以上（C4 授权分层）。
    authorize(&state, &user, &id, Role::Editor).await?;
    let (meta, snapshot) = load_artifact(&state, &user, &id).await?;
    if meta.kind != ArtifactKind::Spreadsheet {
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
                invalidation: invalidation_from_keys(
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
    crate::events::stamp_actor(&mut events, &user.id);

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
        &[],
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
    // Spreadsheet has no server-authoritative history yet; affordances stay
    // false instead of pretending an undo path exists.
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
        can_undo: false,
        can_redo: false,
    }))
}

async fn discard_candidate_snapshot(state: &AppState, snapshot_key: &str) {
    if let Err(error) = state.store.delete(snapshot_key).await {
        tracing::warn!(snapshot_key, error = %error, "无法清理未提交的 Spreadsheet snapshot");
    }
}

fn spreadsheet_command_from_record(record: CommandRecord) -> Result<SpreadsheetCommand, AppError> {
    let command: SpreadsheetCommand = serde_json::from_value(record.payload)
        .map_err(|error| AppError::BadRequest(format!("Spreadsheet command 无效：{error}")))?;
    let expected_type_id = match &command {
        SpreadsheetCommand::CreateSheet { .. } => "spreadsheet.createSheet",
        SpreadsheetCommand::RenameSheet { .. } => "spreadsheet.renameSheet",
        SpreadsheetCommand::DeleteSheet { .. } => "spreadsheet.deleteSheet",
        SpreadsheetCommand::SetCell { .. } => "spreadsheet.setCell",
        SpreadsheetCommand::SetCellStyle { .. } => "spreadsheet.setCellStyle",
        SpreadsheetCommand::SetSheetMetadata { .. } => "spreadsheet.setSheetMetadata",
        SpreadsheetCommand::ClearCell { .. } => "spreadsheet.clearCell",
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
