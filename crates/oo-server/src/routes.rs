//! Document Artifact 的内部持久化与事务支撑。
//!
//! 公共 HTTP 资源只在 `artifact_routes` 注册为 `/api/artifacts/**`。本模块承载
//! Document 专属的 snapshot、history 和 transaction 支撑实现；它不是公共资源树，
//! 也不得重新注册旧的 `/api/docs/**` 路由。
//!
//! **不要在本模块新增处理器。** 唯一真正注册的路由是 [`health`]；其余 `pub` 项
//! （[`list_snapshots`]、[`get_snapshot`]、[`restore_snapshot`]、[`history_state`]、
//! [`submit_transaction`]、[`artifact_snapshot_key`]、[`download_content_disposition`]）
//! 都是被 `artifact_routes` 复用的支撑函数，不是路由。
//!
//! cutover 之前这里曾并存一整套 `upload` / `create` / `patch` / `list` / `get_meta` /
//! `get_artifact` / `get_original` / `export_docx` / `delete` 处理器：它们既未注册、
//! 也无任何调用点，却和活跃实现同名同形。这类「第二套写路径」本身不可达，但极容易
//! 在后续维护中被误当成活跃入口接回去，因此已整体删除，不要恢复。

use axum::extract::{Path, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use axum::Json;
use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::artifact_routes::{authorize, Role};
use crate::auth::CurrentUser;
use crate::db::{self, ArtifactKind, ArtifactMeta};
use crate::error::AppError;
use crate::request_context;
use crate::transaction_kernel;
use crate::AppState;
use oo_document::{
    DocumentCommand, DocumentCommandBatch, DocumentEngine, DocumentEngineError,
    Mutation as DocumentMutation,
};
use oo_protocol::{
    ArtifactCommandEnvelope, CommandRecord, CommitResult, DocumentHistoryOperation,
    DomainEventRecord, EntityRef, HistoryAction, Invalidation, MutationRecord, SnapshotEnvelope,
    CURRENT_PROTOCOL_VERSION, DOCUMENT_HISTORY_TYPE_ID,
};
use oo_schema::{ArtifactEnvelope, ArtifactPayload, AssetReferenceSource, DocumentModel};

pub async fn health() -> impl IntoResponse {
    Json(serde_json::json!({ "status": "ok" }))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PatchRequest {
    pub title: Option<String>,
    pub starred: Option<bool>,
}

/// Load a historical object-store snapshot for an authoritative undo/redo
/// transition.  The target artifact's old revision is expected; the next
/// artifact is written with the current revision by `prepare_artifact_save`.
async fn load_model_from_snapshot_key(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
    snapshot_key: &str,
) -> Result<DocumentModel, AppError> {
    load_owned(state, user, id).await?;
    let bytes = state.store.get(snapshot_key).await?;
    let artifact: ArtifactEnvelope = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::Internal(format!("历史 Artifact JSON 无效：{error}")))?;
    artifact
        .validate()
        .map_err(|error| AppError::Internal(format!("历史 Artifact 无效：{error}")))?;
    if artifact.artifact_id != id {
        return Err(AppError::Internal("历史 Artifact id 不一致".into()));
    }
    match artifact.payload {
        ArtifactPayload::Document(model) => Ok(model),
        _ => Err(AppError::Internal(
            "历史 snapshot 不是 Document Artifact".into(),
        )),
    }
}

/// 读取并校验指定版本的 Artifact。版本登记表是唯一的定位来源，不能由请求拼接对象键。
async fn load_snapshot(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
    version: i64,
) -> Result<SnapshotEnvelope, AppError> {
    load_owned(state, user, id).await?;
    let snapshot = db::get_artifact_snapshot(&state.pool, id, version)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("文档 {id} 的版本 {version} 不存在")))?;
    let bytes = state.store.get(&snapshot.snapshot_key).await?;
    let artifact: ArtifactEnvelope = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::Internal(format!("已保存的历史 Artifact JSON 无效：{error}")))?;
    artifact
        .validate()
        .map_err(|error| AppError::Internal(format!("已保存的历史 Artifact 无效：{error}")))?;
    if artifact.artifact_id != id
        || artifact.revision != u64::try_from(snapshot.version).unwrap_or(0)
    {
        return Err(AppError::Internal("历史 Artifact 与版本登记不一致".into()));
    }
    Ok(SnapshotEnvelope {
        protocol_version: CURRENT_PROTOCOL_VERSION,
        artifact,
    })
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotMeta {
    pub version: i64,
    pub created_at: DateTime<Utc>,
    pub current: bool,
}

