//! Canonical, polymorphic Artifact REST boundary.
//!
//! Resource names live here instead of being duplicated per editor product:
//! metadata, snapshots, revisions, transactions, source and exports all use
//! the same `/api/artifacts/{id}` hierarchy.  A route selects an engine from
//! the persisted `ArtifactKind`; it never infers a kind from a legacy path or
//! silently downgrades a spreadsheet/presentation to a document.

use axum::extract::{Multipart, Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use crate::auth::CurrentUser;
use crate::db::{self, ArtifactKind, ArtifactMeta, NewArtifact};
use crate::document_support;
use crate::error::AppError;
use crate::AppState;
use oo_protocol::{
    ArtifactCapability, ArtifactCapabilityStatus, ArtifactCommandCapability,
    ArtifactTransportCapability, CapabilityCatalog, DomainEventRecord, SnapshotEnvelope,
    CAPABILITY_CONTRACT_VERSION, CURRENT_PROTOCOL_VERSION,
};
use oo_schema::{ArtifactEnvelope, ArtifactPayload, BlockData, DocumentModel};

const MAX_UPLOAD_BYTES: usize = 32 * 1024 * 1024;

/// Adapter-neutral imported media.  Importers return bytes outside their
/// canonical Artifact payload; this is the single bridge that persists those
/// bytes through the Artifact asset service rather than embedding them in a
/// Deck/Document snapshot.
#[derive(Debug, Clone)]
struct ImportedBinaryAsset {
    asset_id: String,
    content_type: String,
    file_name: String,
    bytes: Vec<u8>,
}

/// Stable paths used by the canonical Artifact API.
pub const ARTIFACTS_PATH: &str = "/api/artifacts";
pub const ARTIFACT_IMPORT_PATH: &str = "/api/artifacts/import";
pub const ARTIFACT_META_PATH: &str = "/api/artifacts/{id}";
pub const ARTIFACT_SNAPSHOT_PATH: &str = "/api/artifacts/{id}/snapshot";
pub const ARTIFACT_REVISIONS_PATH: &str = "/api/artifacts/{id}/revisions";
pub const ARTIFACT_REVISION_PATH: &str = "/api/artifacts/{id}/revisions/{version}";
pub const ARTIFACT_REVISION_RESTORE_PATH: &str = "/api/artifacts/{id}/revisions/{version}/restore";
pub const ARTIFACT_HISTORY_PATH: &str = "/api/artifacts/{id}/history";
pub const ARTIFACT_TRANSACTIONS_PATH: &str = "/api/artifacts/{id}/transactions";
pub const ARTIFACT_SOURCE_PATH: &str = "/api/artifacts/{id}/source";
pub const ARTIFACT_EXPORT_PATH: &str = "/api/artifacts/{id}/export/{format}";
pub const ARTIFACT_ASSETS_PATH: &str = "/api/artifacts/{id}/assets";
pub const ARTIFACT_ASSET_PATH: &str = "/api/artifacts/{id}/assets/{asset_id}";

/// `GET /api/capabilities` is the single machine-readable discovery document
/// for agents, SDKs and MCP adapters. It lists only commands that the current
/// server can dispatch; future Artifact kinds remain explicit namespaces with
/// an empty command set rather than speculative pseudo-capabilities.
pub async fn capabilities() -> Json<CapabilityCatalog> {
    Json(CapabilityCatalog {
        protocol_version: CURRENT_PROTOCOL_VERSION,
        contract_version: CAPABILITY_CONTRACT_VERSION,
        transport: ArtifactTransportCapability {
            snapshot_endpoint: "/api/artifacts/{artifactId}/snapshot".into(),
            transaction_endpoint: "/api/artifacts/{artifactId}/transactions".into(),
            revision_header: "If-Match".into(),
            idempotency_header: "x-transaction-id".into(),
        },
        artifacts: vec![
            ArtifactCapability {
                kind: ArtifactKind::Document,
                namespace: "document".into(),
                status: ArtifactCapabilityStatus::Stable,
                commands: document_command_capabilities(),
            },
            planned_capability(ArtifactKind::Spreadsheet, "spreadsheet"),
            ArtifactCapability {
                kind: ArtifactKind::Presentation,
                namespace: "presentation".into(),
                status: ArtifactCapabilityStatus::Stable,
                commands: presentation_command_capabilities(),
            },
            planned_capability(ArtifactKind::Mindmap, "mindmap"),
            planned_capability(ArtifactKind::Whiteboard, "whiteboard"),
        ],
    })
}

fn planned_capability(kind: ArtifactKind, namespace: &str) -> ArtifactCapability {
    ArtifactCapability {
        kind,
        namespace: namespace.into(),
        status: ArtifactCapabilityStatus::Planned,
        commands: Vec::new(),
    }
}

fn document_command_capabilities() -> Vec<ArtifactCommandCapability> {
    const COMMANDS: &[(&str, &str)] = &[
        ("document.insertBlock", "document"),
        ("document.insertQuote", "document"),
        ("document.insertTodo", "document"),
        ("document.insertLink", "document"),
        ("document.insertDivider", "document"),
        ("document.setBlockPresentation", "document"),
        ("document.patchInlineRange", "document"),
        ("document.deleteBlock", "document"),
        ("document.resetBlock", "document"),
        ("document.moveBlock", "document"),
        ("document.setPageSetup", "document"),
        ("document.formatTableCells", "document.table"),
        ("document.setTableBorders", "document.table"),
        ("document.applyTableBorderPreset", "document.table"),
        ("document.setTodoChecked", "document"),
        ("document.convertToLink", "document"),
        ("document.setLinkTarget", "document"),
        ("document.setCodeConfig", "document"),
        ("document.setImageConfig", "document.image"),
        ("document.replaceBlockText", "document"),
        ("document.convertBlock", "document"),
        ("document.replaceTableCellText", "document.table"),
        ("document.patchTableCellInlineRange", "document.table"),
        ("document.insertTableRow", "document.table"),
        ("document.insertTableColumn", "document.table"),
        ("document.deleteTableRow", "document.table"),
        ("document.deleteTableColumn", "document.table"),
        ("document.setTableColumnWidth", "document.table"),
        ("document.setTableRowHeight", "document.table"),
        ("document.mergeTableCells", "document.table"),
        ("document.splitTableCells", "document.table"),
        ("document.history", "document.history"),
    ];
    COMMANDS
        .iter()
        .map(|(type_id, scope)| ArtifactCommandCapability {
            type_id: (*type_id).into(),
            scope: (*scope).into(),
            requires_revision: true,
            supports_idempotency: true,
        })
        .collect()
}

/// The capability catalog intentionally mirrors only concrete variants in
/// `PresentationCommand` plus the server-authoritative history intent. In
/// particular it does not advertise renderer gestures or generic patches.
fn presentation_command_capabilities() -> Vec<ArtifactCommandCapability> {
    const COMMANDS: &[(&str, &str)] = &[
        ("presentation.setPageSpec", "presentation.deck"),
        ("presentation.createSlide", "presentation.slide"),
        ("presentation.deleteSlide", "presentation.slide"),
        ("presentation.moveSlide", "presentation.slide"),
        ("presentation.insertNode", "presentation.node"),
        ("presentation.deleteNode", "presentation.node"),
        ("presentation.moveNode", "presentation.node"),
        ("presentation.reorderNode", "presentation.node"),
        ("presentation.groupNodes", "presentation.node"),
        ("presentation.ungroupNodes", "presentation.node"),
        ("presentation.setNodeTransform", "presentation.node"),
        ("presentation.setShapeStyle", "presentation.node.shape"),
        ("presentation.setTextContent", "presentation.node.text"),
        ("presentation.setTextFrame", "presentation.node.text"),
        ("presentation.setImageConfig", "presentation.node.image"),
        ("presentation.setMediaConfig", "presentation.node.media"),
        ("presentation.setSlideNotes", "presentation.slide"),
        ("presentation.setSlideBackground", "presentation.slide"),
        ("presentation.setSlideLayout", "presentation.slide"),
        ("presentation.setTheme", "presentation.deck"),
        ("presentation.setSlideTransition", "presentation.slide"),
        (
            "presentation.upsertAnimation",
            "presentation.slide.timeline",
        ),
        (
            "presentation.deleteAnimation",
            "presentation.slide.timeline",
        ),
        ("presentation.moveAnimation", "presentation.slide.timeline"),
        ("presentation.history", "presentation.history"),
    ];
    COMMANDS
        .iter()
        .map(|(type_id, scope)| ArtifactCommandCapability {
            type_id: (*type_id).into(),
            scope: (*scope).into(),
            requires_revision: true,
            supports_idempotency: true,
        })
        .collect()
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactList {
    pub artifacts: Vec<ArtifactMeta>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateArtifactRequest {
    pub kind: ArtifactKind,
    pub title: Option<String>,
}

/// `GET /api/artifacts`.
pub async fn list(
    State(state): State<AppState>,
    user: CurrentUser,
) -> Result<Json<ArtifactList>, AppError> {
    let artifacts = db::list_artifacts(&state.pool, &user.id).await?;
    Ok(Json(ArtifactList { artifacts }))
}

/// `POST /api/artifacts` creates an empty model for the requested kind.
pub async fn create(
    State(state): State<AppState>,
    user: CurrentUser,
    Json(request): Json<CreateArtifactRequest>,
) -> Result<Response, AppError> {
    let id = uuid::Uuid::new_v4().to_string();
    let payload = empty_payload(request.kind);
    let artifact = artifact_for(&id, 1, payload)?;
    let snapshot = serde_json::to_vec(&artifact)
        .map_err(|error| AppError::Internal(format!("序列化 Artifact 失败：{error}")))?;
    let title = request
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| default_title(request.kind).to_string());
    let source_key = format!("{id}/source.{}", source_extension(request.kind));
    let snapshot_key = document_support::artifact_snapshot_key(&id, 1);
    state.store.put(&source_key, &[]).await?;
    state.store.put(&snapshot_key, &snapshot).await?;
    let events = [created_event(&id, request.kind)];
    let meta = db::insert_artifact(
        &state.pool,
        NewArtifact {
            id: &id,
            kind: request.kind,
            title: &title,
            owner_id: &user.id,
            size: snapshot.len() as i64,
            source_key: &source_key,
            snapshot_key: &snapshot_key,
            events: &events,
        },
    )
    .await?;
    register_blob_integrity(&state, &id, &source_key, "source", &[]).await?;
    register_blob_integrity(&state, &id, &snapshot_key, "snapshot", &snapshot).await?;
    Ok((StatusCode::CREATED, Json(meta)).into_response())
}

/// `POST /api/artifacts/import` imports DOCX, XLSX or PPTX by explicit filename extension.
pub async fn import(
    State(state): State<AppState>,
    user: CurrentUser,
    mut multipart: Multipart,
) -> Result<Response, AppError> {
    let mut file_name = None;
    let mut bytes = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| AppError::BadRequest(format!("表单解析失败：{error}")))?
    {
        if field.name() == Some("file") {
            file_name = field.file_name().map(str::to_string);
            let data = field
                .bytes()
                .await
                .map_err(|error| AppError::BadRequest(format!("读取上传内容失败：{error}")))?;
            if data.len() > MAX_UPLOAD_BYTES {
                return Err(AppError::BadRequest(format!(
                    "文件超过 {} MB 的上限",
                    MAX_UPLOAD_BYTES / 1024 / 1024
                )));
            }
            bytes = Some(data);
        }
    }
    let bytes = bytes.ok_or_else(|| AppError::BadRequest("缺少 file 字段".into()))?;
    if bytes.is_empty() {
        return Err(AppError::BadRequest("上传内容为空".into()));
    }
    let file_name = file_name.ok_or_else(|| AppError::BadRequest("缺少文件名扩展名".into()))?;
    let kind = import_kind(&file_name)?;
    let id = uuid::Uuid::new_v4().to_string();
    let mut imported_assets: Vec<ImportedBinaryAsset> = Vec::new();
    let (payload, warnings) = match kind {
        ArtifactKind::Document => {
            let imported = oo_docx::parse_docx_with_assets(&bytes, &id)?;
            imported_assets = imported
                .assets
                .into_iter()
                .map(|asset| ImportedBinaryAsset {
                    asset_id: asset.asset_id,
                    content_type: asset.content_type,
                    file_name: asset.file_name,
                    bytes: asset.bytes,
                })
                .collect();
            (ArtifactPayload::Document(imported.document), Vec::new())
        }
        ArtifactKind::Spreadsheet => {
            let result = oo_xlsx::read_xlsx_with_report(&bytes)
                .map_err(|error| AppError::BadRequest(format!("XLSX 解析失败：{error}")))?;
            (
                ArtifactPayload::Spreadsheet(result.model),
                result.ignored_parts,
            )
        }
        ArtifactKind::Presentation => {
            let imported = oo_pptx::parse_pptx_with_report(&bytes)
                .map_err(|error| AppError::BadRequest(format!("PPTX 解析失败：{error}")))?;
            require_lossless_pptx_import(&imported.loss_report)?;
            imported_assets = imported
                .assets
                .into_iter()
                .map(|asset| ImportedBinaryAsset {
                    file_name: presentation_asset_file_name(
                        &asset.asset.asset_id,
                        &asset.asset.mime_type,
                    ),
                    asset_id: asset.asset.asset_id,
                    content_type: asset.asset.mime_type,
                    bytes: asset.bytes,
                })
                .collect();
            (ArtifactPayload::Presentation(imported.deck), Vec::new())
        }
        ArtifactKind::Mindmap | ArtifactKind::Whiteboard => {
            return Err(AppError::UnsupportedCapability(format!(
                "暂不支持导入 {kind:?} 文件"
            )))
        }
    };
    let artifact = artifact_for(&id, 1, payload)?;
    let snapshot = serde_json::to_vec(&artifact)
        .map_err(|error| AppError::Internal(format!("序列化 Artifact 失败：{error}")))?;
    let source_key = format!("{id}/source.{}", source_extension(kind));
    let snapshot_key = document_support::artifact_snapshot_key(&id, 1);
    state.store.put(&source_key, &bytes).await?;
    state.store.put(&snapshot_key, &snapshot).await?;
    let title = title_from_file_name(&file_name, kind);
    let events = [created_event(&id, kind)];
    let meta = db::insert_artifact(
        &state.pool,
        NewArtifact {
            id: &id,
            kind,
            title: &title,
            owner_id: &user.id,
            size: bytes.len() as i64,
            source_key: &source_key,
            snapshot_key: &snapshot_key,
            events: &events,
        },
    )
    .await?;
    register_blob_integrity(&state, &id, &source_key, "source", &bytes).await?;
    register_blob_integrity(&state, &id, &snapshot_key, "snapshot", &snapshot).await?;
    if !imported_assets.is_empty() {
        if let Err(error) = register_imported_assets(
            &state,
            &id,
            &imported_assets,
            &asset_references(&artifact.payload),
        )
        .await
        {
            // The artifact row is not useful without its media objects. Best-effort cleanup
            // keeps a failed import from leaving an unreachable snapshot and orphan blobs.
            for asset in &imported_assets {
                let _ = state
                    .store
                    .delete(&format!("{id}/assets/{}", asset.asset_id))
                    .await;
            }
            let _ = state.store.delete(&source_key).await;
            let _ = state.store.delete(&snapshot_key).await;
            let _ = db::delete_artifact(&state.pool, &id).await;
            return Err(error);
        }
    }
    Ok((
        StatusCode::CREATED,
        Json(ImportResponse {
            artifact: meta,
            warnings,
        }),
    )
        .into_response())
}

/// Online imports must never materialize a partial Presentation as if it were
/// a faithful editable deck.  The original upload is only persisted after the
/// complete canonical payload (including binary assets) has passed this gate.
///
/// The adapter's report is deliberately surfaced in the error instead of being
/// converted to a warning: callers can preserve the original PPTX externally
/// and retry once the missing typed capability exists.
fn require_lossless_pptx_import(report: &oo_pptx::PptxLossReport) -> Result<(), AppError> {
    if report.unsupported.is_empty() {
        return Ok(());
    }
    let details = report
        .unsupported
        .iter()
        .map(|item| format!("{} ({})", item.capability, item.part))
        .collect::<Vec<_>>()
        .join("，");
    Err(AppError::UnsupportedCapability(format!(
        "PPTX 导入包含尚不支持的内容，拒绝创建不完整的 Presentation：{details}"
    )))
}

fn asset_references(payload: &ArtifactPayload) -> Vec<String> {
    match payload {
        ArtifactPayload::Document(document) => document_image_asset_references(document),
        ArtifactPayload::Presentation(deck) => deck
            .assets
            .iter()
            .map(|asset| asset.asset_id.clone())
            .collect(),
        ArtifactPayload::Spreadsheet(_)
        | ArtifactPayload::Mindmap(_)
        | ArtifactPayload::Whiteboard(_) => Vec::new(),
    }
}

/// Asset references keep both the active rendition and its original source alive.
/// The latter is intentionally not exported to DOCX, but must survive compression
/// so that the image toolbar can restore the original without a second upload.
fn document_image_asset_references(document: &DocumentModel) -> Vec<String> {
    let blocks = document
        .blocks
        .iter()
        .map(|block| (block.id.as_str(), block))
        .collect::<std::collections::HashMap<_, _>>();
    let mut references = Vec::new();
    fn visit(
        id: &str,
        blocks: &std::collections::HashMap<&str, &oo_schema::DocumentBlock>,
        references: &mut Vec<String>,
    ) {
        let Some(block) = blocks.get(id) else {
            return;
        };
        if let BlockData::Image(image) = &block.data {
            references.push(image.asset_id.clone());
            if let Some(original_asset_id) = &image.original_asset_id {
                references.push(original_asset_id.clone());
            }
        }
        for child in &block.children {
            visit(child, blocks, references);
        }
    }
    for root in &document.root {
        visit(root, &blocks, &mut references);
    }
    references
}

/// DOCX receives only the currently rendered image, never a hidden recovery copy.
fn document_image_render_asset_ids(document: &DocumentModel) -> Vec<String> {
    let blocks = document
        .blocks
        .iter()
        .map(|block| (block.id.as_str(), block))
        .collect::<std::collections::HashMap<_, _>>();
    let mut references = Vec::new();
    fn visit(
        id: &str,
        blocks: &std::collections::HashMap<&str, &oo_schema::DocumentBlock>,
        references: &mut Vec<String>,
    ) {
        let Some(block) = blocks.get(id) else {
            return;
        };
        if let BlockData::Image(image) = &block.data {
            references.push(image.asset_id.clone());
        }
        for child in &block.children {
            visit(child, blocks, references);
        }
    }
    for root in &document.root {
        visit(root, &blocks, &mut references);
    }
    references
}

async fn docx_assets_for_export(
    state: &AppState,
    artifact_id: &str,
    document: &DocumentModel,
) -> Result<Vec<oo_docx::DocxAsset>, AppError> {
    let mut assets = Vec::new();
    let mut loaded = HashSet::new();
    for asset_id in document_image_render_asset_ids(document) {
        if !loaded.insert(asset_id.clone()) {
            continue;
        }
        let asset = db::get_artifact_asset(&state.pool, artifact_id, &asset_id)
            .await?
            .ok_or_else(|| {
                AppError::Internal(format!(
                    "Document 引用了不存在的图片资产 {artifact_id}/{asset_id}"
                ))
            })?;
        let bytes =
            crate::store::get_verified(state.store.as_ref(), &asset.object_key, &asset.checksum)
                .await
                .map_err(|error| {
                    AppError::Internal(format!("读取 DOCX 图片资产 {asset_id} 失败：{error}"))
                })?;
        assets.push(oo_docx::DocxAsset {
            asset_id: asset.asset_id,
            content_type: asset.content_type,
            file_name: asset.file_name,
            bytes,
        });
    }
    Ok(assets)
}

/// Presentation export resolves image bytes through the Artifact asset store.
/// Deck only contains immutable asset metadata, never a data URI or renderer
/// cache, so missing bytes are a server integrity error rather than a lossy
/// PPTX export.
async fn pptx_assets_for_export(
    state: &AppState,
    artifact_id: &str,
    deck: &oo_schema::presentation_v5::Deck,
) -> Result<oo_pptx::PptxAssetSource, AppError> {
    let mut assets = oo_pptx::PptxAssetSource::new();
    for asset_ref in &deck.assets {
        let asset = db::get_artifact_asset(&state.pool, artifact_id, &asset_ref.asset_id)
            .await?
            .ok_or_else(|| {
                AppError::Internal(format!(
                    "Presentation 引用了不存在的图片资产 {artifact_id}/{}",
                    asset_ref.asset_id
                ))
            })?;
        require_presentation_asset_matches_store(asset_ref, &asset)?;
        let bytes =
            crate::store::get_verified(state.store.as_ref(), &asset.object_key, &asset.checksum)
                .await
                .map_err(|error| {
                    AppError::Internal(format!(
                        "读取 Presentation 图片资产 {} 失败：{error}",
                        asset_ref.asset_id
                    ))
                })?;
        assets.insert(asset_ref.asset_id.clone(), bytes);
    }
    Ok(assets)
}

/// A Presentation snapshot is an immutable declaration of the binary assets
/// it renders.  Looking up only by asset id would permit a corrupted snapshot
/// to export different bytes from the declared digest/MIME metadata.  Validate
/// the closure before the adapter sees the bytes.
fn require_presentation_asset_matches_store(
    asset_ref: &oo_schema::presentation_v5::AssetRef,
    stored: &db::ArtifactAsset,
) -> Result<(), AppError> {
    if asset_ref.digest != stored.checksum {
        return Err(AppError::Internal(format!(
            "Presentation asset {} 摘要与已验证存储不一致",
            asset_ref.asset_id
        )));
    }
    if asset_ref.mime_type != stored.content_type {
        return Err(AppError::Internal(format!(
            "Presentation asset {} MIME 类型与已验证存储不一致",
            asset_ref.asset_id
        )));
    }
    Ok(())
}

async fn register_imported_assets(
    state: &AppState,
    artifact_id: &str,
    assets: &[ImportedBinaryAsset],
    references: &[String],
) -> Result<(), AppError> {
    for asset in assets {
        let object_key = format!("{artifact_id}/assets/{}", asset.asset_id);
        let digest =
            crate::store::put_verified(state.store.as_ref(), &object_key, &asset.bytes).await?;
        let now = chrono::Utc::now();
        let row = db::ArtifactAsset {
            artifact_id: artifact_id.into(),
            asset_id: asset.asset_id.clone(),
            object_key: object_key.clone(),
            content_type: asset.content_type.clone(),
            file_name: asset.file_name.clone(),
            checksum: digest.checksum.clone(),
            size: i64::try_from(digest.size)
                .map_err(|_| AppError::BadRequest("导入资产过大".into()))?,
            ref_count: 0,
            created_at: now,
            updated_at: now,
        };
        db::insert_artifact_asset(&state.pool, &row).await?;
        db::register_blob_integrity(
            &state.pool,
            artifact_id,
            &object_key,
            "asset",
            &digest.checksum,
            row.size,
        )
        .await?;
    }
    db::set_artifact_asset_references(&state.pool, artifact_id, references).await?;
    Ok(())
}

fn presentation_asset_file_name(asset_id: &str, mime_type: &str) -> String {
    let extension = match mime_type {
        "image/png" => "png",
        "image/jpeg" => "jpg",
        "image/gif" => "gif",
        "image/bmp" => "bmp",
        "image/tiff" => "tiff",
        "image/svg+xml" => "svg",
        _ => "bin",
    };
    format!("{asset_id}.{extension}")
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetList {
    pub assets: Vec<db::ArtifactAsset>,
}

/// `POST /api/artifacts/{id}/assets` stores one binary asset with a verified
/// SHA-256 digest. Assets are independent objects; the snapshot only stores
/// the stable asset id and never embeds bytes.
pub async fn upload_asset(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
    mut multipart: Multipart,
) -> Result<Response, AppError> {
    owned_meta(&state, &user, &id).await?;
    let mut file_name = None;
    let mut content_type = None;
    let mut bytes = None;
    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|error| AppError::BadRequest(format!("资产表单解析失败：{error}")))?
    {
        if field.name() == Some("file") {
            file_name = field.file_name().map(str::to_string);
            content_type = field.content_type().map(str::to_string);
            let data = field
                .bytes()
                .await
                .map_err(|error| AppError::BadRequest(format!("读取资产失败：{error}")))?;
            if data.len() > MAX_UPLOAD_BYTES {
                return Err(AppError::BadRequest(format!(
                    "资产超过 {} MB 的上限",
                    MAX_UPLOAD_BYTES / 1024 / 1024
                )));
            }
            bytes = Some(data);
        }
    }
    let bytes = bytes.ok_or_else(|| AppError::BadRequest("缺少 file 字段".into()))?;
    if bytes.is_empty() {
        return Err(AppError::BadRequest("资产内容为空".into()));
    }
    let asset_id = uuid::Uuid::new_v4().to_string();
    let file_name = file_name.unwrap_or_else(|| "asset.bin".into());
    let content_type = content_type.unwrap_or_else(|| "application/octet-stream".into());
    let object_key = format!("{id}/assets/{asset_id}");
    let digest = crate::store::put_verified(state.store.as_ref(), &object_key, &bytes).await?;
    let now = chrono::Utc::now();
    let asset = db::ArtifactAsset {
        artifact_id: id.clone(),
        asset_id: asset_id.clone(),
        object_key: object_key.clone(),
        content_type,
        file_name,
        checksum: digest.checksum.clone(),
        size: i64::try_from(digest.size).map_err(|_| AppError::BadRequest("资产过大".into()))?,
        ref_count: 0,
        created_at: now,
        updated_at: now,
    };
    if let Err(error) = db::insert_artifact_asset(&state.pool, &asset).await {
        let _ = state.store.delete(&object_key).await;
        return Err(error.into());
    }
    if let Err(error) = db::register_blob_integrity(
        &state.pool,
        &id,
        &object_key,
        "asset",
        &digest.checksum,
        asset.size,
    )
    .await
    {
        let _ = state.store.delete(&object_key).await;
        return Err(error.into());
    }
    Ok((StatusCode::CREATED, Json(asset)).into_response())
}

pub async fn list_assets(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<AssetList>, AppError> {
    owned_meta(&state, &user, &id).await?;
    Ok(Json(AssetList {
        assets: db::list_artifact_assets(&state.pool, &id).await?,
    }))
}

pub async fn get_asset(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, asset_id)): Path<(String, String)>,
) -> Result<Response, AppError> {
    owned_meta(&state, &user, &id).await?;
    let asset = db::get_artifact_asset(&state.pool, &id, &asset_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("资产 {asset_id} 不存在")))?;
    let bytes =
        crate::store::get_verified(state.store.as_ref(), &asset.object_key, &asset.checksum)
            .await?;
    Ok((
        [
            (header::CONTENT_TYPE, asset.content_type),
            (header::CONTENT_LENGTH, bytes.len().to_string()),
            (
                header::CONTENT_DISPOSITION,
                document_support::download_content_disposition(&asset.file_name, "bin"),
            ),
        ],
        bytes,
    )
        .into_response())
}

