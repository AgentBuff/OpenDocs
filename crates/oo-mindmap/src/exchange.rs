//! Versioned offline interchange for canonical Mindmap models.
//!
//! These DTOs never enter the transaction engine. Importers materialize one
//! current-schema candidate and validate it before the server publishes an
//! immutable Artifact snapshot.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Write as _;
use std::io::{Cursor, Read};

use base64::Engine as _;
use oo_schema::{
    ArtifactEnvelope, ArtifactPayload, AssetReferenceSource, MindmapModel, SchemaValidationError,
    CURRENT_SCHEMA_VERSION,
};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde::{Deserialize, Serialize};

use crate::{
    export_markdown, MindmapEngineError, MindmapLayoutOptions, MindmapProjection, MindmapTheme,
};

pub const MINDMAP_EXCHANGE_FORMAT: &str = "open-office-mindmap";
pub const MINDMAP_EXCHANGE_VERSION: u32 = 1;
pub const MAX_MINDMAP_EXCHANGE_BYTES: usize = 32 * 1024 * 1024;
pub const MAX_MINDMAP_EXCHANGE_ASSETS: usize = 256;
pub const MAX_MINDMAP_ASSET_FILE_NAME_BYTES: usize = 255;
pub const MAX_MINDMAP_IMPORT_NODES: usize = 100_000;
pub const MAX_MINDMAP_IMPORT_DEPTH: usize = 256;
pub const MAX_MINDMAP_TEXT_BYTES: usize = 1024 * 1024;
pub const MAX_XMIND_ENTRIES: usize = 4096;
pub const MAX_XMIND_ENTRY_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_XMIND_EXPANDED_BYTES: u64 = 128 * 1024 * 1024;
pub const MAX_XMIND_COMPRESSION_RATIO: u64 = 100;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MindmapExchangeAsset {
    pub asset_id: String,
    pub file_name: String,
    pub content_type: String,
    pub checksum: String,
    pub data_base64: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MindmapExchangeEnvelope {
    pub format: String,
    pub version: u32,
    pub schema_version: u16,
    pub model: MindmapModel,
    #[serde(default)]
    pub assets: Vec<MindmapExchangeAsset>,
}

impl MindmapExchangeEnvelope {
    pub fn new(model: MindmapModel, assets: Vec<MindmapExchangeAsset>) -> Self {
        Self {
            format: MINDMAP_EXCHANGE_FORMAT.into(),
            version: MINDMAP_EXCHANGE_VERSION,
            schema_version: CURRENT_SCHEMA_VERSION,
            model,
            assets,
        }
    }

    pub fn validate(&self) -> Result<(), MindmapExchangeError> {
        if self.format != MINDMAP_EXCHANGE_FORMAT {
            return Err(MindmapExchangeError::UnsupportedFormat(self.format.clone()));
        }
        if self.version != MINDMAP_EXCHANGE_VERSION {
            return Err(MindmapExchangeError::UnsupportedVersion(self.version));
        }
        if self.schema_version != CURRENT_SCHEMA_VERSION {
            return Err(MindmapExchangeError::UnsupportedSchemaVersion(
                self.schema_version,
            ));
        }
        ArtifactEnvelope::new(
            "mindmap-exchange-validation",
            ArtifactPayload::Mindmap(self.model.clone()),
        )
        .validate()
        .map_err(MindmapExchangeError::Schema)?;
        if self.assets.len() > MAX_MINDMAP_EXCHANGE_ASSETS {
            return Err(MindmapExchangeError::TooManyAssets(self.assets.len()));
        }

        let referenced = self
            .model
            .asset_references()
            .into_iter()
            .map(|reference| reference.asset_id)
            .collect::<BTreeSet<_>>();
        let mut embedded = BTreeSet::new();
        for asset in &self.assets {
            validate_asset(asset)?;
            if !embedded.insert(asset.asset_id.clone()) {
                return Err(MindmapExchangeError::DuplicateAsset(asset.asset_id.clone()));
            }
        }
        if referenced != embedded {
            let missing = referenced
                .difference(&embedded)
                .cloned()
                .collect::<Vec<_>>();
            let unused = embedded
                .difference(&referenced)
                .cloned()
                .collect::<Vec<_>>();
            return Err(MindmapExchangeError::AssetClosure { missing, unused });
        }
        Ok(())
    }
}

fn validate_asset(asset: &MindmapExchangeAsset) -> Result<(), MindmapExchangeError> {
    if asset.asset_id.trim().is_empty() {
        return Err(MindmapExchangeError::InvalidAsset(
            "assetId 不能为空".into(),
        ));
    }
    let file_name = asset.file_name.as_str();
    if file_name.trim().is_empty()
        || file_name.len() > MAX_MINDMAP_ASSET_FILE_NAME_BYTES
        || file_name.contains('/')
        || file_name.contains('\\')
        || matches!(file_name, "." | "..")
    {
        return Err(MindmapExchangeError::InvalidAsset(format!(
            "资产 {} 的 fileName 必须是安全 basename",
            asset.asset_id
        )));
    }
    if !matches!(
        asset.content_type.as_str(),
        "image/png" | "image/jpeg" | "image/gif" | "image/webp" | "image/svg+xml"
    ) {
        return Err(MindmapExchangeError::InvalidAsset(format!(
            "资产 {} 的 contentType 不是受支持图片类型",
            asset.asset_id
        )));
    }
    if asset.checksum.len() != 64
        || !asset
            .checksum
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(MindmapExchangeError::InvalidAsset(format!(
            "资产 {} 的 checksum 不是 lowercase SHA-256",
            asset.asset_id
        )));
    }
    if asset.data_base64.is_empty() {
        return Err(MindmapExchangeError::InvalidAsset(format!(
            "资产 {} 的 dataBase64 不能为空",
            asset.asset_id
        )));
    }
    Ok(())
}

pub fn parse_mindmap_exchange(
    input: &[u8],
) -> Result<MindmapExchangeEnvelope, MindmapExchangeError> {
    if input.len() > MAX_MINDMAP_EXCHANGE_BYTES {
        return Err(MindmapExchangeError::InputTooLarge(input.len()));
    }
    let envelope: MindmapExchangeEnvelope =
        serde_json::from_slice(input).map_err(MindmapExchangeError::Json)?;
    envelope.validate()?;
    Ok(envelope)
}