/// 列出仍被保留的不可变 Artifact revisions。
///
/// 历史版本不会覆盖当前指针；列表只返回版本号和时间，不泄露 BlobStore 的对象键。
pub async fn list_snapshots(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<Vec<SnapshotMeta>>, AppError> {
    let current = load_owned(&state, &user, &id).await?;
    let snapshots = db::list_artifact_snapshots(&state.pool, &id)
        .await?
        .into_iter()
        .map(|snapshot| SnapshotMeta {
            current: snapshot.version == current.version,
            version: snapshot.version,
            created_at: snapshot.created_at,
        })
        .collect();
    Ok(Json(snapshots))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactHistoryState {
    pub can_undo: bool,
    pub can_redo: bool,
}

/// Authoritative toolbar history state after a
/// reload.  It is intentionally separate from transaction commit responses so
/// a fresh session does not need to guess from client-side history.
pub async fn history_state(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<ArtifactHistoryState>, AppError> {
    load_owned(&state, &user, &id).await?;
    let (can_undo, can_redo) = db::artifact_history_state(&state.pool, &id).await?;
    Ok(Json(ArtifactHistoryState { can_undo, can_redo }))
}

/// 读取指定历史版本。内容经过和当前 snapshot 相同的 envelope/schema 校验。
pub async fn get_snapshot(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, version)): Path<(String, i64)>,
) -> Result<Json<SnapshotEnvelope>, AppError> {
    let snapshot = load_snapshot(&state, &user, &id, version).await?;
    Ok(Json(snapshot))
}

/// 将指定历史版本恢复为一个新的当前版本，而不是把文档指针回拨到旧版本。
/// `If-Match` 保护恢复操作不会覆盖用户在查看历史期间产生的新编辑。
/// `x-transaction-id` 是 system/import 写入的幂等键，响应使用 typed CommitResult。
pub async fn restore_snapshot(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, version)): Path<(String, i64)>,
    headers: HeaderMap,
) -> Result<Json<TransactionCommit>, AppError> {
    let transaction_id = transaction_id_header(&headers)?;
    let expected_version = expected_version(&headers)?;
    let current = load_owned(&state, &user, &id).await?;
    // A lost response must be safely retryable even though the document has
    // already advanced.  Resolve the transaction id before the If-Match check;
    // a new transaction still remains protected by the revision guard below.
    if db::get_artifact_transaction(&state.pool, &id, &transaction_id)
        .await?
        .is_some()
    {
        return Ok(Json(
            empty_transaction_commit(&state, &id, &transaction_id, current).await?,
        ));
    }
    if current.version != expected_version {
        return Err(AppError::VersionConflict);
    }
    if version <= 0 {
        return Err(AppError::BadRequest("snapshot 版本号必须为正数".into()));
    }
    if version == current.version {
        return Ok(Json(
            empty_transaction_commit(&state, &id, &transaction_id, current).await?,
        ));
    }
    let snapshot = load_snapshot(&state, &user, &id, version).await?;
    let model = match snapshot.artifact.payload {
        ArtifactPayload::Document(model) => model,
        _ => {
            return Err(AppError::Internal(
                "历史 snapshot 不是 Document Artifact".into(),
            ))
        }
    };
    let committed = save_artifact_transaction(
        &state,
        &user,
        &id,
        expected_version,
        model,
        &transaction_id,
        "document.artifactRestored",
    )
    .await?;
    Ok(Json(committed))
}

