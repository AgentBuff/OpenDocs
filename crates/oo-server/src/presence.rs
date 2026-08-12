//! Ephemeral collaboration presence for Presentation.
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

use crate::artifact_routes::owned_meta;
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
    participant: PresentationPresenceParticipant,
    updated_at: Instant,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PresentationPresenceUpdate {
    pub slide_id: Option<String>,
    #[serde(default)]
    pub selected_node_ids: Vec<String>,
    pub cursor: Option<PresentationPresenceCursor>,
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
pub struct PresentationPresenceParticipant {
    pub session_id: String,
    pub actor_id: String,
    pub display_name: String,
    pub slide_id: Option<String>,
    pub selected_node_ids: Vec<String>,
    pub cursor: Option<PresentationPresenceCursor>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationPresencePage {
    pub artifact_id: String,
    pub participants: Vec<PresentationPresenceParticipant>,
    pub ttl_ms: u64,
}

/// `PUT /api/artifacts/{id}/presence/{sessionId}` updates one in-memory
/// participant projection.  It intentionally does not accept revision or a
/// transaction id because it never writes an Artifact.
pub async fn put(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, session_id)): Path<(String, String)>,
    Json(update): Json<PresentationPresenceUpdate>,
) -> Result<StatusCode, AppError> {
    ensure_presentation(&state, &user, &id).await?;
    validate_session_id(&session_id)?;
    validate_update(&update)?;

    let participant = PresentationPresenceParticipant {
        session_id: session_id.clone(),
        actor_id: user.id,
        display_name: user.display_name,
        slide_id: update.slide_id,
        selected_node_ids: update.selected_node_ids,
        cursor: update.cursor,
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
) -> Result<Json<PresentationPresencePage>, AppError> {
    ensure_presentation(&state, &user, &id).await?;
    let mut store = state.presence.lock().await;
    purge_artifact(&mut store, &id);
    let mut participants = store
        .entries
        .get(&id)
        .map(|entries| {
            entries
                .values()
                .map(|entry| entry.participant.clone())
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    participants.sort_by(|left, right| left.session_id.cmp(&right.session_id));
    Ok(Json(PresentationPresencePage {
        artifact_id: id,
        participants,
        ttl_ms: PRESENCE_TTL.as_millis() as u64,
    }))
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

async fn ensure_presentation(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
) -> Result<(), AppError> {
    let meta = owned_meta(state, user, id).await?;
    if meta.kind != ArtifactKind::Presentation {
        return Err(AppError::UnsupportedArtifact(meta.kind));
    }
    Ok(())
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

fn validate_update(update: &PresentationPresenceUpdate) -> Result<(), AppError> {
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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_duplicate_selection() {
        let update = PresentationPresenceUpdate {
            slide_id: Some("slide-1".into()),
            selected_node_ids: vec!["node-1".into(), "node-1".into()],
            cursor: None,
        };
        assert!(validate_update(&update).is_err());
    }
}
