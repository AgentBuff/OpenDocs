//! Bounded, read-only projections of canonical Artifact snapshots.
//!
//! Projections intentionally borrow the persisted DocumentModel and produce
//! short-lived JSON DTOs. They never become a second editable model and never
//! infer state from the DOM or renderer.

use std::collections::{HashMap, HashSet};

use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::artifact_routes::load_artifact;
use crate::auth::CurrentUser;
use crate::error::AppError;
use crate::AppState;
use oo_mindmap::{layout as mindmap_layout, route_edges, MindmapLayoutOptions, MindmapTheme};
use oo_presentation::v5_projection::DeckProjection;
use oo_protocol::{
    ArtifactProjectionKind, ProjectionEnvelope, CURRENT_PROTOCOL_VERSION,
    PROJECTION_CONTRACT_VERSION,
};
use oo_schema::presentation_v5::{
    Deck, SceneNode, Slide, SlideLayout, SlideMaster, SlidePageSpec, Timeline,
};
use oo_schema::{ArtifactPayload, DocumentBlock, DocumentBlockKind, DocumentModel};
use oo_spreadsheet::{GridViewport, SparseGridViewport};
use oo_whiteboard::export_projection as export_whiteboard_projection;

const DEFAULT_MAX_BYTES: usize = 256 * 1024;
const MIN_MAX_BYTES: usize = 256;
const MAX_MAX_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionQuery {
    /// Comma-separated includes. Outline supports `content` and `headingPath`;
    /// block additionally supports `refs`.
    pub include: Option<String>,
    /// Opaque cursor returned by this route. It is tied to the snapshot revision.
    pub cursor: Option<String>,
    /// Optional direct-parent filter for block reference listings.
    pub parent_id: Option<String>,
    /// Number of items requested by a block reference listing.
    pub limit: Option<usize>,
    /// Response budget, including the JSON envelope.
    pub max_bytes: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactProjectionQuery {
    /// Mindmap theme is renderer state and is deliberately not persisted.
    pub theme: Option<String>,
    /// Spreadsheet grid window, flattened so the single axum `Query` extractor
    /// serves both mindmap theme and the grid viewport.
    #[serde(flatten)]
    pub spreadsheet: SpreadsheetProjectionQuery,
}

const MIN_GRID_WINDOW_ROWS: u32 = 1;
const MAX_GRID_WINDOW_ROWS: u32 = 1_000;
const MAX_GRID_WINDOW_COLUMNS: u32 = 200;

/// Bounded spreadsheet grid window. The viewport is a read-only request, never
/// persisted; only materialized cells in the half-open range are returned so a
/// million-row sheet does not materialize a million render nodes. Deliberately
/// a standalone type: it owns its own deserialization and boundary validation.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpreadsheetProjectionQuery {
    #[serde(default)]
    pub sheet_id: Option<String>,
    #[serde(default, deserialize_with = "de_u32_query")]
    pub start_row: Option<u32>,
    #[serde(default, deserialize_with = "de_u32_query")]
    pub end_row: Option<u32>,
    #[serde(default, deserialize_with = "de_u32_query")]
    pub start_column: Option<u32>,
    #[serde(default, deserialize_with = "de_u32_query")]
    pub end_column: Option<u32>,
}

/// Query strings deserialize every value as a string; a `#[serde(flatten)]`
/// struct does not inherit the coercion axum applies to top-level numeric
/// fields, so accept both the numeric and string forms here. Missing (`null`)
/// becomes `None` so the caller's default windowing kicks in.
fn de_u32_query<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum U32Value {
        Number(u32),
        String(String),
    }
    match Option::<U32Value>::deserialize(deserializer)? {
        None => Ok(None),
        Some(U32Value::Number(n)) => Ok(Some(n)),
        Some(U32Value::String(text)) => text.parse().map(Some).map_err(serde::de::Error::custom),
    }
}

impl SpreadsheetProjectionQuery {
    fn viewport(&self) -> Result<GridViewport, AppError> {
        let start_row = self.start_row.unwrap_or(0);
        let end_row = self.end_row.unwrap_or_else(|| start_row + 30);
        let start_column = self.start_column.unwrap_or(0);
        let end_column = self.end_column.unwrap_or_else(|| start_column + 20);
        if start_row >= end_row || start_column >= end_column {
            return Err(AppError::BadRequest(
                "网格窗口必须是半开区间 [start, end)".into(),
            ));
        }
        if (end_row - start_row) > MAX_GRID_WINDOW_ROWS
            || (end_column - start_column) > MAX_GRID_WINDOW_COLUMNS
            || (end_row - start_row) < MIN_GRID_WINDOW_ROWS
        {
            return Err(AppError::BadRequest(format!(
                "网格窗口过大：行数 ≤ {MAX_GRID_WINDOW_ROWS}，列数 ≤ {MAX_GRID_WINDOW_COLUMNS}"
            )));
        }
        GridViewport::new(start_row, end_row, start_column, end_column)
            .map_err(|error| AppError::BadRequest(format!("网格窗口无效：{error}")))
    }
}

