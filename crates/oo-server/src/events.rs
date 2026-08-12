//! Read-only durable Artifact event feed.
//!
//! The feed exposes committed facts only. Outbox lease/claim fields remain an
//! internal delivery concern; consumers use eventId for at-least-once
//! idempotency and cursor replay.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::artifact_routes::load_artifact;
use crate::auth::CurrentUser;
use crate::error::AppError;
use crate::{db, AppState};

const DEFAULT_LIMIT: u32 = 100;
const MAX_LIMIT: u32 = 1_000;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EventQuery {
    pub since_revision: Option<u64>,
    pub cursor: Option<String>,
    pub limit: Option<u32>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactEventPage {
    pub artifact_id: String,
    pub revision: u64,
    pub events: Vec<PublicArtifactEvent>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicArtifactEvent {
    pub event_id: String,
    pub artifact_id: String,
    pub transaction_id: String,
    pub revision: u64,
    pub type_id: String,
    pub payload: Value,
}

/// `GET /api/artifacts/{id}/events?sinceRevision=&cursor=&limit=`.
pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
    Query(query): Query<EventQuery>,
) -> Result<Json<ArtifactEventPage>, AppError> {
    let (_meta, artifact) = load_artifact(&state, &user, &id).await?;
    let revision = artifact.revision;
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT);
    if !(1..=MAX_LIMIT).contains(&limit) {
        return Err(AppError::BadRequest(format!(
            "events limit 必须在 1 到 {MAX_LIMIT} 之间"
        )));
    }
    if query.cursor.is_some() && query.since_revision.is_some() {
        return Err(AppError::BadRequest(
            "events cursor 不能与 sinceRevision 同时使用".into(),
        ));
    }
    let cursor = query.cursor.as_deref().map(parse_cursor).transpose()?;
    let since_revision = query.since_revision.unwrap_or(0);
    // `sinceRevision` is exclusive. A cursor advances within a revision by
    // event id, so use a high sentinel when starting without an explicit cursor.
    let cursor = match cursor {
        Some(cursor) => Some(cursor),
        None => Some((
            i64::try_from(since_revision)
                .map_err(|_| AppError::BadRequest("sinceRevision 超出范围".into()))?,
            "\u{10ffff}",
        )),
    };
    let mut records = db::list_artifact_events(
        &state.pool,
        &id,
        i64::try_from(since_revision)
            .map_err(|_| AppError::BadRequest("sinceRevision 超出范围".into()))?,
        cursor,
        limit.saturating_add(1),
    )
    .await?;
    let has_more = records.len() > limit as usize;
    if has_more {
        records.truncate(limit as usize);
    }
    let next_cursor = has_more
        .then(|| {
            records
                .last()
                .map(|record| format!("{}:{}", record.revision, record.event_id))
        })
        .flatten();
    let events = records
        .into_iter()
        .map(|record| PublicArtifactEvent {
            event_id: record.event_id,
            artifact_id: record.artifact_id,
            transaction_id: record.transaction_id,
            revision: u64::try_from(record.revision).unwrap_or_default(),
            type_id: record.event.type_id,
            payload: record.event.payload,
        })
        .collect();
    Ok(Json(ArtifactEventPage {
        artifact_id: id,
        revision,
        events,
        next_cursor,
    }))
}

fn parse_cursor(value: &str) -> Result<(i64, &str), AppError> {
    let (revision, event_id) = value
        .split_once(':')
        .ok_or_else(|| AppError::BadRequest("events cursor 无效".into()))?;
    if event_id.is_empty() {
        return Err(AppError::BadRequest("events cursor 无效".into()));
    }
    let revision = revision
        .parse::<i64>()
        .ok()
        .filter(|revision| *revision >= 0)
        .ok_or_else(|| AppError::BadRequest("events cursor 无效".into()))?;
    Ok((revision, event_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_keeps_event_id_after_revision() {
        assert_eq!(parse_cursor("4:tx-1:4:0").unwrap(), (4, "tx-1:4:0"));
        assert!(parse_cursor("bad").is_err());
        assert!(parse_cursor("-1:event").is_err());
    }
}
