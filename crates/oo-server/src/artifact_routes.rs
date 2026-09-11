//! Canonical, polymorphic Artifact REST boundary.
//!
//! Resource names live here instead of being duplicated per editor product:
//! metadata, snapshots, revisions, transactions, source and exports all use
//! the same `/api/artifacts/{id}` hierarchy.  A route selects an engine from
//! the persisted `ArtifactKind`; it never infers a kind from a legacy path or
//! silently downgrades a spreadsheet/presentation to a document.

use axum::extract::{Multipart, Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use crate::auth::CurrentUser;
use crate::db::{self, ArtifactKind, ArtifactMeta, NewArtifact};
use crate::document_support;
use crate::error::AppError;
use crate::AppState;
use oo_document::document_command_registry;
use oo_mindmap::{
    export_pdf, export_svg, mindmap_command_registry, parse_mindmap_exchange,
    write_mindmap_exchange, MindmapExchangeAsset, MindmapExchangeEnvelope, MindmapPdfMode,
    MindmapPdfOptions, MindmapPdfOrientation, MindmapPdfPaper,
};
use oo_presentation::presentation_command_registry;
use oo_protocol::{
    ArtifactCapability, ArtifactCapabilityStatus, ArtifactCommandCapability,
    ArtifactFeatureCapabilities, ArtifactTransportCapability, CapabilityCatalog, DomainEventRecord,
    SnapshotEnvelope, CAPABILITY_CONTRACT_VERSION, CURRENT_PROTOCOL_VERSION,
};
use oo_schema::{
    ArtifactEnvelope, ArtifactPayload, AssetReference, AssetReferenceSource, BlockData,
    DocumentModel, SheetMetadata, SheetModel, SpreadsheetMetadata, SpreadsheetModel,
};
use oo_spreadsheet::spreadsheet_command_registry;
use oo_whiteboard::whiteboard_command_registry;

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
pub const ARTIFACT_COLLABORATORS_PATH: &str = "/api/artifacts/{id}/collaborators";
pub const ARTIFACT_COLLABORATOR_PATH: &str = "/api/artifacts/{id}/collaborators/{userId}";
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
                features: ArtifactFeatureCapabilities {
                    edit: ArtifactCapabilityStatus::Stable,
                    history: ArtifactCapabilityStatus::Stable,
                    projection: ArtifactCapabilityStatus::Stable,
                    import: ArtifactCapabilityStatus::Stable,
                    export: ArtifactCapabilityStatus::Preview,
                    assets: ArtifactCapabilityStatus::Stable,
                    presence: ArtifactCapabilityStatus::Preview,
                },
                commands: document_command_capabilities(),
            },
            ArtifactCapability {
                kind: ArtifactKind::Spreadsheet,
                namespace: "spreadsheet".into(),
                features: ArtifactFeatureCapabilities {
                    edit: ArtifactCapabilityStatus::Stable,
                    history: ArtifactCapabilityStatus::Stable,
                    projection: ArtifactCapabilityStatus::Stable,
                    import: ArtifactCapabilityStatus::Preview,
                    export: ArtifactCapabilityStatus::Preview,
                    assets: ArtifactCapabilityStatus::Planned,
                    presence: ArtifactCapabilityStatus::Planned,
                },
                commands: spreadsheet_command_capabilities(),
            },
            ArtifactCapability {
                kind: ArtifactKind::Presentation,
                namespace: "presentation".into(),
                features: ArtifactFeatureCapabilities {
                    edit: ArtifactCapabilityStatus::Stable,
                    history: ArtifactCapabilityStatus::Stable,
                    projection: ArtifactCapabilityStatus::Stable,
                    import: ArtifactCapabilityStatus::Preview,
                    export: ArtifactCapabilityStatus::Preview,
                    assets: ArtifactCapabilityStatus::Stable,
                    presence: ArtifactCapabilityStatus::Preview,
                },
                commands: presentation_command_capabilities(),
            },
            ArtifactCapability {
                kind: ArtifactKind::Mindmap,
                namespace: "mindmap".into(),
                features: ArtifactFeatureCapabilities {
                    edit: ArtifactCapabilityStatus::Stable,
                    history: ArtifactCapabilityStatus::Stable,
                    projection: ArtifactCapabilityStatus::Stable,
                    import: ArtifactCapabilityStatus::Preview,
                    export: ArtifactCapabilityStatus::Preview,
                    assets: ArtifactCapabilityStatus::Stable,
                    presence: ArtifactCapabilityStatus::Preview,
                },
                commands: mindmap_command_capabilities(),
            },
            ArtifactCapability {
                kind: ArtifactKind::Whiteboard,
                namespace: "whiteboard".into(),
                features: ArtifactFeatureCapabilities {
                    edit: ArtifactCapabilityStatus::Preview,
                    history: ArtifactCapabilityStatus::Planned,
                    projection: ArtifactCapabilityStatus::Stable,
                    import: ArtifactCapabilityStatus::Unsupported,
                    export: ArtifactCapabilityStatus::Unsupported,
                    assets: ArtifactCapabilityStatus::Planned,
                    presence: ArtifactCapabilityStatus::Planned,
                },
                commands: whiteboard_command_capabilities(),
            },
        ],
    })
}