const DEFAULT_PRESENTATION_SLIDE_LIMIT: usize = 100;
const MAX_PRESENTATION_SLIDE_LIMIT: usize = 1_000;

/// Query contract for the v5 Presentation read surface.  It deliberately has
/// no write, selection, renderer or canvas fields: those are local operation
/// state and must never enter the Artifact REST boundary.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PresentationProjectionQuery {
    /// Opaque cursor returned by the Presentation outline endpoint. It is
    /// bound to the immutable snapshot revision.
    pub cursor: Option<String>,
    /// Maximum number of slide summaries in one page.
    pub limit: Option<usize>,
    /// Response budget, including the projection envelope.
    pub max_bytes: Option<usize>,
    /// Slide-only optional sections. Allowed values are `nodes`, `notes` and
    /// `timeline`; omitting it returns metadata only.
    pub include: Option<String>,
}

/// `GET /api/artifacts/{id}/projection/{kind}` returns a bounded, read-only
/// projection for non-document artifacts. The route never accepts commands.
/// A non-document write path may only be enabled after its own engine is
/// registered in the capability catalog; until then `/transactions` rejects it
/// rather than falling through to the Document engine.
pub async fn artifact_projection(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, kind)): Path<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<ArtifactProjectionQuery>,
) -> Result<Response, AppError> {
    let (_meta, artifact) = load_artifact(&state, &user, &id).await?;
    let (projection, data) = match (kind.as_str(), &artifact.payload) {
        ("mindmap", oo_schema::ArtifactPayload::Mindmap(model)) => {
            let theme = match query.theme.as_deref().unwrap_or("light") {
                "light" => MindmapTheme::Light,
                "dark" => MindmapTheme::Dark,
                "highContrast" | "high-contrast" => MindmapTheme::HighContrast,
                value => return Err(AppError::BadRequest(format!("mindmap theme 无效：{value}"))),
            };
            let layout =
                mindmap_layout(model, MindmapLayoutOptions::default()).map_err(|error| {
                    AppError::BadRequest(format!("mindmap projection 失败：{error}"))
                })?;
            let edges = route_edges(model, &layout).map_err(|error| {
                AppError::BadRequest(format!("mindmap projection 失败：{error}"))
            })?;
            (
                ArtifactProjectionKind::Mindmap,
                serde_json::json!({
                    "theme": theme,
                    "layout": layout,
                    "edges": edges,
                }),
            )
        }
        ("whiteboard", oo_schema::ArtifactPayload::Whiteboard(model)) => {
            let export = export_whiteboard_projection(model).map_err(|error| {
                AppError::BadRequest(format!("whiteboard projection 失败：{error}"))
            })?;
            (
                ArtifactProjectionKind::Whiteboard,
                serde_json::json!({
                    "scene": export,
                    "camera": model.camera,
                }),
            )
        }
        ("presentation", ArtifactPayload::Presentation(deck)) => {
            let projection = DeckProjection::new(deck).map_err(|error| {
                AppError::Internal(format!("Presentation projection 校验失败：{error}"))
            })?;
            (
                ArtifactProjectionKind::Presentation,
                serde_json::to_value(presentation_overview(deck, &projection)).map_err(
                    |error| {
                        AppError::Internal(format!("Presentation projection 序列化失败：{error}"))
                    },
                )?,
            )
        }
        ("mindmap" | "whiteboard" | "presentation", _) => {
            return Err(AppError::UnsupportedCapability(format!(
                "artifact kind 与 projection 不匹配：{kind}"
            )))
        }
        ("spreadsheet", ArtifactPayload::Spreadsheet(model)) => {
            let Some(sheet_id) = query
                .spreadsheet
                .sheet_id
                .as_deref()
                .filter(|s| !s.is_empty())
            else {
                return Err(AppError::BadRequest(
                    "spreadsheet projection 需要 sheetId".into(),
                ));
            };
            let viewport = query.spreadsheet.viewport()?;
            let projection =
                SparseGridViewport::project(model, sheet_id, viewport).map_err(|error| {
                    AppError::BadRequest(format!("spreadsheet projection 失败：{error}"))
                })?;
            (
                ArtifactProjectionKind::Spreadsheet,
                serde_json::json!({
                    "sheetId": sheet_id,
                    "startRow": viewport.start_row,
                    "endRow": viewport.end_row,
                    "startColumn": viewport.start_column,
                    "endColumn": viewport.end_column,
                    "cells": projection.cells,
                    "cellCount": projection.materialized_cell_count,
                    "sparse": projection.is_sparse(),
                }),
            )
        }
        ("spreadsheet", _) => {
            return Err(AppError::UnsupportedCapability(format!(
                "artifact kind 与 projection 不匹配：{kind}"
            )))
        }
        _ => {
            return Err(AppError::BadRequest(format!(
                "未知 artifact projection：{kind}"
            )))
        }
    };
    Ok(projection_response(
        id,
        artifact.revision,
        projection,
        data,
        None,
        None,
        &headers,
    ))
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PresentationOverview<'a> {
    page_spec: &'a SlidePageSpec,
    theme_id: &'a str,
    theme_name: &'a str,
    slide_count: usize,
    master_count: usize,
    layout_count: usize,
    asset_count: usize,
    masters: Vec<PresentationMasterOverview<'a>>,
    layouts: Vec<PresentationLayoutOverview<'a>>,
}