pub fn write_mindmap_exchange(
    envelope: &MindmapExchangeEnvelope,
) -> Result<Vec<u8>, MindmapExchangeError> {
    envelope.validate()?;
    let bytes = serde_json::to_vec_pretty(envelope).map_err(MindmapExchangeError::Json)?;
    if bytes.len() > MAX_MINDMAP_EXCHANGE_BYTES {
        return Err(MindmapExchangeError::InputTooLarge(bytes.len()));
    }
    Ok(bytes)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MindmapExchangeLoss {
    pub capability: String,
    pub count: usize,
    pub detail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MindmapExchangeLossReport {
    pub unsupported: Vec<MindmapExchangeLoss>,
}

impl MindmapExchangeLossReport {
    pub fn header_summary(&self) -> Option<String> {
        (!self.unsupported.is_empty()).then(|| {
            self.unsupported
                .iter()
                .map(|loss| format!("{}:{}", loss.capability, loss.count))
                .collect::<Vec<_>>()
                .join(",")
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MindmapMarkdownExport {
    pub text: String,
    pub loss_report: MindmapExchangeLossReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MindmapSvgExport {
    pub text: String,
    pub loss_report: MindmapExchangeLossReport,
}

#[cfg(feature = "pdf-export")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MindmapPdfPaper {
    #[default]
    A4,
    A3,
}

#[cfg(feature = "pdf-export")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MindmapPdfOrientation {
    Portrait,
    #[default]
    Landscape,
}

#[cfg(feature = "pdf-export")]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum MindmapPdfMode {
    #[default]
    Fit,
    Tile,
}

#[cfg(feature = "pdf-export")]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MindmapPdfOptions {
    pub paper: MindmapPdfPaper,
    pub orientation: MindmapPdfOrientation,
    pub mode: MindmapPdfMode,
    pub margin_points: f32,
}

#[cfg(feature = "pdf-export")]
impl Default for MindmapPdfOptions {
    fn default() -> Self {
        Self {
            paper: MindmapPdfPaper::A4,
            orientation: MindmapPdfOrientation::Landscape,
            mode: MindmapPdfMode::Fit,
            margin_points: 28.0,
        }
    }
}

#[cfg(feature = "pdf-export")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MindmapPdfExport {
    pub bytes: Vec<u8>,
    pub page_count: usize,
    pub loss_report: MindmapExchangeLossReport,
}

#[cfg(feature = "pdf-export")]
/// Converts the standalone SVG projection into a vector PDF. Fit mode emits a
/// single paper-sized page; tile mode keeps 96-DPI scale and clips the same
/// immutable XObject across a stable left-to-right, top-to-bottom page grid.
pub fn export_pdf(
    svg: &str,
    options: MindmapPdfOptions,
) -> Result<MindmapPdfExport, MindmapPdfExportError> {
    if !options.margin_points.is_finite() || !(0.0..=144.0).contains(&options.margin_points) {
        return Err(MindmapPdfExportError::InvalidOptions);
    }
    let (mut page_width, mut page_height) = match options.paper {
        MindmapPdfPaper::A4 => (595.28_f32, 841.89_f32),
        MindmapPdfPaper::A3 => (841.89_f32, 1190.55_f32),
    };
    if options.orientation == MindmapPdfOrientation::Landscape {
        std::mem::swap(&mut page_width, &mut page_height);
    }
    let available_width = page_width - options.margin_points * 2.0;
    let available_height = page_height - options.margin_points * 2.0;
    if available_width <= 0.0 || available_height <= 0.0 {
        return Err(MindmapPdfExportError::InvalidOptions);
    }

    let mut parse_options = svg2pdf::usvg::Options::default();
    parse_options.fontdb_mut().load_system_fonts();
    let tree = svg2pdf::usvg::Tree::from_str(svg, &parse_options)
        .map_err(|error| MindmapPdfExportError::Svg(error.to_string()))?;
    let svg_width = tree.size().width();
    let svg_height = tree.size().height();
    let (scale, columns, rows) = match options.mode {
        MindmapPdfMode::Fit => (
            (available_width / svg_width)
                .min(available_height / svg_height)
                .min(1.0),
            1,
            1,
        ),
        MindmapPdfMode::Tile => {
            let scale = 72.0 / 96.0;
            (
                scale,
                ((svg_width * scale) / available_width).ceil().max(1.0) as usize,
                ((svg_height * scale) / available_height).ceil().max(1.0) as usize,
            )
        }
    };
    let page_count = columns
        .checked_mul(rows)
        .ok_or(MindmapPdfExportError::TooManyPages)?;
    if page_count > 64 {
        return Err(MindmapPdfExportError::TooManyPages);
    }

    use pdf_writer::{Content, Finish, Name, Pdf, Rect, Ref};
    let mut allocator = Ref::new(1);
    let catalog_id = allocator.bump();
    let page_tree_id = allocator.bump();
    let page_ids = (0..page_count)
        .map(|_| allocator.bump())
        .collect::<Vec<_>>();
    let content_ids = (0..page_count)
        .map(|_| allocator.bump())
        .collect::<Vec<_>>();
    let (chunk, svg_id) = svg2pdf::to_chunk(
        &tree,
        svg2pdf::ConversionOptions {
            embed_text: true,
            ..Default::default()
        },
    )
    .map_err(|error| MindmapPdfExportError::Conversion(error.to_string()))?;
    let mut ref_map = HashMap::new();
    let chunk = chunk.renumber(|old| *ref_map.entry(old).or_insert_with(|| allocator.bump()));
    let svg_id = *ref_map
        .get(&svg_id)
        .ok_or_else(|| MindmapPdfExportError::Conversion("SVG XObject ref 丢失".into()))?;
    let svg_name = Name(b"Mindmap");
    let mut pdf = Pdf::new();
    pdf.catalog(catalog_id).pages(page_tree_id);
    pdf.pages(page_tree_id)
        .kids(page_ids.iter().copied())
        .count(page_count as i32);
    let drawn_width = svg_width * scale;
    let drawn_height = svg_height * scale;
    for row in 0..rows {
        for column in 0..columns {
            let index = row * columns + column;
            let mut page = pdf.page(page_ids[index]);
            page.media_box(Rect::new(0.0, 0.0, page_width, page_height));
            page.parent(page_tree_id);
            page.contents(content_ids[index]);
            page.resources().x_objects().pair(svg_name, svg_id);
            page.finish();

            let x = match options.mode {
                MindmapPdfMode::Fit => {
                    options.margin_points + (available_width - drawn_width) / 2.0
                }
                MindmapPdfMode::Tile => options.margin_points - column as f32 * available_width,
            };
            let y = match options.mode {
                MindmapPdfMode::Fit => {
                    options.margin_points + (available_height - drawn_height) / 2.0
                }
                MindmapPdfMode::Tile => {
                    options.margin_points
                        - (drawn_height - (row + 1) as f32 * available_height).max(0.0)
                }
            };
            let mut content = Content::new();
            content
                .save_state()
                .rect(
                    options.margin_points,
                    options.margin_points,
                    available_width,
                    available_height,
                )
                .clip_nonzero()
                .end_path()
                .transform([drawn_width, 0.0, 0.0, drawn_height, x, y])
                .x_object(svg_name)
                .restore_state();
            pdf.stream(content_ids[index], &content.finish());
        }
    }
    pdf.extend(&chunk);
    let bytes = pdf.finish();
    if bytes.len() > MAX_XMIND_EXPANDED_BYTES as usize {
        return Err(MindmapPdfExportError::OutputTooLarge(bytes.len()));
    }
    Ok(MindmapPdfExport {
        bytes,
        page_count,
        loss_report: MindmapExchangeLossReport {
            unsupported: vec![MindmapExchangeLoss {
                capability: "fontSubstitution".into(),
                count: 1,
                detail: "PDF 使用服务端可用字体并嵌入实际使用字形；缺失字形由字体库替代".into(),
            }],
        },
    })
}

#[cfg(feature = "pdf-export")]
#[derive(Debug, thiserror::Error)]
pub enum MindmapPdfExportError {
    #[error("Mindmap PDF 选项无效")]
    InvalidOptions,
    #[error("Mindmap PDF SVG 解析失败：{0}")]
    Svg(String),
    #[error("Mindmap PDF vector 转换失败：{0}")]
    Conversion(String),
    #[error("Mindmap PDF 分页超过 64 页上限")]
    TooManyPages,
    #[error("Mindmap PDF 输出 {0} bytes，超过 128 MiB 上限")]
    OutputTooLarge(usize),
}

/// Renders a standalone, deterministic SVG from the immutable canonical
/// projection. Artifact assets are supplied by the server only after checksum
/// verification; the renderer never reads storage or view state itself.
pub fn export_svg(
    model: &MindmapModel,
    assets: &[MindmapExchangeAsset],
) -> Result<MindmapSvgExport, MindmapSvgExportError> {
    let projection =
        MindmapProjection::build(model, MindmapLayoutOptions::default(), MindmapTheme::Light)?;
    let asset_data = assets
        .iter()
        .map(|asset| -> Result<_, MindmapSvgExportError> {
            if !matches!(
                asset.content_type.as_str(),
                "image/png" | "image/jpeg" | "image/gif" | "image/webp" | "image/svg+xml"
            ) {
                return Err(MindmapSvgExportError::UnsafeAsset(asset.asset_id.clone()));
            }
            if asset.content_type == "image/svg+xml" {
                let decoded = base64::engine::general_purpose::STANDARD
                    .decode(&asset.data_base64)
                    .map_err(|_| MindmapSvgExportError::UnsafeAsset(asset.asset_id.clone()))?;
                if !safe_embedded_svg(&decoded) {
                    return Err(MindmapSvgExportError::UnsafeAsset(asset.asset_id.clone()));
                }
            }
            Ok((
                asset.asset_id.as_str(),
                format!("data:{};base64,{}", asset.content_type, asset.data_base64),
            ))
        })
        .collect::<Result<HashMap<_, _>, _>>()?;
    let nodes = model
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<HashMap<_, _>>();
    let edges = model
        .edges
        .iter()
        .map(|edge| (edge.id.as_str(), edge))
        .collect::<HashMap<_, _>>();
    let summaries = model
        .summaries
        .iter()
        .map(|summary| (summary.id.as_str(), summary))
        .collect::<HashMap<_, _>>();
    let boundaries = model
        .boundaries
        .iter()
        .map(|boundary| (boundary.id.as_str(), boundary))
        .collect::<HashMap<_, _>>();
    let formulas = model
        .formulas
        .iter()
        .map(|formula| (formula.id.as_str(), formula))
        .collect::<HashMap<_, _>>();

    let margin = 32.0;
    let width = (projection.layout.width + margin * 2.0).max(1.0);
    let height = (projection.layout.height + margin * 2.0).max(1.0);
    let mut svg = String::with_capacity(model.nodes.len().saturating_mul(512));
    writeln!(
        svg,
        r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width}" height="{height}" viewBox="0 0 {width} {height}" role="img" aria-labelledby="mindmap-title">"#
    )?;
    svg.push_str("<title id=\"mindmap-title\">Mindmap</title>\n");
    svg.push_str("<defs><marker id=\"mindmap-arrow\" markerWidth=\"8\" markerHeight=\"8\" refX=\"7\" refY=\"4\" orient=\"auto\"><path d=\"M0 0L8 4L0 8Z\" fill=\"#64748b\"/></marker></defs>\n");
    writeln!(
        svg,
        r#"<g transform="translate({margin} {margin})" font-family="&quot;Hiragino Sans GB&quot;, &quot;Noto Sans CJK SC&quot;, &quot;Source Han Sans SC&quot;, &quot;Microsoft YaHei&quot;, Inter, system-ui, sans-serif">"#
    )?;

    for item in &projection.advanced.boundaries {
        let label = boundaries
            .get(item.boundary_id.as_str())
            .and_then(|boundary| boundary.label.as_ref());
        writeln!(
            svg,
            r##"<rect data-kind="boundary" x="{}" y="{}" width="{}" height="{}" rx="12" fill="#dbeafe" fill-opacity="0.18" stroke="#60a5fa" stroke-width="1.5" stroke-dasharray="6 4"/>"##,
            item.rect.x, item.rect.y, item.rect.width, item.rect.height
        )?;
        if let Some(label) = label {
            write_svg_text(
                &mut svg,
                item.label_anchor.x,
                item.label_anchor.y,
                "start",
                12.0,
                "#2563eb",
                &label.text,
                "boundary-label",
            )?;
        }
    }

    for route in &projection.edges.routes {
        let explicit = route.edge_id.is_some();
        let style = route
            .edge_id
            .as_deref()
            .and_then(|id| edges.get(id).copied())
            .map(|edge| &edge.style)
            .unwrap_or(&model.settings.connector);
        let color = safe_svg_color(style.color.as_deref(), "#94a3b8");
        let points = route
            .points
            .iter()
            .map(|point| format!("{},{}", point.x, point.y))
            .collect::<Vec<_>>()
            .join(" ");
        writeln!(
            svg,
            r#"<polyline data-kind="{}" points="{}" fill="none" stroke="{}" stroke-width="{}"{}{} stroke-linecap="round" stroke-linejoin="round"/>"#,
            if explicit {
                "explicit-edge"
            } else {
                "tree-edge"
            },
            points,
            color,
            style.width,
            if style.dashed {
                " stroke-dasharray=\"7 5\""
            } else {
                ""
            },
            if explicit {
                " marker-end=\"url(#mindmap-arrow)\""
            } else {
                ""
            }
        )?;
        if let Some(edge) = route
            .edge_id
            .as_deref()
            .and_then(|id| edges.get(id).copied())
        {
            if let Some(label) = &edge.label {
                let anchor = route.points[route.points.len() / 2];
                write_svg_text(
                    &mut svg,
                    anchor.x + 5.0,
                    anchor.y - 5.0,
                    "start",
                    11.0,
                    "#475569",
                    &label.text,
                    "edge-label",
                )?;
            }
        }
    }

    for item in &projection.advanced.summaries {
        let Some(summary) = summaries.get(item.summary_id.as_str()) else {
            continue;
        };
        let points = item
            .points
            .iter()
            .map(|point| format!("{},{}", point.x, point.y))
            .collect::<Vec<_>>()
            .join(" ");
        writeln!(
            svg,
            r##"<polyline data-kind="summary" points="{points}" fill="none" stroke="#8b5cf6" stroke-width="2"/>"##
        )?;
        write_svg_text(
            &mut svg,
            item.label_anchor.x,
            item.label_anchor.y,
            "start",
            12.0,
            "#6d28d9",
            &summary.content.text,
            "summary-label",
        )?;
    }

    for item in &projection.layout.nodes {
        let Some(node) = nodes.get(item.id.as_str()).copied() else {
            continue;
        };
        let fill = safe_svg_color(
            node.style.fill_color.as_deref(),
            if item.depth == 0 {
                "#2563eb"
            } else {
                "#ffffff"
            },
        );
        let border = safe_svg_color(
            node.style.border_color.as_deref(),
            if item.depth == 0 {
                "#1d4ed8"
            } else {
                "#cbd5e1"
            },
        );
        let text_color = safe_svg_color(
            node.style.text_color.as_deref(),
            if item.depth == 0 {
                "#ffffff"
            } else {
                "#0f172a"
            },
        );
        write_svg_node_shape(
            &mut svg,
            item,
            node.style.shape,
            fill,
            border,
            node.style.border_width,
        )?;
        let text_x = match node.style.text_align {
            oo_schema::MindmapTextAlign::Start => item.x + 12.0,
            oo_schema::MindmapTextAlign::Center => item.x + item.width / 2.0,
            oo_schema::MindmapTextAlign::End => item.x + item.width - 12.0,
        };
        let anchor = match node.style.text_align {
            oo_schema::MindmapTextAlign::Start => "start",
            oo_schema::MindmapTextAlign::Center => "middle",
            oo_schema::MindmapTextAlign::End => "end",
        };
        write_svg_text(
            &mut svg,
            text_x,
            item.y + item.height / 2.0 + 4.0,
            anchor,
            13.0,
            text_color,
            node.content
                .as_ref()
                .map_or("", |content| content.text.as_str()),
            "node-label",
        )?;
        if let Some(image) = &node.supplement.image {
            let href = asset_data
                .get(image.asset_id.as_str())
                .ok_or_else(|| MindmapSvgExportError::MissingAsset(image.asset_id.clone()))?;
            let image_width = image.width.unwrap_or(24.0).min(item.width / 3.0).max(8.0);
            let image_height = image.height.unwrap_or(24.0).min(item.height - 8.0).max(8.0);
            writeln!(
                svg,
                r#"<image data-kind="node-image" x="{}" y="{}" width="{}" height="{}" href="{}" preserveAspectRatio="xMidYMid meet"/>"#,
                item.x + item.width - image_width - 4.0,
                item.y + (item.height - image_height) / 2.0,
                image_width,
                image_height,
                xml_escape_attr(href)
            )?;
        }
        if node.collapsed {
            writeln!(
                svg,
                r##"<circle data-kind="collapsed" cx="{}" cy="{}" r="7" fill="#ffffff" stroke="#64748b"/><path d="M{} {}h8M{} {}v8" stroke="#475569" stroke-width="1.5"/>"##,
                item.x + item.width + 8.0,
                item.y + item.height / 2.0,
                item.x + item.width + 4.0,
                item.y + item.height / 2.0,
                item.x + item.width + 8.0,
                item.y + item.height / 2.0 - 4.0
            )?;
        }
    }

    for item in &projection.advanced.formulas {
        let Some(formula) = formulas.get(item.formula_id.as_str()) else {
            continue;
        };
        write_svg_text(
            &mut svg,
            item.anchor.x,
            item.anchor.y,
            "middle",
            12.0,
            "#0f766e",
            &format!("${}$", formula.source),
            "formula",
        )?;
    }
    svg.push_str("</g>\n</svg>\n");

    let mut losses = BTreeMap::new();
    let rich_runs = model
        .nodes
        .iter()
        .filter(|node| {
            node.content
                .as_ref()
                .is_some_and(|text| !text.runs.is_empty())
        })
        .count();
    if rich_runs > 0 {
        losses.insert("richTextFormatting", rich_runs);
    }
    Ok(MindmapSvgExport {
        text: svg,
        loss_report: MindmapExchangeLossReport {
            unsupported: losses
                .into_iter()
                .map(|(capability, count)| MindmapExchangeLoss {
                    capability: capability.into(),
                    count,
                    detail: "SVG 当前使用节点级样式，行内 RichText run 降级为纯文本".into(),
                })
                .collect(),
        },
    })
}

fn write_svg_node_shape(
    svg: &mut String,
    item: &crate::MindmapLayoutNode,
    shape: oo_schema::MindmapNodeShape,
    fill: &str,
    border: &str,
    border_width: f32,
) -> Result<(), std::fmt::Error> {
    match shape {
        oo_schema::MindmapNodeShape::Ellipse => writeln!(
            svg,
            r#"<ellipse data-kind="node" cx="{}" cy="{}" rx="{}" ry="{}" fill="{fill}" stroke="{border}" stroke-width="{border_width}"/>"#,
            item.x + item.width / 2.0,
            item.y + item.height / 2.0,
            item.width / 2.0,
            item.height / 2.0
        ),
        oo_schema::MindmapNodeShape::Diamond => writeln!(
            svg,
            r#"<polygon data-kind="node" points="{},{} {},{} {},{} {},{}" fill="{fill}" stroke="{border}" stroke-width="{border_width}"/>"#,
            item.x + item.width / 2.0,
            item.y,
            item.x + item.width,
            item.y + item.height / 2.0,
            item.x + item.width / 2.0,
            item.y + item.height,
            item.x,
            item.y + item.height / 2.0
        ),
        oo_schema::MindmapNodeShape::Underline => writeln!(
            svg,
            r#"<line data-kind="node" x1="{}" y1="{}" x2="{}" y2="{}" stroke="{border}" stroke-width="{border_width}"/>"#,
            item.x,
            item.y + item.height,
            item.x + item.width,
            item.y + item.height
        ),
        oo_schema::MindmapNodeShape::Rectangle => writeln!(
            svg,
            r#"<rect data-kind="node" x="{}" y="{}" width="{}" height="{}" fill="{fill}" stroke="{border}" stroke-width="{border_width}"/>"#,
            item.x, item.y, item.width, item.height
        ),
        oo_schema::MindmapNodeShape::Pill => writeln!(
            svg,
            r#"<rect data-kind="node" x="{}" y="{}" width="{}" height="{}" rx="{}" fill="{fill}" stroke="{border}" stroke-width="{border_width}"/>"#,
            item.x,
            item.y,
            item.width,
            item.height,
            item.height / 2.0
        ),
        oo_schema::MindmapNodeShape::RoundedRectangle => writeln!(
            svg,
            r#"<rect data-kind="node" x="{}" y="{}" width="{}" height="{}" rx="8" fill="{fill}" stroke="{border}" stroke-width="{border_width}"/>"#,
            item.x, item.y, item.width, item.height
        ),
    }
}

#[allow(clippy::too_many_arguments)]
fn write_svg_text(
    svg: &mut String,
    x: f32,
    y: f32,
    anchor: &str,
    size: f32,
    color: &str,
    value: &str,
    kind: &str,
) -> Result<(), std::fmt::Error> {
    writeln!(
        svg,
        r#"<text data-kind="{kind}" x="{x}" y="{y}" text-anchor="{anchor}" font-size="{size}" fill="{color}">{}</text>"#,
        xml_escape_text(value)
    )
}

fn safe_svg_color<'a>(candidate: Option<&'a str>, fallback: &'a str) -> &'a str {
    candidate
        .filter(|value| {
            let bytes = value.as_bytes();
            bytes.first() == Some(&b'#')
                && matches!(bytes.len(), 4 | 5 | 7 | 9)
                && bytes[1..].iter().all(u8::is_ascii_hexdigit)
        })
        .unwrap_or(fallback)
}

fn xml_escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn xml_escape_attr(value: &str) -> String {
    xml_escape_text(value)
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

#[derive(Debug, thiserror::Error)]
pub enum MindmapSvgExportError {
    #[error("Mindmap SVG projection 失败：{0}")]
    Layout(#[from] crate::MindmapLayoutError),
    #[error("Mindmap SVG 缺少图片资产：{0}")]
    MissingAsset(String),
    #[error("Mindmap SVG 图片资产不安全或类型不受支持：{0}")]
    UnsafeAsset(String),
    #[error("Mindmap SVG string 写入失败：{0}")]
    Format(#[from] std::fmt::Error),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MindmapExternalImport {
    pub model: MindmapModel,
    pub assets: Vec<MindmapExternalAsset>,
    pub loss_report: MindmapExchangeLossReport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MindmapExternalAsset {
    pub asset_id: String,
    pub file_name: String,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

/// Imports the structural FreeMind `.mm` subset using a bounded streaming
/// parser. DTDs are rejected before any entity expansion can occur.
pub fn import_freemind(input: &[u8]) -> Result<MindmapExternalImport, MindmapExternalImportError> {
    if input.len() > MAX_MINDMAP_EXCHANGE_BYTES {
        return Err(MindmapExternalImportError::InputTooLarge(input.len()));
    }
    let mut reader = Reader::from_reader(input);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut model = MindmapModel::default();
    let mut node_stack = Vec::<String>::new();
    let mut external_ids = HashMap::<String, String>::new();
    let mut pending_edges = Vec::<(String, String)>::new();
    let mut losses = BTreeMap::<&'static str, usize>::new();
    let mut note_capture: Option<(String, usize, String)> = None;

    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) if note_capture.is_some() => {
                let name = event.local_name();
                let (_, depth, text) = note_capture.as_mut().expect("checked");
                if matches!(name.as_ref(), b"p" | b"div" | b"li") && !text.ends_with('\n') {
                    text.push('\n');
                }
                *depth += 1;
            }
            Event::Empty(event) if note_capture.is_some() => {
                if event.local_name().as_ref() == b"br" {
                    note_capture.as_mut().expect("checked").2.push('\n');
                }
            }
            Event::Text(text) if note_capture.is_some() => {
                let decoded = text.unescape()?.into_owned();
                let capture = note_capture.as_mut().expect("checked");
                if capture.2.len().saturating_add(decoded.len()) > MAX_MINDMAP_TEXT_BYTES {
                    return Err(MindmapExternalImportError::TextTooLarge);
                }
                capture.2.push_str(&decoded);
            }
            Event::End(event) if note_capture.is_some() => {
                let is_outer = {
                    let (_, depth, _) = note_capture.as_mut().expect("checked");
                    if *depth == 1 && event.local_name().as_ref() == b"richcontent" {
                        true
                    } else {
                        *depth = depth.saturating_sub(1);
                        false
                    }
                };
                if is_outer {
                    let (owner, _, text) = note_capture.take().expect("checked");
                    let normalized = normalize_imported_note(&text);
                    if !normalized.is_empty() {
                        let node = model
                            .nodes
                            .iter_mut()
                            .find(|node| node.id == owner)
                            .ok_or_else(|| {
                                MindmapExternalImportError::InvalidStructure(
                                    "FreeMind note owner 不存在".into(),
                                )
                            })?;
                        node.supplement.note = Some(oo_schema::RichText {
                            text: normalized,
                            runs: Vec::new(),
                        });
                    }
                }
            }
            Event::Start(event) if event.local_name().as_ref() == b"node" => {
                let id = push_freemind_node(
                    &event,
                    &reader,
                    &mut model,
                    &node_stack,
                    &mut external_ids,
                    &mut losses,
                )?;
                node_stack.push(id);
                if node_stack.len() > MAX_MINDMAP_IMPORT_DEPTH {
                    return Err(MindmapExternalImportError::DepthLimit);
                }
            }
            Event::Empty(event) if event.local_name().as_ref() == b"node" => {
                push_freemind_node(
                    &event,
                    &reader,
                    &mut model,
                    &node_stack,
                    &mut external_ids,
                    &mut losses,
                )?;
            }
            Event::End(event) if event.local_name().as_ref() == b"node" => {
                node_stack.pop().ok_or_else(|| {
                    MindmapExternalImportError::InvalidStructure(
                        "FreeMind node 结束标签没有对应开始标签".into(),
                    )
                })?;
            }
            Event::Start(event) if event.local_name().as_ref() == b"richcontent" => {
                if attr(&event, b"TYPE", &reader)?.as_deref() == Some("NOTE") {
                    let owner = node_stack.last().cloned().ok_or_else(|| {
                        MindmapExternalImportError::InvalidStructure(
                            "FreeMind note 不在 node 内".into(),
                        )
                    })?;
                    note_capture = Some((owner, 1, String::new()));
                } else {
                    *losses.entry("richTextFormatting").or_default() += 1;
                }
            }
            Event::Empty(event) if event.local_name().as_ref() == b"arrowlink" => {
                let source = node_stack.last().cloned().ok_or_else(|| {
                    MindmapExternalImportError::InvalidStructure(
                        "FreeMind arrowlink 不在 node 内".into(),
                    )
                })?;
                if let Some(destination) = attr(&event, b"DESTINATION", &reader)? {
                    pending_edges.push((source, destination));
                } else {
                    *losses.entry("explicitEdge").or_default() += 1;
                }
            }
            Event::Start(event) | Event::Empty(event) if event.local_name().as_ref() == b"icon" => {
                *losses.entry("unsupportedFeature").or_default() += 1;
            }
            Event::DocType(_) => return Err(MindmapExternalImportError::DocTypeForbidden),
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    if note_capture.is_some() || !node_stack.is_empty() {
        return Err(MindmapExternalImportError::UnexpectedEof);
    }
    if model.nodes.is_empty() {
        return Err(MindmapExternalImportError::NoTopics);
    }
    for (source_id, destination_external_id) in pending_edges {
        let Some(target_id) = external_ids.get(&destination_external_id).cloned() else {
            *losses.entry("externalResource").or_default() += 1;
            continue;
        };
        if source_id == target_id {
            *losses.entry("explicitEdge").or_default() += 1;
            continue;
        }
        model.edges.push(oo_schema::MindmapEdge {
            id: format!("edge-{}", model.edges.len() + 1),
            source_id,
            target_id,
            ..Default::default()
        });
    }
    ArtifactEnvelope::new(
        "freemind-import-validation",
        ArtifactPayload::Mindmap(model.clone()),
    )
    .validate()
    .map_err(MindmapExternalImportError::Schema)?;
    Ok(MindmapExternalImport {
        model,
        assets: Vec::new(),
        loss_report: external_loss_report(losses),
    })
}

/// Imports the documented JSON-based XMind package subset. Package entries
/// are fully bounded and normalized before `content.json` is inspected, so no
/// renderer or asset-store code ever sees a path supplied directly by ZIP.
pub fn import_xmind(input: &[u8]) -> Result<MindmapExternalImport, MindmapExternalImportError> {
    if input.len() > MAX_MINDMAP_EXCHANGE_BYTES {
        return Err(MindmapExternalImportError::InputTooLarge(input.len()));
    }
    let mut archive = zip::ZipArchive::new(Cursor::new(input))?;
    if archive.len() > MAX_XMIND_ENTRIES {
        return Err(MindmapExternalImportError::TooManyArchiveEntries(
            archive.len(),
        ));
    }
    let mut entries = HashMap::<String, Vec<u8>>::new();
    let mut expanded = 0u64;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        if file.is_dir() {
            continue;
        }
        if file.encrypted() {
            return Err(MindmapExternalImportError::EncryptedArchiveEntry);
        }
        let path = file
            .enclosed_name()
            .ok_or_else(|| MindmapExternalImportError::UnsafeArchivePath(file.name().to_string()))?
            .to_string_lossy()
            .replace('\\', "/");
        if path.starts_with('/') || path.split('/').any(|part| part.is_empty() || part == "..") {
            return Err(MindmapExternalImportError::UnsafeArchivePath(path));
        }
        if file.size() > MAX_XMIND_ENTRY_BYTES {
            return Err(MindmapExternalImportError::ArchiveEntryTooLarge {
                path,
                size: file.size(),
            });
        }
        let compressed = file.compressed_size();
        if file.size() > 0
            && (compressed == 0 || file.size() / compressed.max(1) > MAX_XMIND_COMPRESSION_RATIO)
        {
            return Err(MindmapExternalImportError::SuspiciousCompression(path));
        }
        expanded = expanded
            .checked_add(file.size())
            .ok_or(MindmapExternalImportError::ExpandedArchiveTooLarge)?;
        if expanded > MAX_XMIND_EXPANDED_BYTES {
            return Err(MindmapExternalImportError::ExpandedArchiveTooLarge);
        }
        let capacity = usize::try_from(file.size()).map_err(|_| {
            MindmapExternalImportError::ArchiveEntryTooLarge {
                path: path.clone(),
                size: file.size(),
            }
        })?;
        let mut bytes = Vec::with_capacity(capacity);
        file.read_to_end(&mut bytes)?;
        if entries.insert(path.clone(), bytes).is_some() {
            return Err(MindmapExternalImportError::DuplicateArchivePath(path));
        }
    }
    let content = entries
        .get("content.json")
        .ok_or(MindmapExternalImportError::MissingXmindContent)?;
    let sheets: serde_json::Value = serde_json::from_slice(content)?;
    ensure_json_depth(&sheets, 0)?;
    let sheets = sheets.as_array().ok_or_else(|| {
        MindmapExternalImportError::InvalidStructure("XMind content.json 顶层必须是数组".into())
    })?;
    let first_sheet = sheets.first().ok_or(MindmapExternalImportError::NoTopics)?;
    let sheet = first_sheet.as_object().ok_or_else(|| {
        MindmapExternalImportError::InvalidStructure("XMind sheet 必须是对象".into())
    })?;
    let root = sheet
        .get("rootTopic")
        .and_then(serde_json::Value::as_object)
        .ok_or_else(|| {
            MindmapExternalImportError::InvalidStructure("XMind sheet 缺少 rootTopic".into())
        })?;
    let mut model = MindmapModel::default();
    let mut external_ids = HashMap::<String, String>::new();
    let mut resource_ids = HashMap::<String, String>::new();
    let mut assets = Vec::<MindmapExternalAsset>::new();
    let mut losses = BTreeMap::<&'static str, usize>::new();
    if sheets.len() > 1 {
        losses.insert("unsupportedFeature", sheets.len() - 1);
    }
    push_xmind_topic(
        root,
        None,
        0,
        &entries,
        &mut model,
        &mut external_ids,
        &mut resource_ids,
        &mut assets,
        &mut losses,
    )?;

    if let Some(relationships) = sheet
        .get("relationships")
        .and_then(serde_json::Value::as_array)
    {
        for relationship in relationships {
            let Some(record) = relationship.as_object() else {
                *losses.entry("explicitEdge").or_default() += 1;
                continue;
            };
            let source = record
                .get("end1Id")
                .and_then(serde_json::Value::as_str)
                .and_then(|id| external_ids.get(id))
                .cloned();
            let target = record
                .get("end2Id")
                .and_then(serde_json::Value::as_str)
                .and_then(|id| external_ids.get(id))
                .cloned();
            let (Some(source_id), Some(target_id)) = (source, target) else {
                *losses.entry("externalResource").or_default() += 1;
                continue;
            };
            if source_id == target_id {
                *losses.entry("explicitEdge").or_default() += 1;
                continue;
            }
            let label = record
                .get("title")
                .and_then(serde_json::Value::as_str)
                .filter(|title| !title.is_empty())
                .map(|title| oo_schema::RichText {
                    text: title.to_string(),
                    runs: Vec::new(),
                });
            model.edges.push(oo_schema::MindmapEdge {
                id: format!("edge-{}", model.edges.len() + 1),
                source_id,
                target_id,
                label,
                ..Default::default()
            });
        }
    }
    ArtifactEnvelope::new(
        "xmind-import-validation",
        ArtifactPayload::Mindmap(model.clone()),
    )
    .validate()
    .map_err(MindmapExternalImportError::Schema)?;
    Ok(MindmapExternalImport {
        model,
        assets,
        loss_report: external_loss_report(losses),
    })
}

#[allow(clippy::too_many_arguments)]
fn push_xmind_topic(
    topic: &serde_json::Map<String, serde_json::Value>,
    parent_id: Option<String>,
    depth: usize,
    entries: &HashMap<String, Vec<u8>>,
    model: &mut MindmapModel,
    external_ids: &mut HashMap<String, String>,
    resource_ids: &mut HashMap<String, String>,
    assets: &mut Vec<MindmapExternalAsset>,
    losses: &mut BTreeMap<&'static str, usize>,
) -> Result<(), MindmapExternalImportError> {
    if depth >= MAX_MINDMAP_IMPORT_DEPTH {
        return Err(MindmapExternalImportError::DepthLimit);
    }
    if model.nodes.len() >= MAX_MINDMAP_IMPORT_NODES {
        return Err(MindmapExternalImportError::NodeLimit);
    }
    let title = topic
        .get("title")
        .and_then(serde_json::Value::as_str)
        .filter(|title| !title.trim().is_empty())
        .unwrap_or("未命名主题");
    if title.len() > MAX_MINDMAP_TEXT_BYTES {
        return Err(MindmapExternalImportError::TextTooLarge);
    }
    let node_id = format!("node-{}", model.nodes.len() + 1);
    if let Some(external_id) = topic.get("id").and_then(serde_json::Value::as_str) {
        if external_ids
            .insert(external_id.to_string(), node_id.clone())
            .is_some()
        {
            return Err(MindmapExternalImportError::DuplicateExternalId(
                external_id.into(),
            ));
        }
    }
    let mut node = oo_schema::MindmapNode {
        id: node_id.clone(),
        parent_id,
        content: Some(oo_schema::RichText {
            text: title.into(),
            runs: Vec::new(),
        }),
        collapsed: topic.get("branch").and_then(serde_json::Value::as_str) == Some("folded"),
        ..Default::default()
    };
    if let Some(href) = topic
        .get("href")
        .and_then(serde_json::Value::as_str)
        .filter(|href| !href.trim().is_empty())
    {
        if href.len() <= 2048 {
            node.supplement.hyperlink = Some(href.into());
        } else {
            *losses.entry("nodeHyperlink").or_default() += 1;
        }
    }
    if let Some(note) = topic
        .get("notes")
        .and_then(serde_json::Value::as_object)
        .and_then(|notes| notes.get("plain"))
        .and_then(serde_json::Value::as_object)
        .and_then(|plain| plain.get("content"))
        .and_then(serde_json::Value::as_str)
    {
        if note.len() > MAX_MINDMAP_TEXT_BYTES {
            return Err(MindmapExternalImportError::TextTooLarge);
        }
        node.supplement.note = Some(oo_schema::RichText {
            text: note.into(),
            runs: Vec::new(),
        });
    } else if topic.get("notes").is_some() {
        *losses.entry("richTextFormatting").or_default() += 1;
    }
    if let Some(markers) = topic.get("markers").and_then(serde_json::Value::as_array) {
        let mut seen = HashSet::new();
        for marker in markers {
            let Some(marker_id) = marker
                .as_object()
                .and_then(|marker| marker.get("markerId"))
                .and_then(serde_json::Value::as_str)
            else {
                *losses.entry("nodeMarkers").or_default() += 1;
                continue;
            };
            if marker_id.len() <= 64 && seen.insert(marker_id) {
                node.supplement.markers.push(marker_id.into());
            } else {
                *losses.entry("nodeMarkers").or_default() += 1;
            }
        }
    }
    if let Some(image) = topic.get("image").and_then(serde_json::Value::as_object) {
        let source = image
            .get("src")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        match normalize_xmind_resource_path(source) {
            Some(path) => {
                let bytes = entries.get(&path).ok_or_else(|| {
                    MindmapExternalImportError::MissingArchiveResource(path.clone())
                })?;
                let content_type = image_content_type(&path).ok_or_else(|| {
                    MindmapExternalImportError::UnsupportedImageType(path.clone())
                })?;
                if content_type == "image/svg+xml" && !safe_embedded_svg(bytes) {
                    return Err(MindmapExternalImportError::UnsafeSvg(path));
                }
                let asset_id = if let Some(asset_id) = resource_ids.get(&path) {
                    asset_id.clone()
                } else {
                    let asset_id = format!("asset-{}", assets.len() + 1);
                    assets.push(MindmapExternalAsset {
                        asset_id: asset_id.clone(),
                        file_name: path.rsplit('/').next().unwrap_or("image.bin").into(),
                        content_type: content_type.into(),
                        bytes: bytes.clone(),
                    });
                    resource_ids.insert(path, asset_id.clone());
                    asset_id
                };
                node.supplement.image = Some(oo_schema::MindmapImage {
                    asset_id,
                    alt: title.into(),
                    width: bounded_image_dimension(image.get("width"), losses),
                    height: bounded_image_dimension(image.get("height"), losses),
                });
            }
            None => *losses.entry("externalResource").or_default() += 1,
        }
    }
    if topic.contains_key("style") || topic.contains_key("structureClass") {
        *losses.entry("nodeStyle").or_default() += 1;
    }
    if model.root.is_none() {
        model.root = Some(node_id.clone());
    }
    model.nodes.push(node);

    if let Some(children) = topic.get("children").and_then(serde_json::Value::as_object) {
        if let Some(summaries) = children
            .get("summary")
            .and_then(serde_json::Value::as_array)
        {
            *losses.entry("summary").or_default() += summaries.len();
        }
        if let Some(attached) = children
            .get("attached")
            .and_then(serde_json::Value::as_array)
        {
            for child in attached {
                let child = child.as_object().ok_or_else(|| {
                    MindmapExternalImportError::InvalidStructure(
                        "XMind attached topic 必须是对象".into(),
                    )
                })?;
                push_xmind_topic(
                    child,
                    Some(node_id.clone()),
                    depth + 1,
                    entries,
                    model,
                    external_ids,
                    resource_ids,
                    assets,
                    losses,
                )?;
            }
        }
    }
    Ok(())
}

fn ensure_json_depth(
    value: &serde_json::Value,
    depth: usize,
) -> Result<(), MindmapExternalImportError> {
    if depth > MAX_MINDMAP_IMPORT_DEPTH {
        return Err(MindmapExternalImportError::DepthLimit);
    }
    match value {
        serde_json::Value::Array(values) => {
            for value in values {
                ensure_json_depth(value, depth + 1)?;
            }
        }
        serde_json::Value::Object(values) => {
            for value in values.values() {
                ensure_json_depth(value, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn normalize_xmind_resource_path(source: &str) -> Option<String> {
    let source = source.strip_prefix("xap:")?.trim_start_matches('/');
    if source.is_empty()
        || source.contains('\\')
        || source
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return None;
    }
    Some(source.into())
}

fn image_content_type(path: &str) -> Option<&'static str> {
    let extension = path.rsplit('.').next()?.to_ascii_lowercase();
    match extension.as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        "svg" => Some("image/svg+xml"),
        _ => None,
    }
}

fn safe_embedded_svg(bytes: &[u8]) -> bool {
    let Ok(text) = std::str::from_utf8(bytes) else {
        return false;
    };
    let lower = text.to_ascii_lowercase();
    ![
        "<!doctype",
        "<script",
        "javascript:",
        "onload=",
        "onerror=",
        "href=\"http",
        "href='http",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn bounded_image_dimension(
    value: Option<&serde_json::Value>,
    losses: &mut BTreeMap<&'static str, usize>,
) -> Option<f32> {
    let value = value.and_then(serde_json::Value::as_f64)?;
    if value.is_finite() && (8.0..=4096.0).contains(&value) {
        Some(value as f32)
    } else {
        *losses.entry("nodeImage").or_default() += 1;
        None
    }
}

fn push_freemind_node(
    event: &BytesStart<'_>,
    reader: &Reader<&[u8]>,
    model: &mut MindmapModel,
    node_stack: &[String],
    external_ids: &mut HashMap<String, String>,
    losses: &mut BTreeMap<&'static str, usize>,
) -> Result<String, MindmapExternalImportError> {
    if model.nodes.len() >= MAX_MINDMAP_IMPORT_NODES {
        return Err(MindmapExternalImportError::NodeLimit);
    }
    if node_stack.is_empty() && model.root.is_some() {
        return Err(MindmapExternalImportError::InvalidStructure(
            "FreeMind 文件只能有一个 root node".into(),
        ));
    }
    let text = attr(event, b"TEXT", reader)?
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "未命名主题".into());
    if text.len() > MAX_MINDMAP_TEXT_BYTES {
        return Err(MindmapExternalImportError::TextTooLarge);
    }
    let id = format!("node-{}", model.nodes.len() + 1);
    if let Some(external_id) = attr(event, b"ID", reader)? {
        if external_ids
            .insert(external_id.clone(), id.clone())
            .is_some()
        {
            return Err(MindmapExternalImportError::DuplicateExternalId(external_id));
        }
    }
    let mut node = oo_schema::MindmapNode {
        id: id.clone(),
        parent_id: node_stack.last().cloned(),
        content: Some(oo_schema::RichText {
            text,
            runs: Vec::new(),
        }),
        collapsed: attr(event, b"FOLDED", reader)?
            .is_some_and(|value| value.eq_ignore_ascii_case("true")),
        ..Default::default()
    };
    node.supplement.hyperlink = attr(event, b"LINK", reader)?;
    if let Some(color) = attr(event, b"COLOR", reader)? {
        if is_hex_color(&color) {
            node.style.text_color = Some(color);
        } else {
            *losses.entry("nodeStyle").or_default() += 1;
        }
    }
    if let Some(color) = attr(event, b"BACKGROUND_COLOR", reader)? {
        if is_hex_color(&color) {
            node.style.fill_color = Some(color);
        } else {
            *losses.entry("nodeStyle").or_default() += 1;
        }
    }
    let supported = HashSet::from([
        b"TEXT".as_slice(),
        b"ID".as_slice(),
        b"FOLDED".as_slice(),
        b"LINK".as_slice(),
        b"COLOR".as_slice(),
        b"BACKGROUND_COLOR".as_slice(),
        b"POSITION".as_slice(),
    ]);
    if event
        .attributes()
        .with_checks(false)
        .filter_map(Result::ok)
        .any(|attribute| !supported.contains(attribute.key.local_name().as_ref()))
    {
        *losses.entry("unsupportedFeature").or_default() += 1;
    }
    if model.root.is_none() {
        model.root = Some(id.clone());
    }
    model.nodes.push(node);
    Ok(id)
}

fn attr(
    event: &BytesStart<'_>,
    name: &[u8],
    reader: &Reader<&[u8]>,
) -> Result<Option<String>, MindmapExternalImportError> {
    for attribute in event.attributes().with_checks(false) {
        let attribute = attribute?;
        if attribute.key.local_name().as_ref() == name {
            return Ok(Some(
                attribute
                    .decode_and_unescape_value(reader.decoder())?
                    .into_owned(),
            ));
        }
    }
    Ok(None)
}

fn is_hex_color(value: &str) -> bool {
    value.len() == 7
        && value.starts_with('#')
        && value.as_bytes()[1..].iter().all(u8::is_ascii_hexdigit)
}

fn normalize_imported_note(value: &str) -> String {
    value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn external_loss_report(counts: BTreeMap<&'static str, usize>) -> MindmapExchangeLossReport {
    MindmapExchangeLossReport {
        unsupported: counts
            .into_iter()
            .filter(|(_, count)| *count > 0)
            .map(|(capability, count)| MindmapExchangeLoss {
                capability: capability.into(),
                count,
                detail: match capability {
                    "explicitEdge" => "部分 FreeMind arrowlink 无法解析",
                    "externalResource" => "外部引用目标不存在或不可导入",
                    "nodeStyle" => "部分 FreeMind 样式值不受支持",
                    "richTextFormatting" => "富文本被归一化为纯文本",
                    _ => "FreeMind 能力不在当前 canonical 子集中",
                }
                .into(),
            })
            .collect(),
    }
}

#[derive(Debug, thiserror::Error)]
pub enum MindmapExternalImportError {
    #[error("Mindmap 外部文件 {0} bytes，超过 32 MiB 上限")]
    InputTooLarge(usize),
    #[error("FreeMind XML 无效：{0}")]
    Xml(#[from] quick_xml::Error),
    #[error("FreeMind XML attribute 无效：{0}")]
    Attribute(#[from] quick_xml::events::attributes::AttrError),
    #[error("XMind ZIP 无效：{0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("读取 XMind ZIP entry 失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("XMind content.json 无效：{0}")]
    Json(#[from] serde_json::Error),
    #[error("XMind ZIP entry 数量 {0}，超过 4096 上限")]
    TooManyArchiveEntries(usize),
    #[error("XMind ZIP 不允许加密 entry")]
    EncryptedArchiveEntry,
    #[error("XMind ZIP entry 路径不安全：{0}")]
    UnsafeArchivePath(String),
    #[error("XMind ZIP entry {path} 展开后 {size} bytes，超过 32 MiB 上限")]
    ArchiveEntryTooLarge { path: String, size: u64 },
    #[error("XMind ZIP entry 压缩比异常：{0}")]
    SuspiciousCompression(String),
    #[error("XMind ZIP 展开总量超过 128 MiB 上限")]
    ExpandedArchiveTooLarge,
    #[error("XMind ZIP entry 路径重复：{0}")]
    DuplicateArchivePath(String),
    #[error("XMind ZIP 缺少 content.json")]
    MissingXmindContent,
    #[error("XMind ZIP 缺少图片资源：{0}")]
    MissingArchiveResource(String),
    #[error("XMind 图片类型不受支持：{0}")]
    UnsupportedImageType(String),
    #[error("XMind SVG 资源包含不安全内容：{0}")]
    UnsafeSvg(String),
    #[error("FreeMind 禁止 DTD/entity 声明")]
    DocTypeForbidden,
    #[error("Mindmap 外部文件 node 数量超过 100000")]
    NodeLimit,
    #[error("Mindmap 外部文件 node/JSON 深度超过 256")]
    DepthLimit,
    #[error("Mindmap 外部文件单段文本超过 1 MiB")]
    TextTooLarge,
    #[error("Mindmap 外部文件中没有可导入的主题")]
    NoTopics,
    #[error("Mindmap 外部文件 external node id 重复：{0}")]
    DuplicateExternalId(String),
    #[error("Mindmap 外部文件结构无效：{0}")]
    InvalidStructure(String),
    #[error("FreeMind XML 意外结束")]
    UnexpectedEof,
    #[error("外部文件导入后的 Mindmap 无效：{0}")]
    Schema(SchemaValidationError),
}

/// Emits the existing readable structural Markdown plus an exhaustive report
/// for canonical semantics the format cannot represent.
pub fn export_markdown_with_report(
    model: &MindmapModel,
) -> Result<MindmapMarkdownExport, MindmapEngineError> {
    let text = export_markdown(model)?;
    let mut counts = BTreeMap::<&'static str, usize>::new();
    let mut add = |capability: &'static str, count: usize| {
        if count > 0 {
            *counts.entry(capability).or_default() += count;
        }
    };
    add(
        "richTextFormatting",
        model
            .nodes
            .iter()
            .filter(|node| {
                node.content
                    .as_ref()
                    .is_some_and(|content| !content.runs.is_empty())
                    || node
                        .supplement
                        .note
                        .as_ref()
                        .is_some_and(|note| !note.runs.is_empty())
            })
            .count(),
    );
    add(
        "nodeStyle",
        model
            .nodes
            .iter()
            .filter(|node| node.style != Default::default())
            .count(),
    );
    add(
        "nodeImage",
        model
            .nodes
            .iter()
            .filter(|node| node.supplement.image.is_some())
            .count(),
    );
    add(
        "nodeHyperlink",
        model
            .nodes
            .iter()
            .filter(|node| node.supplement.hyperlink.is_some())
            .count(),
    );
    add(
        "nodeMarkers",
        model
            .nodes
            .iter()
            .filter(|node| !node.supplement.markers.is_empty())
            .count(),
    );
    add(
        "collapsedState",
        model.nodes.iter().filter(|node| node.collapsed).count(),
    );
    add("explicitEdge", model.edges.len());
    add("summary", model.summaries.len());
    add("boundary", model.boundaries.len());
    add("formula", model.formulas.len());
    add(
        "extensionAttrs",
        model
            .nodes
            .iter()
            .filter(|node| !node.attrs.is_empty())
            .count()
            + model
                .edges
                .iter()
                .filter(|edge| !edge.attrs.is_empty())
                .count(),
    );
    let details = BTreeMap::from([
        ("boundary", "外框结构无法写入结构 Markdown"),
        ("collapsedState", "折叠状态不属于 Markdown 文档语义"),
        ("explicitEdge", "显式关联线无法写入树形 Markdown"),
        ("extensionAttrs", "扩展 attrs 不会写入 Markdown"),
        ("formula", "公式实体无法写入结构 Markdown"),
        ("nodeHyperlink", "节点 hyperlink 尚未映射为 Markdown link"),
        ("nodeImage", "Markdown 不携带 Artifact 二进制资产"),
        ("nodeMarkers", "节点 marker 无标准 Markdown 表达"),
        ("nodeStyle", "节点形状与颜色不属于 Markdown 语义"),
        ("richTextFormatting", "行内 RichText run 会降级为纯文本"),
        ("summary", "概要区间无法写入结构 Markdown"),
    ]);
    let unsupported = counts
        .into_iter()
        .map(|(capability, count)| MindmapExchangeLoss {
            capability: capability.into(),
            count,
            detail: details
                .get(capability)
                .copied()
                .unwrap_or("该能力无法写入 Markdown")
                .into(),
        })
        .collect();
    Ok(MindmapMarkdownExport {
        text,
        loss_report: MindmapExchangeLossReport { unsupported },
    })
}

#[derive(Debug, thiserror::Error)]
pub enum MindmapExchangeError {
    #[error("Mindmap exchange 输入 {0} bytes，超过 32 MiB 上限")]
    InputTooLarge(usize),
    #[error("Mindmap exchange JSON 无效：{0}")]
    Json(serde_json::Error),
    #[error("不支持的 Mindmap exchange format：{0}")]
    UnsupportedFormat(String),
    #[error("不支持的 Mindmap exchange version：{0}")]
    UnsupportedVersion(u32),
    #[error("不支持的 Artifact schema version：{0}")]
    UnsupportedSchemaVersion(u16),
    #[error("Mindmap exchange model 无效：{0}")]
    Schema(SchemaValidationError),
    #[error("Mindmap exchange 资产数量 {0} 超过上限")]
    TooManyAssets(usize),
    #[error("Mindmap exchange asset id 重复：{0}")]
    DuplicateAsset(String),
    #[error("Mindmap exchange 资产闭包不完整，missing={missing:?}, unused={unused:?}")]
    AssetClosure {
        missing: Vec<String>,
        unused: Vec<String>,
    },
    #[error("Mindmap exchange 资产无效：{0}")]
    InvalidAsset(String),
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use oo_schema::{
        MindmapBoundary, MindmapFormula, MindmapFormulaDisplay, MindmapImage, MindmapNode,
        MindmapSummary, RichText,
    };

    use super::*;

    fn xmind_archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (path, bytes) in entries {
            archive
                .start_file(*path, zip::write::SimpleFileOptions::default())
                .unwrap();
            archive.write_all(bytes).unwrap();
        }
        archive.finish().unwrap().into_inner()
    }

    fn text(value: &str) -> RichText {
        RichText {
            text: value.into(),
            runs: Vec::new(),
        }
    }

    fn model_with_asset() -> MindmapModel {
        MindmapModel {
            root: Some("root".into()),
            nodes: vec![
                MindmapNode {
                    id: "root".into(),
                    content: Some(text("Root")),
                    ..MindmapNode::default()
                },
                MindmapNode {
                    id: "a".into(),
                    parent_id: Some("root".into()),
                    content: Some(text("A")),
                    supplement: oo_schema::MindmapNodeSupplement {
                        image: Some(MindmapImage {
                            asset_id: "image-1".into(),
                            alt: "A".into(),
                            width: None,
                            height: None,
                        }),
                        ..Default::default()
                    },
                    ..MindmapNode::default()
                },
                MindmapNode {
                    id: "b".into(),
                    parent_id: Some("root".into()),
                    content: Some(text("B")),
                    ..MindmapNode::default()
                },
            ],
            summaries: vec![MindmapSummary {
                id: "summary".into(),
                start_node_id: "a".into(),
                end_node_id: "b".into(),
                content: text("Summary"),
            }],
            boundaries: vec![MindmapBoundary {
                id: "boundary".into(),
                root_node_id: "a".into(),
                label: None,
            }],
            formulas: vec![MindmapFormula {
                id: "formula".into(),
                node_id: "b".into(),
                source: "x^2".into(),
                display: MindmapFormulaDisplay::Inline,
            }],
            ..MindmapModel::default()
        }
    }

    fn asset() -> MindmapExchangeAsset {
        MindmapExchangeAsset {
            asset_id: "image-1".into(),
            file_name: "topic.png".into(),
            content_type: "image/png".into(),
            checksum: "a".repeat(64),
            data_base64: "aW1hZ2U=".into(),
        }
    }

    #[test]
    fn canonical_json_is_versioned_strict_and_requires_the_complete_asset_closure() {
        let envelope = MindmapExchangeEnvelope::new(model_with_asset(), vec![asset()]);
        let bytes = write_mindmap_exchange(&envelope).unwrap();
        assert_eq!(parse_mindmap_exchange(&bytes).unwrap(), envelope);

        let missing = MindmapExchangeEnvelope::new(model_with_asset(), Vec::new());
        assert!(matches!(
            missing.validate(),
            Err(MindmapExchangeError::AssetClosure { .. })
        ));

        let mut future = envelope.clone();
        future.version += 1;
        assert!(matches!(
            future.validate(),
            Err(MindmapExchangeError::UnsupportedVersion(_))
        ));

        let mut value = serde_json::to_value(envelope).unwrap();
        value
            .as_object_mut()
            .unwrap()
            .insert("revision".into(), 3.into());
        assert!(matches!(
            parse_mindmap_exchange(&serde_json::to_vec(&value).unwrap()),
            Err(MindmapExchangeError::Json(_))
        ));
    }

    #[test]
    fn markdown_reports_every_unrepresentable_advanced_or_asset_semantic() {
        let exported = export_markdown_with_report(&model_with_asset()).unwrap();
        assert!(exported.text.contains("# Root"));
        let capabilities = exported
            .loss_report
            .unsupported
            .iter()
            .map(|loss| loss.capability.as_str())
            .collect::<BTreeSet<_>>();
        assert!(capabilities.contains("nodeImage"));
        assert!(capabilities.contains("summary"));
        assert!(capabilities.contains("boundary"));
        assert!(capabilities.contains("formula"));
        assert_eq!(
            exported.loss_report.header_summary().as_deref(),
            Some("boundary:1,formula:1,nodeImage:1,summary:1")
        );
    }

    #[test]
    fn svg_is_structured_standalone_escaped_and_contains_advanced_projection() {
        let mut model = model_with_asset();
        model.nodes[0].content = Some(text("Root <unsafe> & visible"));
        model.nodes[2].collapsed = true;
        model.edges.push(oo_schema::MindmapEdge {
            id: "edge".into(),
            source_id: "a".into(),
            target_id: "b".into(),
            label: Some(text("depends <on>")),
            ..Default::default()
        });
        let exported = export_svg(&model, &[asset()]).unwrap();
        assert!(exported.text.starts_with("<svg xmlns="));
        assert!(exported.text.contains("viewBox=\"0 0 "));
        assert!(exported.text.contains("Root &lt;unsafe&gt; &amp; visible"));
        assert!(!exported.text.contains("Root <unsafe>"));
        for kind in [
            "node",
            "node-image",
            "tree-edge",
            "explicit-edge",
            "edge-label",
            "collapsed",
            "summary",
            "boundary",
            "formula",
        ] {
            assert!(
                exported.text.contains(&format!("data-kind=\"{kind}\"")),
                "SVG must contain {kind}"
            );
        }
        assert!(exported.text.contains("data:image/png;base64,aW1hZ2U="));
        assert!(!exported.text.contains("selection"));
        assert!(!exported.text.contains("presence"));

        let mut reader = Reader::from_str(&exported.text);
        let mut buffer = Vec::new();
        loop {
            match reader.read_event_into(&mut buffer).unwrap() {
                Event::Eof => break,
                _ => buffer.clear(),
            }
        }
    }

    #[test]
    fn svg_rejects_a_missing_image_asset() {
        assert!(matches!(
            export_svg(&model_with_asset(), &[]),
            Err(MindmapSvgExportError::MissingAsset(asset_id)) if asset_id == "image-1"
        ));
    }

    #[test]
    #[cfg(feature = "pdf-export")]
    fn pdf_fit_is_vector_parseable_and_keeps_extractable_cjk_text() {
        let mut model = model_with_asset();
        model.nodes[0].content = Some(text("产品路线图 Root"));
        model.nodes[2].content = Some(text("Emoji 😀"));
        model.nodes[1].supplement.image = None;
        let svg = export_svg(&model, &[]).unwrap();
        let exported = export_pdf(&svg.text, MindmapPdfOptions::default()).unwrap();
        assert_eq!(exported.page_count, 1);
        assert!(exported.bytes.starts_with(b"%PDF-"));
        let document = lopdf::Document::load_mem(&exported.bytes).unwrap();
        assert_eq!(document.get_pages().len(), 1);
        let extracted = pdf_extract::extract_text_from_mem(&exported.bytes).unwrap();
        let compact = extracted
            .chars()
            .filter(|character| !character.is_whitespace())
            .collect::<String>();
        assert!(compact.contains("Root"), "extracted text: {extracted:?}");
        assert!(
            compact.contains("产品路线图"),
            "extracted text: {extracted:?}"
        );
        assert!(compact.contains('😀'), "extracted text: {extracted:?}");
    }

    #[test]
    #[cfg(feature = "pdf-export")]
    fn pdf_tile_has_stable_bounded_page_grid() {
        let mut model = MindmapModel {
            root: Some("root".into()),
            ..Default::default()
        };
        model.nodes.push(MindmapNode {
            id: "root".into(),
            content: Some(text("Root")),
            ..Default::default()
        });
        for index in 0..40 {
            model.nodes.push(MindmapNode {
                id: format!("node-{index}"),
                parent_id: Some("root".into()),
                content: Some(text(&format!("Node {index}"))),
                ..Default::default()
            });
        }
        let svg = export_svg(&model, &[]).unwrap();
        let exported = export_pdf(
            &svg.text,
            MindmapPdfOptions {
                mode: MindmapPdfMode::Tile,
                ..Default::default()
            },
        )
        .unwrap();
        assert!(exported.page_count > 1);
        assert!(exported.page_count <= 64);
        let document = lopdf::Document::load_mem(&exported.bytes).unwrap();
        assert_eq!(document.get_pages().len(), exported.page_count);
    }

    #[test]
    fn freemind_stream_import_maps_tree_notes_links_colors_and_arrowlinks() {
        let imported = import_freemind(
            r##"<?xml version="1.0"?>
            <map version="1.0.1">
              <node ID="ROOT" TEXT="产品 😀" LINK="https://example.com" COLOR="#112233" BACKGROUND_COLOR="#ddeeff">
                <richcontent TYPE="NOTE"><html><body><p>第一行</p><p>Second</p></body></html></richcontent>
                <node ID="A" TEXT="分支 A" FOLDED="true"><arrowlink DESTINATION="B"/><icon BUILTIN="idea"/></node>
                <node ID="B" TEXT="分支 B"/>
              </node>
            </map>"##
                .as_bytes(),
        )
        .unwrap();
        assert_eq!(imported.model.nodes.len(), 3);
        assert_eq!(
            imported.model.nodes[0].content.as_ref().unwrap().text,
            "产品 😀"
        );
        assert_eq!(
            imported.model.nodes[0]
                .supplement
                .note
                .as_ref()
                .unwrap()
                .text,
            "第一行\nSecond"
        );
        assert_eq!(
            imported.model.nodes[0].supplement.hyperlink.as_deref(),
            Some("https://example.com")
        );
        assert_eq!(
            imported.model.nodes[0].style.text_color.as_deref(),
            Some("#112233")
        );
        assert!(imported.model.nodes[1].collapsed);
        assert_eq!(imported.model.edges.len(), 1);
        assert_eq!(imported.model.edges[0].source_id, "node-2");
        assert_eq!(imported.model.edges[0].target_id, "node-3");
        assert_eq!(
            imported.loss_report.header_summary().as_deref(),
            Some("unsupportedFeature:1")
        );
    }

    #[test]
    fn freemind_rejects_dtd_duplicate_ids_and_depth_overflow() {
        assert!(matches!(
            import_freemind(br#"<!DOCTYPE map [<!ENTITY x "boom">]><map><node TEXT="&x;"/></map>"#),
            Err(MindmapExternalImportError::DocTypeForbidden)
        ));
        assert!(matches!(
            import_freemind(
                br#"<map><node ID="same" TEXT="A"><node ID="same" TEXT="B"/></node></map>"#
            ),
            Err(MindmapExternalImportError::DuplicateExternalId(_))
        ));
        let mut deep = String::from("<map>");
        for index in 0..=MAX_MINDMAP_IMPORT_DEPTH {
            deep.push_str(&format!("<node ID=\"n{index}\" TEXT=\"N\">"));
        }
        for _ in 0..=MAX_MINDMAP_IMPORT_DEPTH {
            deep.push_str("</node>");
        }
        deep.push_str("</map>");
        assert!(matches!(
            import_freemind(deep.as_bytes()),
            Err(MindmapExternalImportError::DepthLimit)
        ));
    }

    #[test]
    fn xmind_import_maps_tree_notes_markers_images_and_relationships() {
        let content = include_bytes!("../../../fixtures/mindmap/xmind/content.json");
        let image = b"test-png-payload";
        let imported = import_xmind(&xmind_archive(&[
            ("content.json", content),
            ("resources/topic.png", image),
        ]))
        .unwrap();

        assert_eq!(imported.model.nodes.len(), 3);
        let root = &imported.model.nodes[0];
        assert_eq!(root.content.as_ref().unwrap().text, "产品路线图");
        assert_eq!(root.supplement.note.as_ref().unwrap().text, "共享说明");
        assert_eq!(root.supplement.markers, ["priority-1"]);
        let embedded = root.supplement.image.as_ref().unwrap();
        assert_eq!(embedded.asset_id, "asset-1");
        assert_eq!(embedded.width, Some(120.0));
        assert_eq!(embedded.height, Some(80.0));
        assert!(imported.model.nodes[1].collapsed);
        assert_eq!(imported.model.edges.len(), 1);
        assert_eq!(imported.model.edges[0].source_id, "node-2");
        assert_eq!(imported.model.edges[0].target_id, "node-3");
        assert_eq!(imported.model.edges[0].label.as_ref().unwrap().text, "依赖");
        assert_eq!(imported.assets.len(), 1);
        assert_eq!(imported.assets[0].file_name, "topic.png");
        assert_eq!(imported.assets[0].content_type, "image/png");
        assert_eq!(imported.assets[0].bytes, image);
        assert_eq!(imported.loss_report.header_summary(), None);
    }

    #[test]
    fn xmind_rejects_missing_content_and_unsafe_svg() {
        assert!(matches!(
            import_xmind(&xmind_archive(&[("manifest.json", b"{}")])),
            Err(MindmapExternalImportError::MissingXmindContent)
        ));
        let content = br#"[{"rootTopic":{"id":"root","title":"Root","image":{"src":"xap:resources/bad.svg"}}}]"#;
        let unsafe_svg =
            br#"<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>"#;
        assert!(matches!(
            import_xmind(&xmind_archive(&[
                ("content.json", content.as_slice()),
                ("resources/bad.svg", unsafe_svg.as_slice()),
            ])),
            Err(MindmapExternalImportError::UnsafeSvg(path)) if path == "resources/bad.svg"
        ));
    }
}