fn spreadsheet_command_capabilities() -> Vec<ArtifactCommandCapability> {
    // M0 单一事实源：目录由引擎的 spreadsheet_command_registry 派生，
    // 注册表测试保证每个条目都能反序列化为真实命令，杜绝 catalog 与
    // engine 命令面漂移。
    let mut commands = spreadsheet_command_registry()
        .into_iter()
        .map(|descriptor| ArtifactCommandCapability {
            type_id: descriptor.type_id.into(),
            scope: descriptor.scope.into(),
            requires_revision: true,
            supports_idempotency: true,
        })
        .collect::<Vec<_>>();
    // history 是 intent-only 命令（由 durable 命令日志重放实现），不属于
    // 引擎变体，但客户端必须能从 catalog 发现撤销/重做入口。
    commands.push(ArtifactCommandCapability {
        type_id: "spreadsheet.history".into(),
        scope: "spreadsheet.sheet".into(),
        requires_revision: true,
        supports_idempotency: true,
    });
    commands
}

fn whiteboard_command_capabilities() -> Vec<ArtifactCommandCapability> {
    whiteboard_command_registry()
        .iter()
        .map(|descriptor| ArtifactCommandCapability {
            type_id: descriptor.type_id.into(),
            scope: descriptor.scope.into(),
            requires_revision: true,
            supports_idempotency: true,
        })
        .collect()
}

fn mindmap_command_capabilities() -> Vec<ArtifactCommandCapability> {
    let mut commands = mindmap_command_registry()
        .into_iter()
        .map(|descriptor| ArtifactCommandCapability {
            type_id: descriptor.type_id.into(),
            scope: descriptor.scope.into(),
            requires_revision: true,
            supports_idempotency: true,
        })
        .collect::<Vec<_>>();
    // History is an intent resolved from the durable transaction journal,
    // rather than an editable graph command.
    commands.push(ArtifactCommandCapability {
        type_id: "mindmap.history".into(),
        scope: "mindmap.graph".into(),
        requires_revision: true,
        supports_idempotency: true,
    });
    commands
}

fn document_command_capabilities() -> Vec<ArtifactCommandCapability> {
    let mut commands = document_command_registry()
        .iter()
        .map(|descriptor| ArtifactCommandCapability {
            type_id: descriptor.type_id.into(),
            scope: descriptor.scope.into(),
            requires_revision: true,
            supports_idempotency: true,
        })
        .collect::<Vec<_>>();
    commands.push(ArtifactCommandCapability {
        type_id: "document.history".into(),
        scope: "document.history".into(),
        requires_revision: true,
        supports_idempotency: true,
    });
    commands
}