/// Stable metadata only. The canonical master/layout bodies intentionally stay
/// out of the overview projection so a layout picker never becomes a writable
/// second Deck model in the browser.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PresentationMasterOverview<'a> {
    id: &'a str,
    name: &'a str,
    placeholder_count: usize,
    /// A read-only, schema-complete entity. The browser uses this only as the
    /// source for an explicit `updateMaster` command; it never persists a
    /// mutable Deck clone.
    master: &'a SlideMaster,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PresentationLayoutOverview<'a> {
    id: &'a str,
    master_id: &'a str,
    name: &'a str,
    placeholder_count: usize,
    /// See `PresentationMasterOverview::master`.
    layout: &'a SlideLayout,
}

fn presentation_overview<'a>(
    deck: &'a Deck,
    _projection: &DeckProjection<'a>,
) -> PresentationOverview<'a> {
    PresentationOverview {
        page_spec: &deck.page_spec,
        theme_id: &deck.theme.id,
        theme_name: &deck.theme.name,
        slide_count: deck.slides.len(),
        master_count: deck.masters.len(),
        layout_count: deck.layouts.len(),
        asset_count: deck.assets.len(),
        masters: deck
            .masters
            .iter()
            .map(|master| PresentationMasterOverview {
                id: &master.id,
                name: &master.name,
                placeholder_count: master.placeholders.len(),
                master,
            })
            .collect(),
        layouts: deck
            .layouts
            .iter()
            .map(|layout| PresentationLayoutOverview {
                id: &layout.id,
                master_id: &layout.master_id,
                name: &layout.name,
                placeholder_count: layout.placeholders.len(),
                layout,
            })
            .collect(),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct PresentationSlideOutline<'a> {
    slide_id: &'a str,
    order_key: &'a str,
    name: &'a str,
    layout_id: Option<&'a str>,
    node_count: usize,
    timeline_entry_count: usize,
    has_notes: bool,
}

fn presentation_slide_outline(slide: &Slide) -> PresentationSlideOutline<'_> {
    PresentationSlideOutline {
        slide_id: &slide.id,
        order_key: &slide.order_key,
        name: &slide.name,
        layout_id: slide.layout_id.as_deref(),
        node_count: slide.nodes.len(),
        timeline_entry_count: slide.timeline.entries.len(),
        has_notes: slide
            .notes
            .as_deref()
            .is_some_and(|notes| !notes.trim().is_empty()),
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PresentationSlideRead<'a> {
    slide_id: &'a str,
    order_key: &'a str,
    name: &'a str,
    layout_id: Option<&'a str>,
    background: &'a oo_schema::presentation_v5::SlideBackground,
    /// Transition is a compact, renderer-neutral slide fact. Playback needs
    /// it even when nodes/timeline payloads are opt-in.
    transition: Option<&'a oo_schema::presentation_v5::SlideTransition>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<Option<&'a str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    nodes: Option<&'a [SceneNode]>,
    /// Required only when `nodes` is present so strict clients can validate
    /// image/media asset references without fetching the whole Deck.
    #[serde(skip_serializing_if = "Option::is_none")]
    asset_ids: Option<Vec<&'a str>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    timeline: Option<&'a Timeline>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PresentationNodeRead<'a> {
    slide_id: &'a str,
    node: &'a SceneNode,
    child_node_ids: Vec<&'a str>,
    /// Canonical ids only, not decoded asset bytes or renderer caches.
    asset_ids: Vec<&'a str>,
    /// Lets strict SDK parsers validate parent/connector references from a
    /// single node read without downloading sibling payloads.
    slide_node_ids: Vec<&'a str>,
}