pub async fn delete_asset(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, asset_id)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    owned_meta(&state, &user, &id).await?;
    let asset = db::get_artifact_asset(&state.pool, &id, &asset_id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("资产 {asset_id} 不存在")))?;
    if asset.ref_count > 0 {
        return Err(AppError::BadRequest(
            "资产仍被 snapshot 引用，不能删除".into(),
        ));
    }
    if !db::delete_artifact_asset(&state.pool, &id, &asset_id).await? {
        return Err(AppError::BadRequest("资产删除竞争失败，请重试".into()));
    }
    state.store.delete(&asset.object_key).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportResponse {
    #[serde(flatten)]
    pub artifact: ArtifactMeta,
    pub warnings: Vec<String>,
}

/// `GET /api/artifacts/{id}`.
pub async fn meta(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<ArtifactMeta>, AppError> {
    Ok(Json(owned_meta(&state, &user, &id).await?))
}

/// `PATCH /api/artifacts/{id}`.
pub async fn patch(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
    Json(request): Json<document_support::PatchRequest>,
) -> Result<Json<ArtifactMeta>, AppError> {
    let mut meta = owned_meta(&state, &user, &id).await?;
    if let Some(title) = request.title {
        let title = title.trim();
        if title.is_empty() {
            return Err(AppError::BadRequest("标题不能为空".into()));
        }
        meta = db::rename_artifact(&state.pool, &id, title).await?;
    }
    if let Some(starred) = request.starred {
        meta = db::set_artifact_starred(&state.pool, &id, starred).await?;
    }
    Ok(Json(meta))
}

/// `DELETE /api/artifacts/{id}`.
pub async fn delete(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    owned_meta(&state, &user, &id).await?;
    if let Some(blobs) = db::get_artifact_blob_keys(&state.pool, &id).await? {
        state.store.delete(&blobs.source_key).await?;
        let mut snapshot_keys = db::list_artifact_snapshot_keys(&state.pool, &id).await?;
        if !snapshot_keys.iter().any(|key| key == &blobs.snapshot_key) {
            snapshot_keys.push(blobs.snapshot_key);
        }
        for key in snapshot_keys {
            state.store.delete(&key).await?;
        }
    }
    db::delete_artifact(&state.pool, &id).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/artifacts/{id}/snapshot`.
pub async fn snapshot(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<SnapshotEnvelope>, AppError> {
    let (meta, artifact) = load_artifact(&state, &user, &id).await?;
    if artifact.revision != u64::try_from(meta.version).unwrap_or_default() {
        return Err(AppError::Internal("Artifact 与元数据版本不一致".into()));
    }
    Ok(Json(SnapshotEnvelope {
        protocol_version: CURRENT_PROTOCOL_VERSION,
        artifact,
    }))
}

/// Document engine writes are deliberately restricted by kind. Other engines
/// must expose their own command interpreter before this route accepts writes.
pub async fn put_snapshot(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<document_support::TransactionCommit>, AppError> {
    ensure_document(&state, &user, &id).await?;
    document_support::put_artifact(State(state), user, Path(id), headers, body).await
}

/// `GET /api/artifacts/{id}/revisions`.
pub async fn revisions(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<Vec<document_support::SnapshotMeta>>, AppError> {
    document_support::list_snapshots(State(state), user, Path(id)).await
}

/// `GET /api/artifacts/{id}/revisions/{version}`.
pub async fn revision(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, version)): Path<(String, i64)>,
) -> Result<Json<SnapshotEnvelope>, AppError> {
    document_support::get_snapshot(State(state), user, Path((id, version))).await
}

/// `POST /api/artifacts/{id}/revisions/{version}/restore`.
pub async fn restore_revision(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, version)): Path<(String, i64)>,
    headers: HeaderMap,
) -> Result<Json<document_support::TransactionCommit>, AppError> {
    ensure_document(&state, &user, &id).await?;
    document_support::restore_snapshot(State(state), user, Path((id, version)), headers).await
}

/// `GET /api/artifacts/{id}/history`.
pub async fn history(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<document_support::ArtifactHistoryState>, AppError> {
    document_support::history_state(State(state), user, Path(id)).await
}

/// `POST /api/artifacts/{id}/transactions`.
pub async fn transactions(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<document_support::TransactionCommit>, AppError> {
    let meta = owned_meta(&state, &user, &id).await?;
    match meta.kind {
        ArtifactKind::Document => {
            document_support::submit_transaction(State(state), user, Path(id), headers, body).await
        }
        ArtifactKind::Presentation => {
            crate::presentation_support::submit_transaction(
                State(state),
                user,
                Path(id),
                headers,
                body,
            )
            .await
        }
        kind => Err(AppError::UnsupportedArtifact(kind)),
    }
}

/// `GET /api/artifacts/{id}/source` returns the immutable uploaded source.
pub async fn source(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let meta = owned_meta(&state, &user, &id).await?;
    let blobs = db::get_artifact_blob_keys(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Artifact {id} 不存在")))?;
    let bytes = state.store.get(&blobs.source_key).await?;
    Ok((
        [
            (
                header::CONTENT_TYPE,
                source_content_type(meta.kind).to_string(),
            ),
            (
                header::CONTENT_DISPOSITION,
                document_support::download_content_disposition(
                    &meta.title,
                    source_extension(meta.kind),
                ),
            ),
        ],
        bytes,
    )
        .into_response())
}

/// `GET /api/artifacts/{id}/export/{format}` dispatches to the matching adapter.
pub async fn export(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, format)): Path<(String, String)>,
) -> Result<Response, AppError> {
    let (meta, artifact) = load_artifact(&state, &user, &id).await?;
    let expected = export_format(meta.kind);
    if format != expected {
        return Err(AppError::BadRequest(format!(
            "Artifact 类型 {:?} 只能导出为 .{}",
            meta.kind, expected
        )));
    }
    let bytes = match artifact.payload {
        ArtifactPayload::Document(model) => {
            let assets = docx_assets_for_export(&state, &id, &model).await?;
            oo_docx::write_docx_with_assets(&model, &assets)
                .map_err(|error| AppError::Internal(format!("生成 DOCX 失败：{error}")))?
        }
        ArtifactPayload::Spreadsheet(model) => oo_xlsx::write_xlsx(&model)
            .map_err(|error| AppError::UnsupportedCapability(format!("XLSX 导出失败：{error}")))?,
        ArtifactPayload::Presentation(model) => {
            let assets = pptx_assets_for_export(&state, &id, &model).await?;
            let exported = oo_pptx::write_pptx_with_assets(&model, &assets).map_err(|error| {
                AppError::UnsupportedCapability(format!("PPTX 导出失败：{error}"))
            })?;
            if !exported.loss_report.unsupported.is_empty() {
                return Err(AppError::UnsupportedCapability(
                    "PPTX 导出包含尚不支持的内容，拒绝静默丢失数据".into(),
                ));
            }
            exported.bytes
        }
        ArtifactPayload::Mindmap(_) | ArtifactPayload::Whiteboard(_) => {
            return Err(AppError::UnsupportedCapability(
                "该 Artifact 尚未定义可交换文件格式".into(),
            ))
        }
    };
    Ok((
        [
            (
                header::CONTENT_TYPE,
                export_content_type(meta.kind).to_string(),
            ),
            (
                header::CONTENT_DISPOSITION,
                document_support::download_content_disposition(&meta.title, expected),
            ),
        ],
        bytes,
    )
        .into_response())
}

async fn ensure_document(state: &AppState, user: &CurrentUser, id: &str) -> Result<(), AppError> {
    let meta = owned_meta(state, user, id).await?;
    if meta.kind != ArtifactKind::Document {
        return Err(AppError::UnsupportedArtifact(meta.kind));
    }
    Ok(())
}

pub(crate) async fn owned_meta(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
) -> Result<ArtifactMeta, AppError> {
    let meta = db::get_artifact(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Artifact {id} 不存在")))?;
    if meta.owner_id != user.id {
        return Err(AppError::Forbidden);
    }
    Ok(meta)
}

pub(crate) async fn load_artifact(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
) -> Result<(ArtifactMeta, ArtifactEnvelope), AppError> {
    let meta = owned_meta(state, user, id).await?;
    let blobs = db::get_artifact_blob_keys(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Artifact {id} 不存在")))?;
    let bytes = state.store.get(&blobs.snapshot_key).await?;
    let artifact: ArtifactEnvelope = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::Internal(format!("已保存的 Artifact JSON 无效：{error}")))?;
    artifact
        .validate()
        .map_err(|error| AppError::Internal(format!("已保存的 Artifact 无效：{error}")))?;
    if artifact.artifact_id != id || artifact.kind != meta.kind {
        return Err(AppError::Internal(
            "Artifact id 或 kind 与元数据不一致".into(),
        ));
    }
    Ok((meta, artifact))
}

fn empty_payload(kind: ArtifactKind) -> ArtifactPayload {
    match kind {
        ArtifactKind::Document => ArtifactPayload::Document(DocumentModel::empty()),
        ArtifactKind::Spreadsheet => ArtifactPayload::Spreadsheet(Default::default()),
        ArtifactKind::Presentation => ArtifactPayload::Presentation(Default::default()),
        ArtifactKind::Mindmap => ArtifactPayload::Mindmap(Default::default()),
        ArtifactKind::Whiteboard => ArtifactPayload::Whiteboard(Default::default()),
    }
}

pub(crate) fn artifact_for(
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

fn import_kind(file_name: &str) -> Result<ArtifactKind, AppError> {
    match file_name
        .rsplit('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "docx" => Ok(ArtifactKind::Document),
        "xlsx" => Ok(ArtifactKind::Spreadsheet),
        "pptx" => Ok(ArtifactKind::Presentation),
        extension => Err(AppError::BadRequest(format!(
            "不支持的导入格式：.{extension}，仅支持 .docx、.xlsx、.pptx"
        ))),
    }
}

fn title_from_file_name(file_name: &str, kind: ArtifactKind) -> String {
    let stem = file_name.rsplit('/').next().unwrap_or(file_name);
    let stem = stem.rsplit_once('.').map_or(stem, |(stem, _)| stem);
    if stem.trim().is_empty() {
        default_title(kind).to_string()
    } else {
        stem.trim().to_string()
    }
}

fn default_title(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Document => "未命名文档",
        ArtifactKind::Spreadsheet => "未命名表格",
        ArtifactKind::Presentation => "未命名演示文稿",
        ArtifactKind::Mindmap => "未命名思维导图",
        ArtifactKind::Whiteboard => "未命名白板",
    }
}

fn source_extension(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Document => "docx",
        ArtifactKind::Spreadsheet => "xlsx",
        ArtifactKind::Presentation => "pptx",
        ArtifactKind::Mindmap => "json",
        ArtifactKind::Whiteboard => "json",
    }
}

fn export_format(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Document => "docx",
        ArtifactKind::Spreadsheet => "xlsx",
        ArtifactKind::Presentation => "pptx",
        ArtifactKind::Mindmap | ArtifactKind::Whiteboard => "json",
    }
}

fn source_content_type(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Document => {
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document"
        }
        ArtifactKind::Spreadsheet => {
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet"
        }
        ArtifactKind::Presentation => {
            "application/vnd.openxmlformats-officedocument.presentationml.presentation"
        }
        ArtifactKind::Mindmap | ArtifactKind::Whiteboard => "application/json",
    }
}

fn export_content_type(kind: ArtifactKind) -> &'static str {
    source_content_type(kind)
}

async fn register_blob_integrity(
    state: &AppState,
    artifact_id: &str,
    object_key: &str,
    object_kind: &str,
    bytes: &[u8],
) -> Result<(), AppError> {
    let checksum = crate::store::checksum(bytes);
    db::register_blob_integrity(
        &state.pool,
        artifact_id,
        object_key,
        object_kind,
        &checksum,
        i64::try_from(bytes.len()).map_err(|_| AppError::Internal("Blob 过大".into()))?,
    )
    .await
    .map_err(AppError::from)
}

fn created_event(id: &str, kind: ArtifactKind) -> DomainEventRecord {
    DomainEventRecord {
        event_id: format!("artifact:create:{id}:1:0"),
        type_id: "artifact.created".into(),
        payload: serde_json::json!({
            "artifactId": id,
            "artifactKind": serde_json::to_value(kind).unwrap_or_default(),
            "revision": 1,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        document_image_asset_references, document_image_render_asset_ids,
        require_lossless_pptx_import, require_presentation_asset_matches_store,
    };
    use crate::{db, error::AppError};
    use oo_pptx::{PptxLossReport, PptxReportKind, PptxUnsupported};
    use oo_schema::presentation_v5::AssetRef;
    use oo_schema::{
        BlockData, BlockPresentation, DocumentBlock, DocumentBlockKind, DocumentModel, ImageBlock,
        ImageTransform,
    };

    #[test]
    fn compressed_image_keeps_original_asset_referenced_but_exports_only_active_image() {
        let document = DocumentModel {
            root: vec!["image-1".into()],
            blocks: vec![DocumentBlock {
                id: "image-1".into(),
                kind: DocumentBlockKind::Image,
                presentation: BlockPresentation::default(),
                content: None,
                children: Vec::new(),
                data: BlockData::Image(ImageBlock {
                    asset_id: "asset-compressed".into(),
                    original_asset_id: Some("asset-original".into()),
                    alt: String::new(),
                    transform: ImageTransform::default(),
                    caption: String::new(),
                }),
            }],
            page_setup: None,
        };

        assert_eq!(
            document_image_asset_references(&document),
            ["asset-compressed", "asset-original"]
        );
        assert_eq!(
            document_image_render_asset_ids(&document),
            ["asset-compressed"]
        );
    }

    #[test]
    fn lossy_pptx_import_is_rejected_with_machine_relevant_capability_detail() {
        let report = PptxLossReport {
            unsupported: vec![PptxUnsupported {
                kind: PptxReportKind::Unsupported,
                capability: "chart".into(),
                part: "ppt/slides/slide1.xml".into(),
                detail: "chart relationship 没有 typed Deck 映射".into(),
                suggestion: Some("保留原始 PPTX source asset".into()),
            }],
        };

        let error = require_lossless_pptx_import(&report).expect_err("lossy import must fail");
        assert!(matches!(error, AppError::UnsupportedCapability(_)));
        assert!(error.to_string().contains("chart (ppt/slides/slide1.xml)"));
    }

    #[test]
    fn lossless_pptx_import_is_accepted() {
        require_lossless_pptx_import(&PptxLossReport::default())
            .expect("lossless PPTX should reach canonical asset persistence");
    }

    fn stored_asset(checksum: &str, content_type: &str) -> db::ArtifactAsset {
        let now = chrono::Utc::now();
        db::ArtifactAsset {
            artifact_id: "presentation-1".into(),
            asset_id: "asset-1".into(),
            object_key: "presentation-1/assets/asset-1".into(),
            content_type: content_type.into(),
            file_name: "asset-1.png".into(),
            checksum: checksum.into(),
            size: 1,
            ref_count: 1,
            created_at: now,
            updated_at: now,
        }
    }

    #[test]
    fn presentation_export_rejects_asset_metadata_that_does_not_match_storage() {
        let reference = AssetRef {
            asset_id: "asset-1".into(),
            digest: "expected-digest".into(),
            mime_type: "image/png".into(),
            width: None,
            height: None,
            original_asset_id: None,
        };
        let error = require_presentation_asset_matches_store(
            &reference,
            &stored_asset("actual-digest", "image/png"),
        )
        .expect_err("digest mismatch must stop export");
        assert!(error.to_string().contains("摘要与已验证存储不一致"));

        let error = require_presentation_asset_matches_store(
            &reference,
            &stored_asset("expected-digest", "image/jpeg"),
        )
        .expect_err("MIME mismatch must stop export");
        assert!(error.to_string().contains("MIME 类型与已验证存储不一致"));
    }
}