/// The capability catalog intentionally mirrors only concrete variants in
/// `PresentationCommand` plus the server-authoritative history intent. In
/// particular it does not advertise renderer gestures or generic patches.
fn presentation_command_capabilities() -> Vec<ArtifactCommandCapability> {
    let mut commands = presentation_command_registry()
        .iter()
        .map(|descriptor| ArtifactCommandCapability {
            type_id: descriptor.type_id.into(),
            scope: descriptor.scope.into(),
            requires_revision: true,
            supports_idempotency: true,
        })
        .collect::<Vec<_>>();
    commands.push(ArtifactCommandCapability {
        type_id: "presentation.history".into(),
        scope: "presentation.history".into(),
        requires_revision: true,
        supports_idempotency: true,
    });
    commands
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactList {
    pub artifacts: Vec<ArtifactMeta>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UpsertCollaboratorRequest {
    pub role: String,
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
    let events = [created_event(&id, request.kind, &user.id)];
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

/// `POST /api/artifacts/import` imports Office files plus Mindmap exchange formats.
pub async fn import(
    State(state): State<AppState>,
    user: CurrentUser,
    mut multipart: Multipart,
) -> Result<Response, AppError> {
    let mut file_name = None;
    let mut bytes = None;
    let mut import_mode = ImportMode::Audit;
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
        } else if field.name() == Some("mode") {
            let value = field
                .text()
                .await
                .map_err(|error| AppError::BadRequest(format!("读取导入模式失败：{error}")))?;
            import_mode = match value.as_str() {
                "audit" => ImportMode::Audit,
                "strict" => ImportMode::Strict,
                _ => {
                    return Err(AppError::BadRequest(
                        "导入 mode 必须是 audit 或 strict".into(),
                    ))
                }
            };
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
            let warnings = imported
                .loss_report
                .unsupported
                .iter()
                .map(|loss| format!("{}:{} — {}", loss.capability, loss.count, loss.detail))
                .collect();
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
            (ArtifactPayload::Document(imported.document), warnings)
        }
        ArtifactKind::Spreadsheet => {
            let result = oo_xlsx::read_xlsx_with_report(&bytes)
                .map_err(|error| AppError::BadRequest(format!("XLSX 解析失败：{error}")))?;
            let warnings = result
                .unsupported_parts
                .iter()
                .map(|loss| format!("{} — {}", loss.part, loss.reason))
                .collect();
            (ArtifactPayload::Spreadsheet(result.model), warnings)
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
        ArtifactKind::Mindmap => {
            let imported = import_mindmap(&file_name, &bytes)?;
            imported_assets = imported.assets;
            (ArtifactPayload::Mindmap(imported.model), imported.warnings)
        }
        ArtifactKind::Whiteboard => {
            return Err(AppError::UnsupportedCapability(format!(
                "暂不支持导入 {kind:?} 文件"
            )))
        }
    };
    let artifact = artifact_for(&id, 1, payload)?;
    if import_mode == ImportMode::Strict && !warnings.is_empty() {
        return Err(AppError::UnsupportedCapability(format!(
            "strict 导入拒绝有损内容：{}",
            warnings.join("；")
        )));
    }
    let snapshot = serde_json::to_vec(&artifact)
        .map_err(|error| AppError::Internal(format!("序列化 Artifact 失败：{error}")))?;
    let source_key = format!("{id}/source.{}", import_source_extension(&file_name, kind));
    let snapshot_key = document_support::artifact_snapshot_key(&id, 1);
    state.store.put(&source_key, &bytes).await?;
    state.store.put(&snapshot_key, &snapshot).await?;
    let title = title_from_file_name(&file_name, kind);
    let events = [created_event(&id, kind, &user.id)];
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
            &artifact.payload.asset_references(),
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImportMode {
    Audit,
    Strict,
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

/// Canonical Mindmap JSON is a portable exchange package rather than a copy
/// of the persisted Artifact envelope. It embeds only the verified image
/// closure and deliberately excludes artifact id, revision and view state.
async fn mindmap_exchange_for_export(
    state: &AppState,
    artifact_id: &str,
    model: &oo_schema::MindmapModel,
) -> Result<Vec<u8>, AppError> {
    let assets = mindmap_assets_for_export(state, artifact_id, model).await?;
    write_mindmap_exchange(&MindmapExchangeEnvelope::new(model.clone(), assets)).map_err(|error| {
        AppError::UnsupportedCapability(format!("生成 Mindmap JSON 失败：{error}"))
    })
}

async fn mindmap_assets_for_export(
    state: &AppState,
    artifact_id: &str,
    model: &oo_schema::MindmapModel,
) -> Result<Vec<MindmapExchangeAsset>, AppError> {
    let mut assets = Vec::new();
    for reference in model.asset_references() {
        let stored = db::get_artifact_asset(&state.pool, artifact_id, &reference.asset_id)
            .await?
            .ok_or_else(|| {
                AppError::Internal(format!(
                    "Mindmap 引用了不存在的图片资产 {artifact_id}/{}",
                    reference.asset_id
                ))
            })?;
        let bytes =
            crate::store::get_verified(state.store.as_ref(), &stored.object_key, &stored.checksum)
                .await
                .map_err(|error| {
                    AppError::Internal(format!(
                        "读取 Mindmap 图片资产 {} 失败：{error}",
                        reference.asset_id
                    ))
                })?;
        assets.push(MindmapExchangeAsset {
            asset_id: stored.asset_id,
            file_name: safe_mindmap_asset_file_name(&stored.file_name, &reference.asset_id),
            content_type: stored.content_type,
            checksum: stored.checksum,
            data_base64: base64::engine::general_purpose::STANDARD.encode(bytes),
        });
    }
    Ok(assets)
}

fn safe_mindmap_asset_file_name(file_name: &str, asset_id: &str) -> String {
    let candidate = file_name
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default()
        .trim();
    if candidate.is_empty() || candidate == "." || candidate == ".." || candidate.len() > 255 {
        format!("{asset_id}.bin")
    } else {
        candidate.to_string()
    }
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
    references: &[AssetReference],
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
    authorize(&state, &user, &id, Role::Editor).await?;
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
    authorize(&state, &user, &id, Role::Viewer).await?;
    Ok(Json(AssetList {
        assets: db::list_artifact_assets(&state.pool, &id).await?,
    }))
}

pub async fn get_asset(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, asset_id)): Path<(String, String)>,
) -> Result<Response, AppError> {
    authorize(&state, &user, &id, Role::Viewer).await?;
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
    authorize(&state, &user, &id, Role::Editor).await?;
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
    // 事务是写路径：editor 及以上（C4 授权分层）；owner 之外由协作者角色决定。
    let meta = authorize(&state, &user, &id, Role::Editor).await?;
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
        ArtifactKind::Mindmap => {
            crate::mindmap_support::submit_transaction(State(state), user, Path(id), headers, body)
                .await
        }
        ArtifactKind::Spreadsheet => {
            crate::spreadsheet_support::submit_transaction(
                State(state),
                user,
                Path(id),
                headers,
                body,
            )
            .await
        }
        ArtifactKind::Whiteboard => {
            crate::whiteboard_support::submit_transaction(
                State(state),
                user,
                Path(id),
                headers,
                body,
            )
            .await
        }
    }
}

/// `GET /api/artifacts/{id}/source` returns the immutable uploaded source.
pub async fn source(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let meta = authorize(&state, &user, &id, Role::Viewer).await?;
    let blobs = db::get_artifact_blob_keys(&state.pool, &id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Artifact {id} 不存在")))?;
    let bytes = state.store.get(&blobs.source_key).await?;
    let source_extension = source_extension_from_key(&blobs.source_key, meta.kind);
    Ok((
        [
            (
                header::CONTENT_TYPE,
                source_content_type_for_extension(source_extension, meta.kind).to_string(),
            ),
            (
                header::CONTENT_DISPOSITION,
                document_support::download_content_disposition(&meta.title, source_extension),
            ),
        ],
        bytes,
    )
        .into_response())
}

/// `GET /api/artifacts/{id}/export/{format}` dispatches to the matching adapter.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ArtifactExportQuery {
    paper: Option<String>,
    orientation: Option<String>,
    mode: Option<String>,
    margin: Option<f32>,
}

pub async fn export(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, format)): Path<(String, String)>,
    Query(query): Query<ArtifactExportQuery>,
) -> Result<Response, AppError> {
    let (meta, artifact) = load_artifact(&state, &user, &id).await?;
    let expected = export_format(meta.kind);
    let allowed = format == expected
        || (meta.kind == ArtifactKind::Mindmap && matches!(format.as_str(), "md" | "svg" | "pdf"));
    if !allowed {
        return Err(AppError::BadRequest(format!(
            "Artifact 类型 {:?} 不支持导出为 .{}",
            meta.kind, format
        )));
    }
    let mut loss_header: Option<(&'static str, String)> = None;
    let bytes = match &artifact.payload {
        ArtifactPayload::Document(model) => {
            let assets = docx_assets_for_export(&state, &id, model).await?;
            // DOCX keeps all text content; the report enumerates semantic
            // approximations (todo state, link targets, containers) so clients
            // can surface them instead of discovering silent data loss.
            let exported = oo_docx::write_docx_with_report(model, &assets)
                .map_err(|error| AppError::Internal(format!("生成 DOCX 失败：{error}")))?;
            if let Some(summary) = exported.loss_report.header_summary() {
                loss_header = Some(("x-docx-losses", summary));
            }
            exported.bytes
        }
        ArtifactPayload::Spreadsheet(model) => oo_xlsx::write_xlsx(model)
            .map_err(|error| AppError::UnsupportedCapability(format!("XLSX 导出失败：{error}")))?,
        ArtifactPayload::Presentation(model) => {
            let assets = pptx_assets_for_export(&state, &id, model).await?;
            let exported = oo_pptx::write_pptx_with_assets(model, &assets).map_err(|error| {
                AppError::UnsupportedCapability(format!("PPTX 导出失败：{error}"))
            })?;
            if !exported.loss_report.unsupported.is_empty() {
                let details = exported
                    .loss_report
                    .unsupported
                    .iter()
                    .take(4)
                    .map(|item| format!("{}（{}）：{}", item.capability, item.part, item.detail))
                    .collect::<Vec<_>>()
                    .join("；");
                return Err(AppError::UnsupportedCapability(format!(
                    "PPTX 导出包含尚不支持的内容，拒绝静默丢失数据：{details}"
                )));
            }
            exported.bytes
        }
        ArtifactPayload::Mindmap(model) => match format.as_str() {
            "json" => mindmap_exchange_for_export(&state, &id, model).await?,
            "md" => {
                let exported = oo_mindmap::export_markdown_with_report(model).map_err(|error| {
                    AppError::Internal(format!("生成 Mindmap Markdown 失败：{error}"))
                })?;
                if let Some(summary) = exported.loss_report.header_summary() {
                    loss_header = Some(("x-mindmap-losses", summary));
                }
                exported.text.into_bytes()
            }
            "svg" => {
                let assets = mindmap_assets_for_export(&state, &id, model).await?;
                let exported = export_svg(model, &assets).map_err(|error| {
                    AppError::Internal(format!("生成 Mindmap SVG 失败：{error}"))
                })?;
                if let Some(summary) = exported.loss_report.header_summary() {
                    loss_header = Some(("x-mindmap-losses", summary));
                }
                exported.text.into_bytes()
            }
            "pdf" => {
                let assets = mindmap_assets_for_export(&state, &id, model).await?;
                let svg = export_svg(model, &assets).map_err(|error| {
                    AppError::Internal(format!("生成 Mindmap PDF 投影失败：{error}"))
                })?;
                let exported =
                    export_pdf(&svg.text, mindmap_pdf_options(&query)?).map_err(|error| {
                        AppError::UnsupportedCapability(format!("生成 Mindmap PDF 失败：{error}"))
                    })?;
                let mut losses = svg.loss_report.unsupported;
                losses.extend(exported.loss_report.unsupported);
                let report = oo_mindmap::MindmapExchangeLossReport {
                    unsupported: losses,
                };
                if let Some(summary) = report.header_summary() {
                    loss_header = Some(("x-mindmap-losses", summary));
                }
                exported.bytes
            }
            _ => unreachable!("format was validated above"),
        },
        ArtifactPayload::Whiteboard(_) => {
            return Err(AppError::UnsupportedCapability(
                "该 Artifact 尚未定义可交换文件格式".into(),
            ))
        }
    };
    let mut headers = vec![
        (
            header::CONTENT_TYPE,
            HeaderValue::from_str(match (meta.kind, format.as_str()) {
                (ArtifactKind::Mindmap, "md") => "text/markdown; charset=utf-8",
                (ArtifactKind::Mindmap, "json") => "application/vnd.open-office.mindmap+json",
                (ArtifactKind::Mindmap, "svg") => "image/svg+xml; charset=utf-8",
                (ArtifactKind::Mindmap, "pdf") => "application/pdf",
                _ => export_content_type(meta.kind),
            })
            .expect("export content type is valid header value"),
        ),
        (
            header::CONTENT_DISPOSITION,
            HeaderValue::from_str(&document_support::download_content_disposition(
                &meta.title,
                if meta.kind == ArtifactKind::Mindmap && format == "json" {
                    "mindmap.json"
                } else {
                    &format
                },
            ))
            .expect("content disposition is valid header value"),
        ),
    ];
    if let Some((name, value)) = loss_header {
        headers.push((
            axum::http::HeaderName::from_static(name),
            HeaderValue::from_str(&value).expect("loss summary is ASCII"),
        ));
    }
    let mut builder = Response::builder().status(StatusCode::OK);
    for (name, value) in headers {
        builder = builder.header(name, value);
    }
    builder
        .body(axum::body::Body::from(bytes))
        .map_err(|error| AppError::Internal(format!("导出响应构建失败：{error}")))
}

fn mindmap_pdf_options(query: &ArtifactExportQuery) -> Result<MindmapPdfOptions, AppError> {
    let paper = match query.paper.as_deref().unwrap_or("a4") {
        "a4" => MindmapPdfPaper::A4,
        "a3" => MindmapPdfPaper::A3,
        value => return Err(AppError::BadRequest(format!("PDF paper 不受支持：{value}"))),
    };
    let orientation = match query.orientation.as_deref().unwrap_or("landscape") {
        "portrait" => MindmapPdfOrientation::Portrait,
        "landscape" => MindmapPdfOrientation::Landscape,
        value => {
            return Err(AppError::BadRequest(format!(
                "PDF orientation 不受支持：{value}"
            )))
        }
    };
    let mode = match query.mode.as_deref().unwrap_or("fit") {
        "fit" => MindmapPdfMode::Fit,
        "tile" => MindmapPdfMode::Tile,
        value => return Err(AppError::BadRequest(format!("PDF mode 不受支持：{value}"))),
    };
    Ok(MindmapPdfOptions {
        paper,
        orientation,
        mode,
        margin_points: query.margin.unwrap_or(28.0),
    })
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
    authorize(state, user, id, Role::Owner).await
}

/// 委派角色层级。owner 即 `artifacts.owner_id`，editor/viewer 来自
/// `artifact_collaborators`；owner 拥有全部能力，不需要出现在表里。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Role {
    Viewer,
    Editor,
    Owner,
}