/// `GET /api/artifacts/{id}/presentation/outline`.
///
/// Lists only slide metadata.  The response is bounded by both `limit` and
/// `maxBytes`; clients must use the opaque revision-bound cursor rather than
/// treating slide array indexes as stable identifiers.
pub async fn presentation_outline(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
    headers: HeaderMap,
    Query(query): Query<PresentationProjectionQuery>,
) -> Result<Response, AppError> {
    let max_bytes = max_bytes(query.max_bytes)?;
    reject_presentation_include(query.include.as_deref(), &[])?;
    let limit = query.limit.unwrap_or(DEFAULT_PRESENTATION_SLIDE_LIMIT);
    if !(1..=MAX_PRESENTATION_SLIDE_LIMIT).contains(&limit) {
        return Err(AppError::BadRequest(format!(
            "presentation outline limit 必须在 1 到 {MAX_PRESENTATION_SLIDE_LIMIT} 之间"
        )));
    }
    let (_meta, artifact) = load_artifact(&state, &user, &id).await?;
    let revision = artifact.revision;
    let ArtifactPayload::Presentation(deck) = artifact.payload else {
        return Err(AppError::UnsupportedCapability(
            "只有 Presentation 支持 presentation projection".into(),
        ));
    };
    DeckProjection::new(&deck).map_err(|error| {
        AppError::Internal(format!("Presentation projection 校验失败：{error}"))
    })?;
    let start = parse_cursor(query.cursor.as_deref(), revision)?;
    if start > deck.slides.len() {
        return Err(AppError::BadRequest(
            "presentation projection cursor 超出范围".into(),
        ));
    }
    let mut items = Vec::new();
    let mut next_index = None;
    for (index, slide) in deck.slides.iter().enumerate().skip(start).take(limit) {
        items.push(presentation_slide_outline(slide));
        let next_cursor = if index + 1 < deck.slides.len() {
            Some(make_cursor(revision, index + 1))
        } else {
            None
        };
        let envelope = projection_envelope(
            id.clone(),
            revision,
            ArtifactProjectionKind::PresentationOutline,
            serde_json::to_value(json!({"items": items})).map_err(|error| {
                AppError::Internal(format!("Presentation projection 序列化失败：{error}"))
            })?,
            query.cursor.clone(),
            next_cursor,
        );
        let bytes = serde_json::to_vec(&envelope)
            .map_err(|error| AppError::Internal(format!("projection 序列化失败：{error}")))?;
        if bytes.len() > max_bytes {
            if items.len() == 1 {
                return Ok(budget_error(max_bytes, bytes.len(), query.cursor));
            }
            items.pop();
            next_index = Some(index);
            break;
        }
    }
    let next_cursor = next_index
        .map(|index| make_cursor(revision, index))
        .or_else(|| {
            let next = start.saturating_add(items.len());
            (next < deck.slides.len()).then(|| make_cursor(revision, next))
        });
    Ok(projection_response(
        id,
        revision,
        ArtifactProjectionKind::PresentationOutline,
        serde_json::to_value(json!({"items": items})).map_err(|error| {
            AppError::Internal(format!("Presentation projection 序列化失败：{error}"))
        })?,
        query.cursor,
        next_cursor,
        &headers,
    ))
}

/// `GET /api/artifacts/{id}/presentation/slides/{slideId}`.
///
/// The default response is deliberately metadata-only.  `nodes`, `notes` and
/// `timeline` are explicit opt-ins so an Agent or thumbnail worker cannot
/// accidentally download a large scene graph while just enumerating slides.
pub async fn presentation_slide(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, slide_id)): Path<(String, String)>,
    headers: HeaderMap,
    Query(query): Query<PresentationProjectionQuery>,
) -> Result<Response, AppError> {
    let max_bytes = max_bytes(query.max_bytes)?;
    let includes =
        presentation_includes(query.include.as_deref(), &["nodes", "notes", "timeline"])?;
    if query.cursor.is_some() || query.limit.is_some() {
        return Err(AppError::BadRequest(
            "单个 Presentation slide 不支持 cursor 或 limit".into(),
        ));
    }
    let (_meta, artifact) = load_artifact(&state, &user, &id).await?;
    let revision = artifact.revision;
    let ArtifactPayload::Presentation(deck) = artifact.payload else {
        return Err(AppError::UnsupportedCapability(
            "只有 Presentation 支持 presentation projection".into(),
        ));
    };
    let projection = DeckProjection::new(&deck).map_err(|error| {
        AppError::Internal(format!("Presentation projection 校验失败：{error}"))
    })?;
    let slide = projection
        .slide(&slide_id)
        .map_err(|_| AppError::NotFound(format!("slide {slide_id} 不存在")))?;
    let data = PresentationSlideRead {
        slide_id: &slide.id,
        order_key: &slide.order_key,
        name: &slide.name,
        layout_id: slide.layout_id.as_deref(),
        background: &slide.background,
        transition: slide.transition.as_ref(),
        notes: includes.contains("notes").then_some(slide.notes.as_deref()),
        nodes: includes.contains("nodes").then_some(slide.nodes.as_slice()),
        asset_ids: includes.contains("nodes").then(|| {
            deck.assets
                .iter()
                .map(|asset| asset.asset_id.as_str())
                .collect()
        }),
        timeline: includes.contains("timeline").then_some(&slide.timeline),
    };
    projection_response_with_budget(
        id,
        revision,
        ArtifactProjectionKind::PresentationSlide,
        data,
        max_bytes,
        &headers,
    )
}

