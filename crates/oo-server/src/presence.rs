//! Ephemeral collaboration presence for Artifact editors.
//!
//! Presence deliberately lives outside the Artifact transaction/snapshot path:
//! it is a short-lived view projection, not Deck state, a domain event, or an
//! audit record.  A process restart therefore clears it by design.

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde::{Deserialize, Serialize};

use oo_schema::{ArtifactPayload, BlockData, DocumentModel, RichText};

use crate::artifact_routes::load_artifact;
use crate::auth::CurrentUser;
use crate::db::ArtifactKind;
use crate::error::AppError;
use crate::AppState;

const PRESENCE_TTL: Duration = Duration::from_secs(30);
const MAX_SELECTED_NODES: usize = 100;
const MAX_IDENTIFIER_BYTES: usize = 128;

#[derive(Debug, Clone, Default)]
pub struct PresenceStore {
    entries: HashMap<String, HashMap<String, PresenceEntry>>,
}

#[derive(Debug, Clone)]
struct PresenceEntry {
    participant: ArtifactPresenceParticipant,
    updated_at: Instant,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactPresenceUpdate {
    /// Required for Document presence so a delayed cursor update cannot be
    /// attached to a different revision accidentally.
    pub revision: Option<u64>,
    pub slide_id: Option<String>,
    #[serde(default)]
    pub selected_node_ids: Vec<String>,
    pub cursor: Option<PresentationPresenceCursor>,
    pub block_id: Option<String>,
    pub selection: Option<DocumentPresenceSelection>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentPresencePoint {
    pub block_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cell_id: Option<String>,
    pub offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DocumentPresenceSelection {
    pub anchor: DocumentPresencePoint,
    pub focus: DocumentPresencePoint,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresentationPresenceCursor {
    /// Coordinates in the canonical slide coordinate space, not screen pixels.
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactPresenceParticipant {
    pub session_id: String,
    pub actor_id: String,
    pub display_name: String,
    pub slide_id: Option<String>,
    pub selected_node_ids: Vec<String>,
    pub cursor: Option<PresentationPresenceCursor>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revision: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub block_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection: Option<DocumentPresenceSelection>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactPresencePage {
    pub artifact_id: String,
    pub participants: Vec<ArtifactPresenceParticipant>,
    pub ttl_ms: u64,
}

/// `PUT /api/artifacts/{id}/presence/{sessionId}` updates one in-memory
/// participant projection.  It intentionally does not accept revision or a
/// transaction id because it never writes an Artifact.
pub async fn put(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, session_id)): Path<(String, String)>,
    Json(update): Json<ArtifactPresenceUpdate>,
) -> Result<StatusCode, AppError> {
    let (_, artifact) = load_presence_artifact(&state, &user, &id).await?;
    validate_session_id(&session_id)?;
    validate_update(&artifact, &update)?;

    let participant = ArtifactPresenceParticipant {
        session_id: session_id.clone(),
        actor_id: user.id,
        display_name: user.display_name,
        slide_id: update.slide_id,
        selected_node_ids: update.selected_node_ids,
        cursor: update.cursor,
        revision: update.revision,
        block_id: update.block_id,
        selection: update.selection,
    };
    let mut store = state.presence.lock().await;
    purge_artifact(&mut store, &id);
    store.entries.entry(id).or_default().insert(
        session_id,
        PresenceEntry {
            participant,
            updated_at: Instant::now(),
        },
    );
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/artifacts/{id}/presence` returns only live in-memory projections.
pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<ArtifactPresencePage>, AppError> {
    Ok(Json(page(&state, &user, &id).await?))
}

pub(crate) async fn page(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
) -> Result<ArtifactPresencePage, AppError> {
    let (_, artifact) = load_presence_artifact(state, user, id).await?;
    let mut store = state.presence.lock().await;
    purge_artifact(&mut store, id);
    if let Some(entries) = store.entries.get_mut(id) {
        entries.retain(|_, entry| participant_is_live(&artifact, &entry.participant));
    }
    let mut participants = store
        .entries
        .get(id)
        .map(|entries| {
            entries
                .values()
                .map(|entry| entry.participant.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    participants.sort_by(|left, right| left.session_id.cmp(&right.session_id));
    Ok(ArtifactPresencePage {
        artifact_id: id.to_string(),
        participants,
        ttl_ms: PRESENCE_TTL.as_millis() as u64,
    })
}

fn purge_artifact(store: &mut PresenceStore, artifact_id: &str) {
    let Some(entries) = store.entries.get_mut(artifact_id) else {
        return;
    };
    entries.retain(|_, entry| entry.updated_at.elapsed() < PRESENCE_TTL);
    if entries.is_empty() {
        store.entries.remove(artifact_id);
    }
}

async fn load_presence_artifact(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
) -> Result<(crate::db::ArtifactMeta, oo_schema::ArtifactEnvelope), AppError> {
    let loaded = load_artifact(state, user, id).await?;
    let meta = &loaded.0;
    if !matches!(
        meta.kind,
        ArtifactKind::Document | ArtifactKind::Presentation | ArtifactKind::Mindmap
    ) {
        return Err(AppError::UnsupportedArtifact(meta.kind));
    }
    Ok(loaded)
}

fn validate_session_id(value: &str) -> Result<(), AppError> {
    if value.is_empty()
        || value.len() > MAX_IDENTIFIER_BYTES
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
    {
        return Err(AppError::BadRequest("presence sessionId 非法".into()));
    }
    Ok(())
}

fn validate_update(
    artifact: &oo_schema::ArtifactEnvelope,
    update: &ArtifactPresenceUpdate,
) -> Result<(), AppError> {
    if update
        .slide_id
        .as_deref()
        .is_some_and(|value| value.is_empty() || value.len() > MAX_IDENTIFIER_BYTES)
    {
        return Err(AppError::BadRequest("presence slideId 非法".into()));
    }
    if update.selected_node_ids.len() > MAX_SELECTED_NODES
        || update
            .selected_node_ids
            .iter()
            .any(|value| value.is_empty() || value.len() > MAX_IDENTIFIER_BYTES)
    {
        return Err(AppError::BadRequest("presence selectedNodeIds 非法".into()));
    }
    if update
        .selected_node_ids
        .iter()
        .collect::<HashSet<_>>()
        .len()
        != update.selected_node_ids.len()
    {
        return Err(AppError::BadRequest(
            "presence selectedNodeIds 不能重复".into(),
        ));
    }
    if let Some(cursor) = &update.cursor {
        if !cursor.x.is_finite() || !cursor.y.is_finite() {
            return Err(AppError::BadRequest(
                "presence cursor 必须是有限数值".into(),
            ));
        }
    }
    match &artifact.payload {
        ArtifactPayload::Document(document) => {
            if update.slide_id.is_some()
                || !update.selected_node_ids.is_empty()
                || update.cursor.is_some()
            {
                return Err(AppError::BadRequest(
                    "Document presence 不能包含 scene 字段".into(),
                ));
            }
            if update.revision != Some(artifact.revision) {
                return Err(AppError::VersionConflict);
            }
            if let Some(block_id) = &update.block_id {
                validate_identifier(block_id, "blockId")?;
                if !document.blocks.iter().any(|block| block.id == *block_id) {
                    return Err(AppError::BadRequest(
                        "Document presence blockId 已失效".into(),
                    ));
                }
            }
            if let Some(selection) = &update.selection {
                validate_document_point(document, &selection.anchor)?;
                validate_document_point(document, &selection.focus)?;
            }
        }
        _ => {
            if update.revision.is_some() || update.block_id.is_some() || update.selection.is_some()
            {
                return Err(AppError::BadRequest(
                    "Scene presence 不能包含 Document 字段".into(),
                ));
            }
        }
    }
    Ok(())
}

fn participant_is_live(
    artifact: &oo_schema::ArtifactEnvelope,
    participant: &ArtifactPresenceParticipant,
) -> bool {
    match &artifact.payload {
        ArtifactPayload::Document(document) => {
            participant.revision == Some(artifact.revision)
                && participant.block_id.as_ref().is_none_or(|block_id| {
                    document.blocks.iter().any(|block| block.id == *block_id)
                })
                && participant.selection.as_ref().is_none_or(|selection| {
                    validate_document_point(document, &selection.anchor).is_ok()
                        && validate_document_point(document, &selection.focus).is_ok()
                })
        }
        _ => true,
    }
}

fn validate_document_point(
    document: &DocumentModel,
    point: &DocumentPresencePoint,
) -> Result<(), AppError> {
    validate_identifier(&point.block_id, "selection.blockId")?;
    let block = document
        .blocks
        .iter()
        .find(|block| block.id == point.block_id)
        .ok_or_else(|| AppError::BadRequest("Document presence selection 已失效".into()))?;
    let rich_text: &RichText = match (&point.row_id, &point.cell_id) {
        (None, None) => block
            .content
            .as_ref()
            .ok_or_else(|| AppError::BadRequest("Document presence 目标没有文本".into()))?,
        (Some(row_id), Some(cell_id)) => match &block.data {
            BlockData::Table(table) => table
                .rows
                .iter()
                .find(|row| row.id == *row_id)
                .and_then(|row| row.cells.iter().find(|cell| cell.id == *cell_id))
                .map(|cell| &cell.content)
                .ok_or_else(|| AppError::BadRequest("Document presence cell 已失效".into()))?,
            _ => {
                return Err(AppError::BadRequest(
                    "Document presence 目标不是表格".into(),
                ))
            }
        },
        _ => {
            return Err(AppError::BadRequest(
                "Document presence rowId 与 cellId 必须同时提供".into(),
            ))
        }
    };
    if point.offset > rich_text.text.chars().count() {
        return Err(AppError::BadRequest(
            "Document presence selection 越界".into(),
        ));
    }
    Ok(())
}

fn validate_identifier(value: &str, field: &str) -> Result<(), AppError> {
    if value.is_empty() || value.len() > MAX_IDENTIFIER_BYTES {
        return Err(AppError::BadRequest(format!("presence {field} 非法")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_selection() {
        let update = ArtifactPresenceUpdate {
            revision: None,
            slide_id: Some("slide-1".into()),
            selected_node_ids: vec!["node-1".into(), "node-1".into()],
            cursor: None,
            block_id: None,
            selection: None,
        };
        let artifact = oo_schema::ArtifactEnvelope::new(
            "deck",
            ArtifactPayload::Presentation(Default::default()),
        );
        assert!(validate_update(&artifact, &update).is_err());
    }

    #[test]
    fn document_presence_requires_current_revision_and_valid_scalar_point() {
        let artifact = oo_schema::ArtifactEnvelope::new(
            "doc",
            ArtifactPayload::Document(DocumentModel::empty()),
        );
        let update = ArtifactPresenceUpdate {
            revision: Some(0),
            slide_id: None,
            selected_node_ids: Vec::new(),
            cursor: None,
            block_id: Some("block-1".into()),
            selection: Some(DocumentPresenceSelection {
                anchor: DocumentPresencePoint {
                    block_id: "block-1".into(),
                    row_id: None,
                    cell_id: None,
                    offset: 0,
                },
                focus: DocumentPresencePoint {
                    block_id: "block-1".into(),
                    row_id: None,
                    cell_id: None,
                    offset: 0,
                },
            }),
        };
        assert!(validate_update(&artifact, &update).is_ok());
    }
}