impl Role {
    fn from_str(role: &str) -> Option<Self> {
        match role {
            "viewer" => Some(Self::Viewer),
            "editor" => Some(Self::Editor),
            _ => None,
        }
    }
}

/// 统一授权入口：解析 artifact 元数据并断言调用者至少拥有 [`Role::min`]。
pub(crate) async fn authorize(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
    min: Role,
) -> Result<ArtifactMeta, AppError> {
    let meta = db::get_artifact(&state.pool, id)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("Artifact {id} 不存在")))?;
    let granted = if meta.owner_id == user.id {
        Some(Role::Owner)
    } else {
        db::list_collaborators(&state.pool, id)
            .await?
            .into_iter()
            .find(|collaborator| collaborator.user_id == user.id)
            .and_then(|collaborator| Role::from_str(&collaborator.role))
    };
    match granted {
        Some(role) if role >= min => Ok(meta),
        _ => Err(AppError::Forbidden),
    }
}

/// `GET /api/artifacts/{id}/collaborators`（editor 及以上）：viewer 不应能枚举
/// 同一 artifact 上的其他 Principal。
pub async fn list_collaborators(
    State(state): State<AppState>,
    user: CurrentUser,
    Path(id): Path<String>,
) -> Result<Json<db::CollaboratorList>, AppError> {
    authorize(&state, &user, &id, Role::Editor).await?;
    let collaborators = db::list_collaborators(&state.pool, &id).await?;
    Ok(Json(db::CollaboratorList { collaborators }))
}