/// `GET /api/artifacts/{id}/presentation/slides/{slideId}/nodes/{nodeId}`.
/// Node identity is slide-scoped even when the same local node id appears on
/// another slide; this is the only node address accepted by the REST surface.
pub async fn presentation_node(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, slide_id, node_id)): Path<(String, String, String)>,
    headers: HeaderMap,
    Query(query): Query<PresentationProjectionQuery>,
) -> Result<Response, AppError> {
    let max_bytes = max_bytes(query.max_bytes)?;
    reject_presentation_include(query.include.as_deref(), &[])?;
    if query.cursor.is_some() || query.limit.is_some() {
        return Err(AppError::BadRequest(
            "单个 Presentation node 不支持 cursor 或 limit".into(),
        ));
    }
    let (_meta, artifact) = load_artifact(&state, &user, &id).await?;
    let revision = artifact.revision;
    let ArtifactPayload::Presentation(deck) = artifact.payload else {
        return Err(AppError::UnsupportedCapability(
            "只有 Presentation 支持 presentation projection".into(),
        ));
    };
    let projection = DeckProjection::new(&deck).map_err(|error| {
        AppError::Internal(format!("Presentation projection 校验失败：{error}"))
    })?;
    let node = projection
        .node_in_slide(&slide_id, &node_id)
        .map_err(|_| AppError::NotFound(format!("node {slide_id}/{node_id} 不存在")))?;
    let child_node_ids = projection
        .children(&slide_id, Some(&node_id))
        .map_err(|_| AppError::NotFound(format!("slide {slide_id} 不存在")))?
        .into_iter()
        .map(|child| child.id.as_str())
        .collect();
    projection_response_with_budget(
        id,
        revision,
        ArtifactProjectionKind::PresentationNode,
        PresentationNodeRead {
            slide_id: &slide_id,
            node,
            child_node_ids,
            asset_ids: deck
                .assets
                .iter()
                .map(|asset| asset.asset_id.as_str())
                .collect(),
            slide_node_ids: projection
                .slide(&slide_id)
                .expect("slide was resolved before node lookup")
                .nodes
                .iter()
                .map(|candidate| candidate.id.as_str())
                .collect(),
        },
        max_bytes,
        &headers,
    )
}

const DEFAULT_BLOCK_LIMIT: usize = 100;
const MAX_BLOCK_LIMIT: usize = 1_000;