fn artifact_for(
    id: &str,
    revision: u64,
    payload: ArtifactPayload,
) -> Result<ArtifactEnvelope, AppError> {
    let mut artifact = ArtifactEnvelope::new(id, payload);
    artifact.revision = revision;
    artifact
        .validate()
        .map_err(|error| AppError::BadRequest(format!("Artifact 无效：{error}")))?;
    Ok(artifact)
}

/// Restore/import is a system write, but it still crosses the same typed
/// transaction boundary as an editor command. The snapshot pointer,
/// idempotency row, history entry and durable outbox event are committed by
/// one SQLite transaction.
async fn save_artifact_transaction(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
    expected_version: i64,
    model: DocumentModel,
    transaction_id: &str,
    event_type: &str,
) -> Result<TransactionCommit, AppError> {
    let _write_guard = state.write_lock.lock().await;
    let meta = load_owned(state, user, id).await?;
    if let Some(record) = db::get_artifact_transaction(&state.pool, id, transaction_id).await? {
        let (can_undo, can_redo) = db::artifact_history_state(&state.pool, id).await?;
        let revision = u64::try_from(record.version)
            .map_err(|_| AppError::Internal("事务 revision 超出协议范围".into()))?;
        return Ok(TransactionCommit {
            result: CommitResult {
                protocol_version: CURRENT_PROTOCOL_VERSION,
                artifact_id: id.to_string(),
                transaction_id: transaction_id.to_string(),
                revision,
                invalidation: document_invalidation(
                    &record.changed_entities,
                    &[],
                    record.structure_changed,
                ),
                mutations: Vec::new(),
                events: Vec::new(),
            },
            document: meta,
            can_undo,
            can_redo,
        });
    }
    if meta.version != expected_version {
        return Err(AppError::VersionConflict);
    }
    let referenced_assets = model.asset_references();
    let (snapshot_key, next_version) =
        prepare_artifact_save(state, user, id, expected_version, model).await?;
    let blobs = db::get_artifact_blob_keys(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("文档 {id} 不存在")))?;
    let before_snapshot_key = blobs.snapshot_key;
    let revision = u64::try_from(next_version)
        .map_err(|_| AppError::Internal("事务 revision 超出协议范围".into()))?;
    let events = vec![DomainEventRecord {
        event_id: format!("{transaction_id}:{revision}:0"),
        type_id: event_type.into(),
        payload: serde_json::json!({ "documentId": id, "revision": revision }),
    }];
    let changed_blocks: Vec<String> = Vec::new();
    let committed = transaction_kernel::commit_candidate(
        state,
        &snapshot_key,
        db::ArtifactTransactionCommit {
            id,
            expected_version,
            snapshot_key: &snapshot_key,
            transaction_id,
            author_id: &user.id,
            client_actor_id: &user.id,
            changed_entities: &changed_blocks,
            structure_changed: true,
            origin: "import",
            commands_json: "[]",
            before_snapshot_key: &before_snapshot_key,
            events: &events,
        },
        &referenced_assets,
    )
    .await?;
    let (can_undo, can_redo) = db::artifact_history_state(&state.pool, id).await?;
    Ok(TransactionCommit {
        result: CommitResult {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            artifact_id: id.to_string(),
            transaction_id: transaction_id.to_string(),
            revision,
            invalidation: document_invalidation(&[], &[], true),
            mutations: Vec::new(),
            events,
        },
        document: committed,
        can_undo,
        can_redo,
    })
}