/// `PUT /api/artifacts/{id}/collaborators/{userId}`（仅 owner）。
pub async fn upsert_collaborator(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, target_user)): Path<(String, String)>,
    Json(request): Json<UpsertCollaboratorRequest>,
) -> Result<StatusCode, AppError> {
    authorize(&state, &user, &id, Role::Owner).await?;
    if !db::COLLABORATOR_ROLES.contains(&request.role.as_str()) {
        return Err(AppError::BadRequest(format!(
            "角色必须是 {:?} 之一",
            db::COLLABORATOR_ROLES
        )));
    }
    db::upsert_collaborator(&state.pool, &id, &target_user, &request.role).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `DELETE /api/artifacts/{id}/collaborators/{userId}`（仅 owner）。
pub async fn delete_collaborator(
    State(state): State<AppState>,
    user: CurrentUser,
    Path((id, target_user)): Path<(String, String)>,
) -> Result<StatusCode, AppError> {
    authorize(&state, &user, &id, Role::Owner).await?;
    db::delete_collaborator(&state.pool, &id, &target_user).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub(crate) async fn load_artifact(
    state: &AppState,
    user: &CurrentUser,
    id: &str,
) -> Result<(ArtifactMeta, ArtifactEnvelope), AppError> {
    // 读路径（snapshot/events/export/source 与事务装载前的元数据检查）降至
    // viewer+；写与管理面在各自入口再收紧到 editor/owner。
    let meta = authorize(state, user, id, Role::Viewer).await?;
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
        ArtifactKind::Spreadsheet => ArtifactPayload::Spreadsheet(SpreadsheetModel {
            metadata: SpreadsheetMetadata {
                active_sheet_id: Some("sheet-1".into()),
                ..SpreadsheetMetadata::default()
            },
            sheets: vec![SheetModel {
                id: "sheet-1".into(),
                name: "Sheet 1".into(),
                cells: Vec::new(),
                metadata: SheetMetadata::default(),
            }],
        }),
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
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".mindmap.json")
        || lower.ends_with(".opmm")
        || lower.ends_with(".md")
        || lower.ends_with(".mm")
        || lower.ends_with(".xmind")
    {
        return Ok(ArtifactKind::Mindmap);
    }
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
        "json" => Ok(ArtifactKind::Mindmap),
        extension => Err(AppError::BadRequest(format!(
            "不支持的导入格式：.{extension}，仅支持 .docx、.xlsx、.pptx、.mindmap.json、.md、.mm、.xmind"
        ))),
    }
}