#[derive(Debug, Clone, PartialEq, Eq)]
struct BlockMeta {
    block_id: String,
    parent_id: Option<String>,
    order: usize,
    heading_path: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectionBudgetError {
    error: &'static str,
    code: &'static str,
    max_bytes: usize,
    actual_bytes: usize,
    cursor: Option<String>,
}

/// `GET /api/artifacts/{id}/outline`.
pub async fn outline(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
    Query(query): Query<ProjectionQuery>,
) -> Result<Response, AppError> {
    let max_bytes = max_bytes(query.max_bytes)?;
    let includes = parse_includes(query.include.as_deref(), &["content", "headingPath"])?;
    let (_meta, artifact) = load_artifact(&state, &user, &id).await?;
    let revision = artifact.revision;
    let model = match artifact.payload {
        ArtifactPayload::Document(model) => model,
        _ => {
            return Err(AppError::UnsupportedCapability(
                "只有 Document 支持 Block projection".into(),
            ))
        }
    };
    let metas = collect_block_meta(&model);
    let by_id: HashMap<&str, &DocumentBlock> = model
        .blocks
        .iter()
        .map(|block| (block.id.as_str(), block))
        .collect();
    let start = parse_cursor(query.cursor.as_deref(), revision)?;
    if start > metas.len() {
        return Err(AppError::BadRequest("projection cursor 超出范围".into()));
    }

    let mut items = Vec::new();
    let mut next_index = None;
    for (index, item_meta) in metas.iter().enumerate().skip(start) {
        let block = by_id
            .get(item_meta.block_id.as_str())
            .ok_or_else(|| AppError::Internal("projection 引用了不存在的 block".into()))?;
        let item = outline_item(
            &id,
            block,
            item_meta,
            &by_id,
            includes.contains("content"),
            includes.contains("headingPath"),
        );
        let mut candidate = items.clone();
        candidate.push(item);
        let next_cursor = if index + 1 < metas.len() {
            Some(make_cursor(revision, index + 1))
        } else {
            None
        };
        let envelope = projection_envelope(
            id.clone(),
            revision,
            ArtifactProjectionKind::Outline,
            json!({"items": candidate}),
            query.cursor.clone(),
            next_cursor,
        );
        let bytes = serde_json::to_vec(&envelope)
            .map_err(|error| AppError::Internal(format!("projection 序列化失败：{error}")))?;
        if bytes.len() > max_bytes {
            if items.is_empty() {
                return Ok(budget_error(max_bytes, bytes.len(), query.cursor.clone()));
            }
            next_index = Some(index);
            break;
        }
        items = candidate;
    }

    let next_cursor = next_index.map(|index| make_cursor(revision, index));
    let envelope = projection_envelope(
        id,
        revision,
        ArtifactProjectionKind::Outline,
        json!({"items": items}),
        query.cursor,
        next_cursor,
    );
    Ok(Json(envelope).into_response())
}

/// `GET /api/artifacts/{id}/blocks`.
///
/// This is the bounded block-reference projection used by indexers and other
/// non-UI clients. It deliberately returns the same read-only envelope as the
/// outline route; it never exposes a writable document model.
pub async fn blocks(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
    Query(query): Query<ProjectionQuery>,
) -> Result<Response, AppError> {
    let max_bytes = max_bytes(query.max_bytes)?;
    let includes = parse_includes(query.include.as_deref(), &["content", "headingPath"])?;
    let limit = query.limit.unwrap_or(DEFAULT_BLOCK_LIMIT);
    if !(1..=MAX_BLOCK_LIMIT).contains(&limit) {
        return Err(AppError::BadRequest(format!(
            "blocks limit 必须在 1 到 {MAX_BLOCK_LIMIT} 之间"
        )));
    }
    let (_meta, artifact) = load_artifact(&state, &user, &id).await?;
    let revision = artifact.revision;
    let model = match artifact.payload {
        ArtifactPayload::Document(model) => model,
        _ => {
            return Err(AppError::UnsupportedCapability(
                "只有 Document 支持 Block projection".into(),
            ))
        }
    };
    let metas = collect_block_meta(&model)
        .into_iter()
        .filter(|meta| match query.parent_id.as_deref() {
            Some(parent_id) => meta.parent_id.as_deref() == Some(parent_id),
            None => true,
        })
        .collect::<Vec<_>>();
    let by_id: HashMap<&str, &DocumentBlock> = model
        .blocks
        .iter()
        .map(|block| (block.id.as_str(), block))
        .collect();
    let start = parse_cursor(query.cursor.as_deref(), revision)?;
    if start > metas.len() {
        return Err(AppError::BadRequest("projection cursor 超出范围".into()));
    }

    let mut items = Vec::new();
    let mut next_index = None;
    for (index, item_meta) in metas.iter().enumerate().skip(start).take(limit) {
        let block = by_id
            .get(item_meta.block_id.as_str())
            .ok_or_else(|| AppError::Internal("projection 引用了不存在的 block".into()))?;
        let item = outline_item(
            &id,
            block,
            item_meta,
            &by_id,
            includes.contains("content"),
            includes.contains("headingPath"),
        );
        items.push(item);
        if index + 1 < metas.len() {
            next_index = Some(index + 1);
        }
    }

    let next_cursor = next_index.map(|index| make_cursor(revision, index));
    let envelope = projection_envelope(
        id,
        revision,
        ArtifactProjectionKind::Block,
        json!({"items": items}),
        query.cursor,
        next_cursor,
    );
    let bytes = serde_json::to_vec(&envelope)
        .map_err(|error| AppError::Internal(format!("projection 序列化失败：{error}")))?;
    if bytes.len() > max_bytes {
        return Ok(budget_error(max_bytes, bytes.len(), envelope.cursor));
    }
    Ok(Json(envelope).into_response())
}

/// `GET /api/artifacts/{id}/blocks/{block_id}`.
pub async fn block(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, block_id)): Path<(String, String)>,
    Query(query): Query<ProjectionQuery>,
) -> Result<Response, AppError> {
    let max_bytes = max_bytes(query.max_bytes)?;
    let includes = parse_includes(
        query.include.as_deref(),
        &["content", "refs", "headingPath"],
    )?;
    let (_meta, artifact) = load_artifact(&state, &user, &id).await?;
    let revision = artifact.revision;
    let model = match artifact.payload {
        ArtifactPayload::Document(model) => model,
        _ => {
            return Err(AppError::UnsupportedCapability(
                "只有 Document 支持 Block projection".into(),
            ))
        }
    };
    let metas = collect_block_meta(&model);
    let item_meta = metas
        .iter()
        .find(|item| item.block_id == block_id)
        .ok_or_else(|| AppError::NotFound(format!("Block {block_id} 不存在")))?;
    let block = model
        .blocks
        .iter()
        .find(|block| block.id == block_id)
        .ok_or_else(|| AppError::NotFound(format!("Block {block_id} 不存在")))?;
    let by_id: HashMap<&str, &DocumentBlock> = model
        .blocks
        .iter()
        .map(|block| (block.id.as_str(), block))
        .collect();
    let data = block_item(
        &id,
        revision,
        block,
        item_meta,
        &by_id,
        ProjectionIncludes {
            content: includes.contains("content"),
            refs: includes.contains("refs"),
            heading_path: includes.contains("headingPath"),
        },
    );
    let envelope = projection_envelope(
        id,
        revision,
        ArtifactProjectionKind::Block,
        data,
        query.cursor.clone(),
        None,
    );
    let bytes = serde_json::to_vec(&envelope)
        .map_err(|error| AppError::Internal(format!("projection 序列化失败：{error}")))?;
    if bytes.len() > max_bytes {
        return Ok(budget_error(max_bytes, bytes.len(), query.cursor));
    }
    Ok(Json(envelope).into_response())
}

fn parse_includes(value: Option<&str>, allowed: &[&str]) -> Result<HashSet<String>, AppError> {
    let mut includes = HashSet::new();
    for include in value
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|include| !include.is_empty())
    {
        if !allowed.contains(&include) {
            return Err(AppError::BadRequest(format!(
                "不支持的 projection include：{include}"
            )));
        }
        includes.insert(include.to_string());
    }
    Ok(includes)
}