async fn prepare_artifact_save(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
    expected_version: i64,
    model: DocumentModel,
) -> Result<(String, i64), AppError> {
    model
        .validate()
        .map_err(|error| AppError::BadRequest(format!("Document schema 无效：{error}")))?;
    let current = load_owned(state, user, id).await?;
    if current.version != expected_version {
        return Err(AppError::VersionConflict);
    }
    let next_version = current
        .version
        .checked_add(1)
        .ok_or_else(|| AppError::Internal("文档版本溢出".into()))?;
    let artifact = artifact_for(
        id,
        u64::try_from(next_version)
            .map_err(|_| AppError::Internal("文档版本超出协议范围".into()))?,
        ArtifactPayload::Document(model),
    )?;
    let bytes = serde_json::to_vec(&artifact)
        .map_err(|error| AppError::Internal(format!("序列化 Artifact 失败：{error}")))?;
    let snapshot_key = artifact_snapshot_key(id, next_version);
    state.store.put(&snapshot_key, &bytes).await?;
    Ok((snapshot_key, next_version))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionCommit {
    #[serde(flatten)]
    pub result: CommitResult,
    pub document: ArtifactMeta,
    pub can_undo: bool,
    pub can_redo: bool,
}

async fn empty_transaction_commit(
    state: &AppState,
    artifact_id: &str,
    transaction_id: &str,
    document: ArtifactMeta,
) -> Result<TransactionCommit, AppError> {
    let (can_undo, can_redo) = db::artifact_history_state(&state.pool, artifact_id).await?;
    Ok(TransactionCommit {
        result: CommitResult {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            artifact_id: artifact_id.to_string(),
            transaction_id: transaction_id.to_string(),
            revision: u64::try_from(document.version)
                .map_err(|_| AppError::Internal("事务 revision 超出协议范围".into()))?,
            invalidation: Invalidation::default(),
            mutations: Vec::new(),
            events: Vec::new(),
        },
        document,
        can_undo,
        can_redo,
    })
}

/// Document engine 在通用 ArtifactCommandEnvelope 上的 HTTP 适配器。
///
/// 普通编辑只接受 `document.*` command。撤销/重做使用唯一的
/// `document.history` command，payload 只表达意图；反向变更和目标 snapshot
/// 完全由服务端历史表解析，浏览器不维护第二套文档模型。
pub async fn submit_transaction(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<TransactionCommit>, AppError> {
    let prepared = transaction_kernel::prepare(&headers, &id, &body)?;

    // Document transaction 使用独立日志做幂等。重试时即使文档已经进入更高 revision，
    // 也返回原提交摘要，不重复应用 block 操作。
    let _write_guard = state.write_lock.lock().await;
    let (meta, snapshot) =
        transaction_kernel::load_target(&state, &user, &id, ArtifactKind::Document).await?;
    if let Some(replay) = transaction_kernel::replay_if_committed(
        &state,
        &id,
        &meta,
        &prepared.transaction_id,
        |changed_entities, structure_changed| {
            document_invalidation(changed_entities, &[], structure_changed)
        },
    )
    .await?
    {
        return Ok(Json(replay));
    }
    transaction_kernel::require_current_revision(&state, &meta, &id, prepared.expected_revision)
        .await?;
    let transaction = prepared.envelope;
    let transaction_id = prepared.transaction_id;
    let base_revision = prepared.expected_version;

    let history = history_operation(&transaction)?;
    if let Some(history) = history {
        return submit_history_transaction(
            &state,
            &user,
            &id,
            &transaction,
            base_revision,
            &transaction_id,
            history,
        )
        .await;
    }

    let model = match snapshot.payload {
        ArtifactPayload::Document(model) => model,
        _ => return Err(AppError::Internal("事务目标不是 Document Artifact".into())),
    };
    let commands_json = serde_json::to_string(&transaction.commands)
        .map_err(|error| AppError::Internal(format!("事务记录序列化失败：{error}")))?;
    let mut engine = DocumentEngine::new(model, transaction.base_revision)
        .map_err(|error| AppError::BadRequest(format!("Document 初始化失败：{error}")))?;
    let commands = transaction
        .commands
        .into_iter()
        .map(document_command_from_record)
        .collect::<Result<Vec<_>, _>>()?;
    let result = engine
        .execute(DocumentCommandBatch {
            base_revision: transaction.base_revision,
            commands,
        })
        .map_err(document_engine_error)?;
    let referenced_assets = engine.model().asset_references();
    let (snapshot_key, _) =
        prepare_artifact_save(&state, &user, &id, base_revision, engine.model().clone()).await?;
    let blobs = db::get_artifact_blob_keys(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("文档 {id} 不存在")))?;
    let before_snapshot_key = blobs.snapshot_key;
    let revision = base_revision
        .checked_add(1)
        .ok_or_else(|| AppError::Internal("事务 revision 超出服务端范围".into()))?;
    let events = document_events(
        &transaction_id,
        revision as u64,
        &result.mutations,
        &user.id,
    )?;
    let committed = transaction_kernel::commit_candidate(
        &state,
        &snapshot_key,
        db::ArtifactTransactionCommit {
            id: &id,
            expected_version: base_revision,
            snapshot_key: &snapshot_key,
            transaction_id: &transaction_id,
            author_id: &user.id,
            client_actor_id: &transaction.actor_id,
            changed_entities: &result.changed_blocks,
            structure_changed: result.structure_changed,
            origin: transaction_origin_name(transaction.origin),
            commands_json: &commands_json,
            before_snapshot_key: &before_snapshot_key,
            events: &events,
        },
        &referenced_assets,
    )
    .await?;
    let revision = u64::try_from(committed.version)
        .map_err(|_| AppError::Internal("事务 revision 超出协议范围".into()))?;
    let (can_undo, can_redo) = db::artifact_history_state(&state.pool, &id).await?;
    Ok(Json(TransactionCommit {
        result: CommitResult {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            artifact_id: id.clone(),
            transaction_id: transaction_id.clone(),
            revision,
            invalidation: document_invalidation(
                &result.changed_blocks,
                &result.changed_containers,
                result.structure_changed,
            ),
            mutations: mutation_records(&result.mutations)?,
            events,
        },
        document: committed,
        can_undo,
        can_redo,
    }))
}

fn transaction_origin_name(origin: oo_protocol::TransactionOrigin) -> &'static str {
    match origin {
        oo_protocol::TransactionOrigin::Local => "local",
        oo_protocol::TransactionOrigin::Remote => "remote",
        oo_protocol::TransactionOrigin::Undo => "undo",
        oo_protocol::TransactionOrigin::Redo => "redo",
        oo_protocol::TransactionOrigin::Import => "import",
        oo_protocol::TransactionOrigin::System => "system",
    }
}