struct ImportedMindmap {
    model: oo_schema::MindmapModel,
    assets: Vec<ImportedBinaryAsset>,
    warnings: Vec<String>,
}

fn import_mindmap(file_name: &str, bytes: &[u8]) -> Result<ImportedMindmap, AppError> {
    let lower = file_name.to_ascii_lowercase();
    if lower.ends_with(".xmind") {
        let imported = oo_mindmap::import_xmind(bytes)
            .map_err(|error| AppError::BadRequest(format!("XMind 解析失败：{error}")))?;
        let mut model = imported.model;
        let assets = remap_external_mindmap_assets(&mut model, imported.assets)?;
        ArtifactEnvelope::new("xmind-import", ArtifactPayload::Mindmap(model.clone()))
            .validate()
            .map_err(|error| AppError::BadRequest(format!("XMind 导入结果无效：{error}")))?;
        return Ok(ImportedMindmap {
            model,
            assets,
            warnings: imported
                .loss_report
                .unsupported
                .into_iter()
                .map(|loss| format!("{}:{} — {}", loss.capability, loss.count, loss.detail))
                .collect(),
        });
    }
    if lower.ends_with(".mm") {
        let imported = oo_mindmap::import_freemind(bytes)
            .map_err(|error| AppError::BadRequest(format!("FreeMind 解析失败：{error}")))?;
        return Ok(ImportedMindmap {
            model: imported.model,
            assets: Vec::new(),
            warnings: imported
                .loss_report
                .unsupported
                .into_iter()
                .map(|loss| format!("{}:{} — {}", loss.capability, loss.count, loss.detail))
                .collect(),
        });
    }
    if lower.ends_with(".md") {
        let text = std::str::from_utf8(bytes).map_err(|error| {
            AppError::BadRequest(format!("Mindmap Markdown 不是 UTF-8：{error}"))
        })?;
        let model = oo_mindmap::import_markdown(text)
            .map_err(|error| AppError::BadRequest(format!("Mindmap Markdown 解析失败：{error}")))?;
        return Ok(ImportedMindmap {
            model,
            assets: Vec::new(),
            warnings: Vec::new(),
        });
    }
    let package = parse_mindmap_exchange(bytes)
        .map_err(|error| AppError::BadRequest(format!("Mindmap JSON 解析失败：{error}")))?;
    let mut model = package.model;
    let mut remap = HashMap::<String, String>::new();
    let mut assets = Vec::with_capacity(package.assets.len());
    let mut decoded_total = 0usize;
    for embedded in package.assets {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(&embedded.data_base64)
            .map_err(|error| {
                AppError::BadRequest(format!(
                    "Mindmap asset {} 的 dataBase64 无效：{error}",
                    embedded.asset_id
                ))
            })?;
        if decoded.is_empty() {
            return Err(AppError::BadRequest(format!(
                "Mindmap asset {} 内容为空",
                embedded.asset_id
            )));
        }
        decoded_total = decoded_total
            .checked_add(decoded.len())
            .ok_or_else(|| AppError::BadRequest("Mindmap embedded asset 总量溢出".into()))?;
        if decoded_total > MAX_UPLOAD_BYTES {
            return Err(AppError::BadRequest(format!(
                "Mindmap embedded assets 超过 {} MB 的上限",
                MAX_UPLOAD_BYTES / 1024 / 1024
            )));
        }
        let actual_checksum = crate::store::checksum(&decoded);
        if actual_checksum != embedded.checksum {
            return Err(AppError::BadRequest(format!(
                "Mindmap asset {} checksum 不匹配",
                embedded.asset_id
            )));
        }
        let new_id = uuid::Uuid::new_v4().to_string();
        remap.insert(embedded.asset_id, new_id.clone());
        assets.push(ImportedBinaryAsset {
            asset_id: new_id,
            content_type: embedded.content_type,
            file_name: embedded.file_name,
            bytes: decoded,
        });
    }
    for node in &mut model.nodes {
        if let Some(image) = &mut node.supplement.image {
            image.asset_id = remap.get(&image.asset_id).cloned().ok_or_else(|| {
                AppError::BadRequest(format!(
                    "Mindmap 图片 {} 缺少 embedded asset",
                    image.asset_id
                ))
            })?;
        }
    }
    ArtifactEnvelope::new("mindmap-import", ArtifactPayload::Mindmap(model.clone()))
        .validate()
        .map_err(|error| AppError::BadRequest(format!("Mindmap JSON 无效：{error}")))?;
    Ok(ImportedMindmap {
        model,
        assets,
        warnings: Vec::new(),
    })
}