fn max_bytes(value: Option<usize>) -> Result<usize, AppError> {
    let value = value.unwrap_or(DEFAULT_MAX_BYTES);
    if !(MIN_MAX_BYTES..=MAX_MAX_BYTES).contains(&value) {
        return Err(AppError::BadRequest(format!(
            "maxBytes 必须在 {MIN_MAX_BYTES} 到 {MAX_MAX_BYTES} 之间"
        )));
    }
    Ok(value)
}

fn parse_cursor(cursor: Option<&str>, revision: u64) -> Result<usize, AppError> {
    let Some(cursor) = cursor else { return Ok(0) };
    let (cursor_revision, offset) = cursor
        .split_once(':')
        .ok_or_else(|| AppError::BadRequest("projection cursor 无效".into()))?;
    if cursor_revision != revision.to_string() {
        return Err(AppError::BadRequest("projection cursor 已过期".into()));
    }
    offset
        .parse()
        .map_err(|_| AppError::BadRequest("projection cursor 无效".into()))
}

fn make_cursor(revision: u64, offset: usize) -> String {
    format!("{revision}:{offset}")
}

fn projection_envelope(
    artifact_id: String,
    revision: u64,
    projection: ArtifactProjectionKind,
    data: Value,
    cursor: Option<String>,
    next_cursor: Option<String>,
) -> ProjectionEnvelope {
    ProjectionEnvelope {
        protocol_version: CURRENT_PROTOCOL_VERSION,
        contract_version: PROJECTION_CONTRACT_VERSION,
        artifact_id,
        revision,
        projection,
        truncated: next_cursor.is_some(),
        data,
        cursor,
        next_cursor,
    }
}

fn presentation_includes(raw: Option<&str>, allowed: &[&str]) -> Result<HashSet<String>, AppError> {
    let mut includes = HashSet::new();
    let Some(raw) = raw else {
        return Ok(includes);
    };
    for include in raw
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        if !allowed.contains(&include) {
            return Err(AppError::BadRequest(format!(
                "不支持的 Presentation projection include：{include}"
            )));
        }
        includes.insert(include.to_owned());
    }
    Ok(includes)
}

fn reject_presentation_include(raw: Option<&str>, allowed: &[&str]) -> Result<(), AppError> {
    let _ = presentation_includes(raw, allowed)?;
    Ok(())
}

fn projection_response(
    artifact_id: String,
    revision: u64,
    projection: ArtifactProjectionKind,
    data: Value,
    cursor: Option<String>,
    next_cursor: Option<String>,
    headers: &HeaderMap,
) -> Response {
    let envelope =
        projection_envelope(artifact_id, revision, projection, data, cursor, next_cursor);
    let etag = revision_etag(revision);
    if if_none_match(headers, &etag) {
        return (StatusCode::NOT_MODIFIED, [(header::ETAG, etag)]).into_response();
    }
    (
        [
            (header::ETAG, etag),
            (
                header::HeaderName::from_static("x-artifact-revision"),
                revision_header(revision),
            ),
        ],
        Json(envelope),
    )
        .into_response()
}

fn projection_response_with_budget<T: Serialize>(
    artifact_id: String,
    revision: u64,
    projection: ArtifactProjectionKind,
    data: T,
    max_bytes: usize,
    headers: &HeaderMap,
) -> Result<Response, AppError> {
    let data = serde_json::to_value(data).map_err(|error| {
        AppError::Internal(format!("Presentation projection 序列化失败：{error}"))
    })?;
    let envelope = projection_envelope(
        artifact_id.clone(),
        revision,
        projection,
        data.clone(),
        None,
        None,
    );
    let actual_bytes = serde_json::to_vec(&envelope)
        .map_err(|error| AppError::Internal(format!("projection 序列化失败：{error}")))?
        .len();
    if actual_bytes > max_bytes {
        return Ok(budget_error(max_bytes, actual_bytes, None));
    }
    Ok(projection_response(
        artifact_id,
        revision,
        projection,
        data,
        None,
        None,
        headers,
    ))
}

fn revision_etag(revision: u64) -> HeaderValue {
    HeaderValue::from_str(&format!("\"{revision}\""))
        .expect("artifact revision is always a valid quoted ETag")
}

fn revision_header(revision: u64) -> HeaderValue {
    HeaderValue::from_str(&revision.to_string()).expect("artifact revision is a valid header")
}

fn if_none_match(headers: &HeaderMap, etag: &HeaderValue) -> bool {
    let Ok(expected) = etag.to_str() else {
        return false;
    };
    headers
        .get(header::IF_NONE_MATCH)
        .and_then(|value| value.to_str().ok())
        .map(|value| {
            value.split(',').map(str::trim).any(|candidate| {
                candidate == "*"
                    || candidate == expected
                    || candidate
                        .strip_prefix("W/")
                        .is_some_and(|weak| weak.trim() == expected)
            })
        })
        .unwrap_or(false)
}

fn budget_error(max_bytes: usize, actual_bytes: usize, cursor: Option<String>) -> Response {
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(ProjectionBudgetError {
            error: "projection 响应超过 maxBytes 预算",
            code: "projection_budget_exceeded",
            max_bytes,
            actual_bytes,
            cursor,
        }),
    )
        .into_response()
}

