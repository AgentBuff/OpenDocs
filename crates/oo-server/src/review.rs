//! Durable Document review threads.
//!
//! Comments and suggestions are collaboration metadata, not RichText styles
//! and not a second Document model. Anchors refer to stable schema entities
//! and are resolved against the current immutable snapshot on every read.

use std::collections::{HashMap, HashSet};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::http::{HeaderMap, HeaderValue};
use axum::Json;
use chrono::{DateTime, Utc};
use oo_schema::{ArtifactPayload, BlockData, DocumentModel, RichText};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::artifact_routes::{authorize, load_artifact, Role};
use crate::auth::CurrentUser;
use crate::db::ArtifactKind;
use crate::error::AppError;
use crate::AppState;

const MAX_IDENTIFIER_BYTES: usize = 128;
const MAX_BODY_BYTES: usize = 10_000;
const MAX_MENTIONS: usize = 32;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentReviewAnchor {
    pub block_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_id: Option<String>,
    pub start: usize,
    pub end: usize,
    pub revision: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentSuggestion {
    pub original_text: String,
    pub replacement: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateReviewRequest {
    pub thread_id: String,
    pub message_id: String,
    pub anchor: DocumentReviewAnchor,
    pub body: String,
    #[serde(default)]
    pub mentions: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateSuggestionRequest {
    pub thread_id: String,
    pub message_id: String,
    pub anchor: DocumentReviewAnchor,
    pub body: String,
    #[serde(default)]
    pub mentions: Vec<String>,
    pub suggestion: DocumentSuggestion,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateReviewMessageRequest {
    pub message_id: String,
    pub body: String,
    #[serde(default)]
    pub mentions: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateReviewRequest {
    pub state: ReviewState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReviewKind {
    Comment,
    Suggestion,
}

impl ReviewKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Comment => "comment",
            Self::Suggestion => "suggestion",
        }
    }

    fn parse(value: &str) -> Result<Self, AppError> {
        match value {
            "comment" => Ok(Self::Comment),
            "suggestion" => Ok(Self::Suggestion),
            _ => Err(AppError::Internal("数据库中存在未知 review kind".into())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReviewState {
    Open,
    Resolved,
    Accepted,
    Rejected,
}

impl ReviewState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Resolved => "resolved",
            Self::Accepted => "accepted",
            Self::Rejected => "rejected",
        }
    }

    fn parse(value: &str) -> Result<Self, AppError> {
        match value {
            "open" => Ok(Self::Open),
            "resolved" => Ok(Self::Resolved),
            "accepted" => Ok(Self::Accepted),
            "rejected" => Ok(Self::Rejected),
            _ => Err(AppError::Internal("数据库中存在未知 review state".into())),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReviewAnchorState {
    Current,
    Stale,
    Detached,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewMessage {
    pub message_id: String,
    pub author_id: String,
    pub body: String,
    pub mentions: Vec<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewThread {
    pub thread_id: String,
    pub artifact_id: String,
    pub kind: ReviewKind,
    pub state: ReviewState,
    pub author_id: String,
    pub anchor: Option<DocumentReviewAnchor>,
    pub anchor_state: ReviewAnchorState,
    pub base_revision: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<DocumentSuggestion>,
    pub messages: Vec<ReviewMessage>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPage {
    pub artifact_id: String,
    pub revision: u64,
    pub threads: Vec<ReviewThread>,
}

pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<ReviewPage>, AppError> {
    let (_, artifact) = load_document(&state, &user, &id).await?;
    let document = document(&artifact.payload)?;
    let rows = sqlx::query(
        "SELECT thread_id, kind, state, author_id, anchor_json, base_revision, suggestion_json, created_at, updated_at
         FROM artifact_review_threads WHERE artifact_id = ? ORDER BY updated_at, thread_id",
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await?;
    let message_rows = sqlx::query(
        "SELECT m.message_id, m.thread_id, m.author_id, m.body, m.mentions_json, m.created_at
         FROM artifact_review_messages m JOIN artifact_review_threads t ON t.thread_id = m.thread_id
         WHERE t.artifact_id = ? ORDER BY m.created_at, m.message_id",
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await?;
    let mut messages: HashMap<String, Vec<ReviewMessage>> = HashMap::new();
    for row in message_rows {
        let thread_id: String = row.try_get("thread_id")?;
        let mentions_json: String = row.try_get("mentions_json")?;
        messages.entry(thread_id).or_default().push(ReviewMessage {
            message_id: row.try_get("message_id")?,
            author_id: row.try_get("author_id")?,
            body: row.try_get("body")?,
            mentions: decode_json(&mentions_json, "review mentions")?,
            created_at: row.try_get("created_at")?,
        });
    }
    let mut threads = Vec::with_capacity(rows.len());
    for row in rows {
        let thread_id: String = row.try_get("thread_id")?;
        let anchor_json: String = row.try_get("anchor_json")?;
        let anchor: DocumentReviewAnchor = decode_json(&anchor_json, "review anchor")?;
        let anchor_state = resolve_anchor(document, &anchor)
            .map(|_| {
                if anchor.revision == artifact.revision {
                    ReviewAnchorState::Current
                } else {
                    ReviewAnchorState::Stale
                }
            })
            .unwrap_or(ReviewAnchorState::Detached);
        let suggestion_json: Option<String> = row.try_get("suggestion_json")?;
        threads.push(ReviewThread {
            thread_id: thread_id.clone(),
            artifact_id: id.clone(),
            kind: ReviewKind::parse(&row.try_get::<String, _>("kind")?)?,
            state: ReviewState::parse(&row.try_get::<String, _>("state")?)?,
            author_id: row.try_get("author_id")?,
            anchor: (anchor_state != ReviewAnchorState::Detached).then_some(anchor),
            anchor_state,
            base_revision: row.try_get::<i64, _>("base_revision")? as u64,
            suggestion: suggestion_json
                .as_deref()
                .map(|value| decode_json(value, "review suggestion"))
                .transpose()?,
            messages: messages.remove(&thread_id).unwrap_or_default(),
            created_at: row.try_get("created_at")?,
            updated_at: row.try_get("updated_at")?,
        });
    }
    Ok(Json(ReviewPage {
        artifact_id: id,
        revision: artifact.revision,
        threads,
    }))
}

pub async fn create_comment(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
    Json(request): Json<CreateReviewRequest>,
) -> Result<(StatusCode, Json<ReviewPage>), AppError> {
    create_thread(
        &state,
        &user,
        &id,
        ReviewKind::Comment,
        request.thread_id,
        request.message_id,
        request.anchor,
        request.body,
        request.mentions,
        None,
    )
    .await?;
    let page = list(State(state), user, Path(id)).await?.0;
    Ok((StatusCode::CREATED, Json(page)))
}

pub async fn create_suggestion(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
    Json(request): Json<CreateSuggestionRequest>,
) -> Result<(StatusCode, Json<ReviewPage>), AppError> {
    create_thread(
        &state,
        &user,
        &id,
        ReviewKind::Suggestion,
        request.thread_id,
        request.message_id,
        request.anchor,
        request.body,
        request.mentions,
        Some(request.suggestion),
    )
    .await?;
    let page = list(State(state), user, Path(id)).await?.0;
    Ok((StatusCode::CREATED, Json(page)))
}

#[allow(clippy::too_many_arguments)]
async fn create_thread(
    state: &AppState,
    user: &CurrentUser,
    artifact_id: &str,
    kind: ReviewKind,
    thread_id: String,
    message_id: String,
    anchor: DocumentReviewAnchor,
    body: String,
    mentions: Vec<String>,
    suggestion: Option<DocumentSuggestion>,
) -> Result<(), AppError> {
    authorize(state, user, artifact_id, Role::Editor).await?;
    let (_, artifact) = load_document(state, user, artifact_id).await?;
    validate_identifier(&thread_id, "threadId")?;
    validate_identifier(&message_id, "messageId")?;
    validate_message(&body, &mentions)?;
    if anchor.revision != artifact.revision {
        return Err(AppError::VersionConflict);
    }
    let selected = resolve_anchor(document(&artifact.payload)?, &anchor)?;
    if let Some(suggestion) = &suggestion {
        if suggestion.replacement.len() > MAX_BODY_BYTES {
            return Err(AppError::BadRequest("suggestion replacement 过长".into()));
        }
        if suggestion.original_text != selected {
            return Err(AppError::BadRequest(
                "suggestion originalText 与当前锚点文本不一致".into(),
            ));
        }
    }
    let now = Utc::now();
    let accept_transaction_id = uuid::Uuid::new_v4().to_string();
    let anchor_json = serde_json::to_string(&anchor)
        .map_err(|error| AppError::Internal(format!("review anchor 序列化失败：{error}")))?;
    let suggestion_json = suggestion
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|error| AppError::Internal(format!("review suggestion 序列化失败：{error}")))?;
    let mentions_json = serde_json::to_string(&mentions)
        .map_err(|error| AppError::Internal(format!("review mentions 序列化失败：{error}")))?;
    let mut transaction = state.pool.begin().await?;
    sqlx::query(
        "INSERT INTO artifact_review_threads
         (thread_id, artifact_id, kind, state, author_id, anchor_json, base_revision, suggestion_json, accept_transaction_id, created_at, updated_at)
         VALUES (?, ?, ?, 'open', ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&thread_id)
    .bind(artifact_id)
    .bind(kind.as_str())
    .bind(&user.id)
    .bind(anchor_json)
    .bind(artifact.revision as i64)
    .bind(suggestion_json)
    .bind(accept_transaction_id)
    .bind(now)
    .bind(now)
    .execute(&mut *transaction)
    .await
    .map_err(map_constraint)?;
    sqlx::query(
        "INSERT INTO artifact_review_messages
         (message_id, thread_id, author_id, body, mentions_json, created_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(message_id)
    .bind(thread_id)
    .bind(&user.id)
    .bind(body)
    .bind(mentions_json)
    .bind(now)
    .execute(&mut *transaction)
    .await
    .map_err(map_constraint)?;
    transaction.commit().await?;
    Ok(())
}

pub async fn reply(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, thread_id)): Path<(String, String)>,
    Json(request): Json<CreateReviewMessageRequest>,
) -> Result<StatusCode, AppError> {
    authorize(&state, &user, &id, Role::Editor).await?;
    validate_identifier(&thread_id, "threadId")?;
    validate_identifier(&request.message_id, "messageId")?;
    validate_message(&request.body, &request.mentions)?;
    let mentions_json = serde_json::to_string(&request.mentions)
        .map_err(|error| AppError::Internal(format!("review mentions 序列化失败：{error}")))?;
    let now = Utc::now();
    let mut transaction = state.pool.begin().await?;
    let exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM artifact_review_threads WHERE artifact_id = ? AND thread_id = ?",
    )
    .bind(&id)
    .bind(&thread_id)
    .fetch_one(&mut *transaction)
    .await?;
    if exists == 0 {
        return Err(AppError::NotFound(format!(
            "Review thread {thread_id} 不存在"
        )));
    }
    sqlx::query(
        "INSERT INTO artifact_review_messages
         (message_id, thread_id, author_id, body, mentions_json, created_at) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(request.message_id)
    .bind(&thread_id)
    .bind(user.id)
    .bind(request.body)
    .bind(mentions_json)
    .bind(now)
    .execute(&mut *transaction)
    .await
    .map_err(map_constraint)?;
    sqlx::query("UPDATE artifact_review_threads SET updated_at = ? WHERE thread_id = ?")
        .bind(now)
        .bind(thread_id)
        .execute(&mut *transaction)
        .await?;
    transaction.commit().await?;
    Ok(StatusCode::CREATED)
}

pub async fn update(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, thread_id)): Path<(String, String)>,
    Json(request): Json<UpdateReviewRequest>,
) -> Result<StatusCode, AppError> {
    authorize(&state, &user, &id, Role::Editor).await?;
    let row = sqlx::query(
        "SELECT kind, state, anchor_json, suggestion_json, accept_transaction_id
         FROM artifact_review_threads WHERE artifact_id = ? AND thread_id = ?",
    )
    .bind(&id)
    .bind(&thread_id)
    .fetch_optional(&state.pool)
    .await?;
    let row = row.ok_or_else(|| AppError::NotFound(format!("Review thread {thread_id} 不存在")))?;
    let kind: String = row.try_get("kind")?;
    let current_state: String = row.try_get("state")?;
    if current_state == request.state.as_str() {
        return Ok(StatusCode::NO_CONTENT);
    }
    let parsed_current_state = ReviewState::parse(&current_state)?;
    let valid = match ReviewKind::parse(&kind)? {
        ReviewKind::Comment => matches!(request.state, ReviewState::Open | ReviewState::Resolved),
        ReviewKind::Suggestion => {
            parsed_current_state == ReviewState::Open
                && matches!(request.state, ReviewState::Accepted | ReviewState::Rejected)
        }
    };
    if !valid {
        return Err(AppError::BadRequest(
            "review state 与 thread kind 不匹配".into(),
        ));
    }
    if request.state == ReviewState::Accepted {
        accept_suggestion(&state, &user, &id, &thread_id, &row).await?;
    }
    sqlx::query(
        "UPDATE artifact_review_threads SET state = ?, updated_at = ? WHERE artifact_id = ? AND thread_id = ?",
    )
    .bind(request.state.as_str())
    .bind(Utc::now())
    .bind(id)
    .bind(thread_id)
    .execute(&state.pool)
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn accept_suggestion(
    state: &AppState,
    user: &CurrentUser,
    artifact_id: &str,
    thread_id: &str,
    row: &sqlx::sqlite::SqliteRow,
) -> Result<(), AppError> {
    let anchor: DocumentReviewAnchor =
        decode_json(&row.try_get::<String, _>("anchor_json")?, "review anchor")?;
    let suggestion: DocumentSuggestion = decode_json(
        &row.try_get::<Option<String>, _>("suggestion_json")?
            .ok_or_else(|| AppError::Internal("Suggestion thread 缺少 suggestion".into()))?,
        "review suggestion",
    )?;
    let transaction_id: String = row.try_get("accept_transaction_id")?;
    let (_, artifact) = load_document(state, user, artifact_id).await?;
    // The canonical engine revalidates the stable target, scalar range and
    // exact original text inside the transaction. Reusing the persisted
    // transaction id lets a retry finish the review-state update after a
    // previously committed replacement without applying it twice.
    let target = match (&anchor.row_id, &anchor.cell_id) {
        (None, None) => serde_json::json!({"type": "block", "blockId": anchor.block_id}),
        (Some(row_id), Some(cell_id)) => serde_json::json!({
            "type": "tableCell", "blockId": anchor.block_id, "rowId": row_id, "cellId": cell_id
        }),
        _ => return Err(AppError::BadRequest("review anchor row/cell 不完整".into())),
    };
    let body = serde_json::json!({
        "protocolVersion": oo_protocol::CURRENT_PROTOCOL_VERSION,
        "transactionId": transaction_id,
        "intentId": format!("accept-{thread_id}"),
        "artifactId": artifact_id,
        "actorId": user.id,
        "baseRevision": artifact.revision,
        "origin": "local",
        "commands": [{
            "commandId": format!("accept-{thread_id}"),
            "typeId": "document.replaceTextMatch",
            "payload": {
                "type": "replaceTextMatch",
                "target": target,
                "range": {"start": anchor.start, "end": anchor.end},
                "query": suggestion.original_text,
                "replacement": suggestion.replacement,
                "options": {}
            }
        }]
    })
    .to_string();
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::IF_MATCH,
        HeaderValue::from_str(&format!("\"{}\"", artifact.revision))
            .map_err(|error| AppError::Internal(format!("review revision header 无效：{error}")))?,
    );
    headers.insert(
        "x-transaction-id",
        HeaderValue::from_str(&transaction_id)
            .map_err(|error| AppError::Internal(format!("review transaction id 无效：{error}")))?,
    );
    let _commit = crate::document_support::submit_transaction(
        State(state.clone()),
        user.clone(),
        Path(artifact_id.to_string()),
        headers,
        body,
    )
    .await?;
    Ok(())
}

async fn load_document(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
) -> Result<(crate::db::ArtifactMeta, oo_schema::ArtifactEnvelope), AppError> {
    let loaded = load_artifact(state, user, id).await?;
    if loaded.0.kind != ArtifactKind::Document {
        return Err(AppError::UnsupportedArtifact(loaded.0.kind));
    }
    Ok(loaded)
}

fn document(payload: &ArtifactPayload) -> Result<&DocumentModel, AppError> {
    match payload {
        ArtifactPayload::Document(document) => Ok(document),
        _ => Err(AppError::Internal(
            "Document 元数据与 payload 不一致".into(),
        )),
    }
}

pub(crate) fn resolve_anchor(
    document: &DocumentModel,
    anchor: &DocumentReviewAnchor,
) -> Result<String, AppError> {
    if anchor.start > anchor.end {
        return Err(AppError::BadRequest("Document 文本锚点无效".into()));
    }
    validate_identifier(&anchor.block_id, "blockId")?;
    let block = document
        .blocks
        .iter()
        .find(|block| block.id == anchor.block_id)
        .ok_or_else(|| AppError::BadRequest("Document 文本锚点已失效".into()))?;
    let rich_text: &RichText = match (&anchor.row_id, &anchor.cell_id) {
        (None, None) => block
            .content
            .as_ref()
            .ok_or_else(|| AppError::BadRequest("目标 block 没有文本内容".into()))?,
        (Some(row_id), Some(cell_id)) => match &block.data {
            BlockData::Table(table) => table
                .rows
                .iter()
                .find(|row| row.id == *row_id)
                .and_then(|row| row.cells.iter().find(|cell| cell.id == *cell_id))
                .map(|cell| &cell.content)
                .ok_or_else(|| AppError::BadRequest("Document table cell 锚点已失效".into()))?,
            _ => return Err(AppError::BadRequest("目标 block 不是表格".into())),
        },
        _ => return Err(AppError::BadRequest("rowId 与 cellId 必须同时提供".into())),
    };
    let length = rich_text.text.chars().count();
    if anchor.end > length {
        return Err(AppError::BadRequest("Document 文本锚点越界".into()));
    }
    Ok(rich_text
        .text
        .chars()
        .skip(anchor.start)
        .take(anchor.end - anchor.start)
        .collect())
}

fn validate_identifier(value: &str, field: &str) -> Result<(), AppError> {
    if value.is_empty()
        || value.len() > MAX_IDENTIFIER_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return Err(AppError::BadRequest(format!("review {field} 非法")));
    }
    Ok(())
}

fn validate_message(body: &str, mentions: &[String]) -> Result<(), AppError> {
    if body.trim().is_empty() || body.len() > MAX_BODY_BYTES {
        return Err(AppError::BadRequest(
            "review body 不能为空或超过限制".into(),
        ));
    }
    if mentions.len() > MAX_MENTIONS
        || mentions
            .iter()
            .any(|mention| validate_identifier(mention, "mention").is_err())
        || mentions.iter().collect::<HashSet<_>>().len() != mentions.len()
    {
        return Err(AppError::BadRequest("review mentions 非法或重复".into()));
    }
    Ok(())
}

fn decode_json<T: serde::de::DeserializeOwned>(value: &str, label: &str) -> Result<T, AppError> {
    serde_json::from_str(value)
        .map_err(|error| AppError::Internal(format!("数据库 {label} JSON 无效：{error}")))
}

fn map_constraint(error: sqlx::Error) -> AppError {
    if matches!(&error, sqlx::Error::Database(database) if database.is_unique_violation()) {
        AppError::BadRequest("review threadId 或 messageId 已存在".into())
    } else {
        AppError::Database(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stable_anchor_resolves_unicode_scalar_range() {
        let document = DocumentModel::empty();
        let mut document = document;
        document.blocks[0].content.as_mut().unwrap().text = "A中🙂Z".into();
        let anchor = DocumentReviewAnchor {
            block_id: "block-1".into(),
            row_id: None,
            cell_id: None,
            start: 1,
            end: 3,
            revision: 0,
        };
        assert_eq!(resolve_anchor(&document, &anchor).unwrap(), "中🙂");
    }

    #[test]
    fn deleted_entity_detaches_anchor() {
        let document = DocumentModel::empty();
        let anchor = DocumentReviewAnchor {
            block_id: "gone".into(),
            row_id: None,
            cell_id: None,
            start: 0,
            end: 0,
            revision: 0,
        };
        assert!(resolve_anchor(&document, &anchor).is_err());
    }

    #[test]
    fn moving_stable_entity_does_not_detach_anchor() {
        let mut document = DocumentModel::empty();
        document.blocks[0].content.as_mut().unwrap().text = "kept".into();
        document.root.reverse();
        let anchor = DocumentReviewAnchor {
            block_id: "block-1".into(),
            row_id: None,
            cell_id: None,
            start: 0,
            end: 4,
            revision: 0,
        };
        assert_eq!(resolve_anchor(&document, &anchor).unwrap(), "kept");
    }
}