fn remap_external_mindmap_assets(
    model: &mut oo_schema::MindmapModel,
    external_assets: Vec<oo_mindmap::MindmapExternalAsset>,
) -> Result<Vec<ImportedBinaryAsset>, AppError> {
    let mut remap = HashMap::<String, String>::new();
    let mut assets = Vec::with_capacity(external_assets.len());
    let mut decoded_total = 0usize;
    for asset in external_assets {
        if asset.bytes.is_empty() {
            return Err(AppError::BadRequest(format!(
                "Mindmap asset {} 内容为空",
                asset.asset_id
            )));
        }
        decoded_total = decoded_total
            .checked_add(asset.bytes.len())
            .ok_or_else(|| AppError::BadRequest("Mindmap embedded asset 总量溢出".into()))?;
        if decoded_total > MAX_UPLOAD_BYTES {
            return Err(AppError::BadRequest(format!(
                "Mindmap embedded assets 超过 {} MB 的上限",
                MAX_UPLOAD_BYTES / 1024 / 1024
            )));
        }
        let new_id = uuid::Uuid::new_v4().to_string();
        if remap
            .insert(asset.asset_id.clone(), new_id.clone())
            .is_some()
        {
            return Err(AppError::BadRequest(format!(
                "Mindmap asset id 重复：{}",
                asset.asset_id
            )));
        }
        assets.push(ImportedBinaryAsset {
            asset_id: new_id,
            content_type: asset.content_type,
            file_name: asset.file_name,
            bytes: asset.bytes,
        });
    }
    for node in &mut model.nodes {
        if let Some(image) = &mut node.supplement.image {
            image.asset_id = remap.get(&image.asset_id).cloned().ok_or_else(|| {
                AppError::BadRequest(format!(
                    "Mindmap 图片 {} 缺少 embedded asset",
                    image.asset_id
                ))
            })?;
        }
    }
    Ok(assets)
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

fn import_source_extension(file_name: &str, kind: ArtifactKind) -> &'static str {
    let lower = file_name.to_ascii_lowercase();
    if kind == ArtifactKind::Mindmap {
        if lower.ends_with(".mindmap.json") {
            return "mindmap.json";
        }
        if lower.ends_with(".md") {
            return "md";
        }
        if lower.ends_with(".mm") {
            return "mm";
        }
        if lower.ends_with(".xmind") {
            return "xmind";
        }
    }
    source_extension(kind)
}