fn collect_block_meta(model: &DocumentModel) -> Vec<BlockMeta> {
    let by_id: HashMap<&str, &DocumentBlock> = model
        .blocks
        .iter()
        .map(|block| (block.id.as_str(), block))
        .collect();
    let mut output = Vec::with_capacity(model.blocks.len());
    let mut visited = HashSet::new();
    for (order, id) in model.root.iter().enumerate() {
        collect_block_meta_recursive(
            id,
            None,
            order,
            Vec::new(),
            &by_id,
            &mut visited,
            &mut output,
        );
    }
    // Schema validation normally guarantees all blocks are rooted. Keeping an
    // explicit deterministic fallback makes unknown future blocks readable.
    for (order, block) in model.blocks.iter().enumerate() {
        if visited.insert(block.id.clone()) {
            output.push(BlockMeta {
                block_id: block.id.clone(),
                parent_id: None,
                order,
                heading_path: Vec::new(),
            });
        }
    }
    output
}

fn collect_block_meta_recursive(
    id: &str,
    parent_id: Option<&str>,
    order: usize,
    mut heading_path: Vec<String>,
    by_id: &HashMap<&str, &DocumentBlock>,
    visited: &mut HashSet<String>,
    output: &mut Vec<BlockMeta>,
) {
    let Some(block) = by_id.get(id) else { return };
    if !visited.insert(id.to_string()) {
        return;
    }
    if let DocumentBlockKind::Heading { level } = &block.kind {
        let level = usize::from((*level).max(1));
        heading_path.truncate(level.saturating_sub(1));
        heading_path.push(
            block
                .content
                .as_ref()
                .map(|content| content.text.clone())
                .unwrap_or_default(),
        );
    }
    output.push(BlockMeta {
        block_id: block.id.clone(),
        parent_id: parent_id.map(ToOwned::to_owned),
        order,
        heading_path: heading_path.clone(),
    });
    for (child_order, child_id) in block.children.iter().enumerate() {
        collect_block_meta_recursive(
            child_id,
            Some(&block.id),
            child_order,
            heading_path.clone(),
            by_id,
            visited,
            output,
        );
    }
}

fn outline_item(
    artifact_id: &str,
    block: &DocumentBlock,
    meta: &BlockMeta,
    by_id: &HashMap<&str, &DocumentBlock>,
    include_content: bool,
    include_heading_path: bool,
) -> Value {
    let mut item = json!({
        "blockId": block.id,
        "kind": block_kind(block),
        "parentId": meta.parent_id,
        "order": meta.order,
        "children": block.children.iter().filter_map(|id| {
            by_id.get(id.as_str()).map(|child| json!({
                "blockId": child.id,
                "kind": block_kind(child),
            }))
        }).collect::<Vec<_>>(),
    });
    if include_content {
        item["content"] = serde_json::to_value(&block.content).unwrap_or(Value::Null);
    }
    if include_heading_path {
        item["headingPath"] = json!(meta.heading_path);
    }
    if include_content {
        item["sourceRef"] = json!({"artifactId": artifact_id, "blockId": block.id});
    }
    item
}

fn block_item(
    artifact_id: &str,
    revision: u64,
    block: &DocumentBlock,
    meta: &BlockMeta,
    by_id: &HashMap<&str, &DocumentBlock>,
    includes: ProjectionIncludes,
) -> Value {
    let mut item = json!({
        "blockId": block.id,
        "kind": block_kind(block),
        "parentId": meta.parent_id,
        "order": meta.order,
        "children": block.children.iter().filter_map(|id| {
            by_id.get(id.as_str()).map(|child| json!({
                "blockId": child.id,
                "kind": block_kind(child),
            }))
        }).collect::<Vec<_>>(),
    });
    if includes.content {
        item["content"] = serde_json::to_value(&block.content).unwrap_or(Value::Null);
        // The projection envelope keeps its stable `payload` response slot for API clients,
        // while the canonical DocumentBlock field is the strict `data` envelope.
        item["payload"] = serde_json::to_value(&block.data).unwrap_or(Value::Null);
    }
    if includes.refs {
        item["refs"] = json!([{
            "artifactId": artifact_id,
            "blockId": block.id,
            "revision": revision,
            "textRange": {
                "start": 0,
                "end": block.content.as_ref().map(|content| content.text.chars().count()).unwrap_or(0),
            },
            "headingPath": meta.heading_path,
        }]);
    }
    if includes.heading_path {
        item["headingPath"] = json!(meta.heading_path);
    }
    item
}

#[derive(Debug, Clone, Copy)]
struct ProjectionIncludes {
    content: bool,
    refs: bool,
    heading_path: bool,
}

fn block_kind(block: &DocumentBlock) -> String {
    serde_json::to_value(&block.kind)
        .ok()
        .and_then(|value| {
            value
                .get("type")
                .and_then(Value::as_str)
                .map(ToOwned::to_owned)
        })
        .unwrap_or_else(|| "unknown".into())
}