fn history_operation(
    transaction: &ArtifactCommandEnvelope,
) -> Result<Option<DocumentHistoryOperation>, AppError> {
    let has_history = transaction
        .commands
        .iter()
        .any(|record| record.type_id == DOCUMENT_HISTORY_TYPE_ID);
    if !has_history {
        if matches!(
            transaction.origin,
            oo_protocol::TransactionOrigin::Undo | oo_protocol::TransactionOrigin::Redo
        ) {
            return Err(AppError::BadRequest(
                "Undo/Redo transaction 必须使用 document.history command".into(),
            ));
        }
        return Ok(None);
    }
    if transaction.commands.len() != 1 {
        return Err(AppError::BadRequest(
            "document.history transaction 只能包含一个 command".into(),
        ));
    }
    let record = &transaction.commands[0];
    if record.type_id != DOCUMENT_HISTORY_TYPE_ID {
        return Err(AppError::BadRequest(
            "document.history transaction 不能混入普通 command".into(),
        ));
    }
    let operation = DocumentHistoryOperation::from_record(record)
        .map_err(|error| AppError::BadRequest(format!("history operation 无效：{error}")))?;
    let expected_origin = match operation.action {
        HistoryAction::Undo => oo_protocol::TransactionOrigin::Undo,
        HistoryAction::Redo => oo_protocol::TransactionOrigin::Redo,
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
    base_revision: i64,
    transaction_id: &str,
    history: DocumentHistoryOperation,
) -> Result<Json<TransactionCommit>, AppError> {
    let undone = matches!(history.action, HistoryAction::Redo);
    let entry = db::get_artifact_history(&state.pool, id, undone)
        .await?
        .ok_or_else(|| {
            AppError::BadRequest(match history.action {
                HistoryAction::Undo => "没有可撤销的事务".into(),
                HistoryAction::Redo => "没有可重做的事务".into(),
            })
        })?;
    let target_key = match history.action {
        HistoryAction::Undo => &entry.before_snapshot_key,
        HistoryAction::Redo => &entry.after_snapshot_key,
    };
    let model = load_model_from_snapshot_key(state, user, id, target_key).await?;
    let referenced_assets = model.asset_references();
    let (snapshot_key, _) = prepare_artifact_save(state, user, id, base_revision, model).await?;
    let commands_json = serde_json::to_string(&transaction.commands)
        .map_err(|error| AppError::Internal(format!("事务记录序列化失败：{error}")))?;
    let origin = match history.action {
        HistoryAction::Undo => "undo",
        HistoryAction::Redo => "redo",
    };
    let revision = base_revision
        .checked_add(1)
        .ok_or_else(|| AppError::Internal("事务 revision 超出服务端范围".into()))?;
    let events = vec![DomainEventRecord {
        event_id: format!("{transaction_id}:{revision}:history"),
        type_id: "document.historyApplied".into(),
        payload: serde_json::json!({
            "action": match history.action {
                HistoryAction::Undo => "undo",
                HistoryAction::Redo => "redo",
            },
            "historyId": entry.history_id,
            "actorId": user.id,
        }),
    }];
    let committed = transaction_kernel::commit_history_candidate(
        state,
        &snapshot_key,
        db::ArtifactHistoryCommit {
            id,
            expected_version: base_revision,
            snapshot_key: &snapshot_key,
            transaction_id,
            author_id: &user.id,
            client_actor_id: &transaction.actor_id,
            changed_entities: &entry.changed_entities,
            structure_changed: entry.structure_changed,
            origin,
            commands_json: &commands_json,
            history_id: entry.history_id,
            expected_undone: undone,
            next_undone: !undone,
            events: &events,
        },
        &referenced_assets,
    )
    .await?;
    let revision = u64::try_from(committed.version)
        .map_err(|_| AppError::Internal("事务 revision 超出协议范围".into()))?;
    let (can_undo, can_redo) = db::artifact_history_state(&state.pool, id).await?;
    Ok(Json(TransactionCommit {
        result: CommitResult {
            protocol_version: CURRENT_PROTOCOL_VERSION,
            artifact_id: id.to_string(),
            transaction_id: transaction_id.to_string(),
            revision,
            invalidation: document_invalidation(
                &entry.changed_entities,
                &[],
                entry.structure_changed,
            ),
            mutations: Vec::new(),
            events,
        },
        document: committed,
        can_undo,
        can_redo,
    }))
}

fn document_invalidation(
    changed_entities: &[String],
    changed_containers: &[String],
    structure_changed: bool,
) -> Invalidation {
    Invalidation {
        changed_entities: changed_entities
            .iter()
            .map(|entity_id| EntityRef {
                entity_type: "document.block".into(),
                entity_id: entity_id.clone(),
            })
            .collect(),
        changed_containers: changed_containers
            .iter()
            .map(|entity_id| EntityRef {
                entity_type: "document.container".into(),
                entity_id: entity_id.clone(),
            })
            .collect(),
        structure_changed,
    }
}

fn mutation_records(mutations: &[DocumentMutation]) -> Result<Vec<MutationRecord>, AppError> {
    mutations
        .iter()
        .map(|mutation| {
            Ok(MutationRecord {
                type_id: "document.mutation".into(),
                payload: serde_json::to_value(mutation)
                    .map_err(|error| AppError::Internal(format!("Mutation 序列化失败：{error}")))?,
            })
        })
        .collect()
}

/// Convert committed mutations into deterministic post-commit facts. Event IDs are derived from
/// the transaction ID and mutation order, so retries cannot create a second logical event. The
/// events are emitted only after the database commit succeeds; an idempotent retry intentionally
/// returns no events because no new commit happened.
fn document_events(
    transaction_id: &str,
    revision: u64,
    mutations: &[DocumentMutation],
    actor_id: &str,
) -> Result<Vec<DomainEventRecord>, AppError> {
    let mut events = mutations
        .iter()
        .enumerate()
        .map(|(index, mutation)| {
            let (type_id, payload) = match mutation {
                DocumentMutation::Insert {
                    block,
                    parent_id,
                    index,
                } => (
                    "document.blockInserted",
                    serde_json::json!({
                        "blockId": block.id,
                        "parentId": parent_id,
                        "index": index,
                    }),
                ),
                DocumentMutation::Delete {
                    block_id,
                    parent_id,
                    index,
                    ..
                } => (
                    "document.blockDeleted",
                    serde_json::json!({
                        "blockId": block_id,
                        "parentId": parent_id,
                        "index": index,
                    }),
                ),
                DocumentMutation::Update { block_id, .. } => (
                    "document.blockUpdated",
                    serde_json::json!({ "blockId": block_id }),
                ),
                DocumentMutation::Move {
                    block_id,
                    from_parent_id,
                    from_index,
                    to_parent_id,
                    to_index,
                } => (
                    "document.blockMoved",
                    serde_json::json!({
                        "blockId": block_id,
                        "fromParentId": from_parent_id,
                        "fromIndex": from_index,
                        "toParentId": to_parent_id,
                        "toIndex": to_index,
                    }),
                ),
                DocumentMutation::SetPageSetup { .. } => {
                    ("document.pageSetupChanged", serde_json::json!({}))
                }
                DocumentMutation::SetPageSemantics { .. } => {
                    ("document.pageSemanticsChanged", serde_json::json!({}))
                }
                DocumentMutation::RemoveInserted { block, .. } => (
                    "document.blockDeleted",
                    serde_json::json!({ "blockId": block.id }),
                ),
                DocumentMutation::Restore { block_id, .. } => (
                    "document.blockRestored",
                    serde_json::json!({ "blockId": block_id }),
                ),
            };
            Ok(DomainEventRecord {
                event_id: format!("{transaction_id}:{revision}:{index}"),
                type_id: type_id.into(),
                payload,
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;
    transaction_kernel::stamp_events(&mut events, actor_id);
    Ok(events)
}

fn document_command_from_record(record: CommandRecord) -> Result<DocumentCommand, AppError> {
    let command: DocumentCommand = serde_json::from_value(record.payload)
        .map_err(|error| AppError::BadRequest(format!("Document command 无效：{error}")))?;
    let expected_type_id = match &command {
        DocumentCommand::InsertBlock { .. } => "document.insertBlock",
        DocumentCommand::InsertQuote { .. } => "document.insertQuote",
        DocumentCommand::InsertTodo { .. } => "document.insertTodo",
        DocumentCommand::InsertLink { .. } => "document.insertLink",
        DocumentCommand::InsertDivider { .. } => "document.insertDivider",
        DocumentCommand::SetBlockPresentation { .. } => "document.setBlockPresentation",
        DocumentCommand::PatchInlineRange { .. } => "document.patchInlineRange",
        DocumentCommand::DeleteBlock { .. } => "document.deleteBlock",
        DocumentCommand::ResetBlock { .. } => "document.resetBlock",
        DocumentCommand::MoveBlock { .. } => "document.moveBlock",
        DocumentCommand::SetPageSetup { .. } => "document.setPageSetup",
        DocumentCommand::UpsertSection { .. } => "document.upsertSection",
        DocumentCommand::DeleteSection { .. } => "document.deleteSection",
        DocumentCommand::UpsertNote { .. } => "document.upsertNote",
        DocumentCommand::DeleteNote { .. } => "document.deleteNote",
        DocumentCommand::FormatTableCells { .. } => "document.formatTableCells",
        DocumentCommand::SetTableBorders { .. } => "document.setTableBorders",
        DocumentCommand::ApplyTableBorderPreset { .. } => "document.applyTableBorderPreset",
        DocumentCommand::SetTodoChecked { .. } => "document.setTodoChecked",
        DocumentCommand::ConvertToLink { .. } => "document.convertToLink",
        DocumentCommand::SetLinkTarget { .. } => "document.setLinkTarget",
        DocumentCommand::SetCodeConfig { .. } => "document.setCodeConfig",
        DocumentCommand::SetImageConfig { .. } => "document.setImageConfig",
        DocumentCommand::ReplaceBlockText { .. } => "document.replaceBlockText",
        DocumentCommand::ReplaceAllText { .. } => "document.replaceAllText",
        DocumentCommand::ReplaceTextMatch { .. } => "document.replaceTextMatch",
        DocumentCommand::ConvertBlock { .. } => "document.convertBlock",
        DocumentCommand::ReplaceTableCellText { .. } => "document.replaceTableCellText",
        DocumentCommand::PatchTableCellInlineRange { .. } => "document.patchTableCellInlineRange",
        DocumentCommand::InsertTableRow { .. } => "document.insertTableRow",
        DocumentCommand::InsertTableColumn { .. } => "document.insertTableColumn",
        DocumentCommand::DeleteTableRow { .. } => "document.deleteTableRow",
        DocumentCommand::DeleteTableColumn { .. } => "document.deleteTableColumn",
        DocumentCommand::SetTableColumnWidth { .. } => "document.setTableColumnWidth",
        DocumentCommand::SetTableRowHeight { .. } => "document.setTableRowHeight",
        DocumentCommand::MergeTableCells { .. } => "document.mergeTableCells",
        DocumentCommand::SplitTableCells { .. } => "document.splitTableCells",
    };
    if record.type_id != expected_type_id {
        return Err(AppError::BadRequest(format!(
            "Document 不支持 command typeId：{}",
            record.type_id
        )));
    }
    Ok(command)
}

fn document_engine_error(error: DocumentEngineError) -> AppError {
    match error {
        DocumentEngineError::RevisionConflict { .. } => AppError::VersionConflict,
        other => AppError::BadRequest(format!("Document 事务无法应用：{other}")),
    }
}

pub(crate) fn artifact_snapshot_key(document_id: &str, version: i64) -> String {
    format!("{document_id}/artifacts/{version}.json")
}

/// 采用 HTTP 标准的 If-Match 传递客户端所基于的文档 revision，例如 `If-Match: "7"`。
fn expected_version(headers: &HeaderMap) -> Result<i64, AppError> {
    i64::try_from(request_context::if_match(headers)?)
        .map_err(|_| AppError::BadRequest("If-Match 版本号超出服务端范围".into()))
}

/// Restore is a typed system/import transaction.  Requiring the caller to
/// supply its transaction id keeps retries idempotent instead of generating a
/// fresh server-side id for every HTTP retry.
fn transaction_id_header(headers: &HeaderMap) -> Result<String, AppError> {
    request_context::transaction_id(headers)
}

pub(crate) fn download_content_disposition(title: &str, extension: &str) -> String {
    let mut fallback: String = title
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, ' ' | '-' | '_' | '.') {
                character
            } else {
                '_'
            }
        })
        .collect();
    fallback = fallback.trim().trim_matches('.').to_string();
    if fallback.is_empty() {
        fallback = "document".into();
    }
    let encoded = title
        .as_bytes()
        .iter()
        .flat_map(|byte| {
            let byte = *byte;
            if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
                vec![char::from(byte)]
            } else {
                vec!['%', hex_digit(byte >> 4), hex_digit(byte & 0x0f)]
            }
        })
        .collect::<String>();
    format!(
        "attachment; filename=\"{fallback}.{extension}\"; filename*=UTF-8''{encoded}.{extension}"
    )
}

fn hex_digit(value: u8) -> char {
    match value {
        0..=9 => char::from(b'0' + value),
        10..=15 => char::from(b'A' + value - 10),
        _ => unreachable!(),
    }
}

/// 取出文档并校验归属。
///
/// 不存在与无权访问在这里被区分开：当前仍是开发用户，但把这条边界画清楚，
/// 后面接入真实用户体系时就不必回头审计每个处理器。
async fn load_owned(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
) -> Result<ArtifactMeta, AppError> {
    // 授权统一走 C4 分层：document 的写路径要求 editor 及以上。
    authorize(state, user, id, Role::Editor).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn download_filename_has_safe_ascii_fallback_and_utf8_name() {
        let value = download_content_disposition("季度/报告\"2026", "docx");
        assert!(value.starts_with("attachment; filename=\"______2026.docx\";"));
        assert!(value.contains("filename*=UTF-8''"));
        assert!(!value.contains('\n'));
        assert!(!value.contains('\r'));
    }
}