fn source_extension_from_key(key: &str, kind: ArtifactKind) -> &'static str {
    if key.ends_with(".mindmap.json") {
        "mindmap.json"
    } else if key.ends_with(".md") {
        "md"
    } else if key.ends_with(".mm") {
        "mm"
    } else if key.ends_with(".xmind") {
        "xmind"
    } else {
        source_extension(kind)
    }
}

fn source_content_type_for_extension(extension: &str, kind: ArtifactKind) -> &'static str {
    match extension {
        "mindmap.json" => "application/vnd.open-office.mindmap+json",
        "md" => "text/markdown; charset=utf-8",
        "mm" => "application/x-freemind; charset=utf-8",
        "xmind" => "application/vnd.xmind.workbook",
        _ => source_content_type(kind),
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

fn created_event(id: &str, kind: ArtifactKind, actor_id: &str) -> DomainEventRecord {
    DomainEventRecord {
        event_id: format!("artifact:create:{id}:1:0"),
        type_id: "artifact.created".into(),
        payload: serde_json::json!({
            "artifactId": id,
            "artifactKind": serde_json::to_value(kind).unwrap_or_default(),
            "revision": 1,
            "actorId": actor_id,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        document_image_render_asset_ids, require_lossless_pptx_import,
        require_presentation_asset_matches_store,
    };
    use crate::{db, error::AppError};
    use oo_pptx::{PptxLossReport, PptxReportKind, PptxUnsupported};
    use oo_schema::presentation_v5::AssetRef;
    use oo_schema::{
        AssetReferenceSource, BlockData, BlockPresentation, DocumentBlock, DocumentBlockKind,
        DocumentModel, ImageBlock, ImageTransform,
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
                    size: Default::default(),
                    placement: Default::default(),
                    caption: String::new(),
                }),
            }],
            page_setup: None,
            page_semantics: Default::default(),
        };

        assert_eq!(
            document
                .asset_references()
                .into_iter()
                .map(|reference| reference.asset_id)
                .collect::<Vec<_>>(),
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
