//! PPTX adapter for the canonical v5 [`Deck`](oo_schema::presentation_v5::Deck).
//!
//! This is intentionally an adapter, not a second scene model.  The P1 cutover preserves a
//! minimal but honest interchange path (slides plus textual shapes).  Features not modelled by
//! this adapter are returned in a loss report by inspection rather than silently converted to a
//! generic attribute bag.

use std::{
    collections::{BTreeMap, HashMap},
    io::{Cursor, Read, Write},
    path::{Component, Path},
};

use oo_schema::presentation_v5::{
    AnimationEntry, AnimationPreset, AnimationTrigger, AssetRef, ColorRef, ConnectorEndpoint,
    ConnectorNode, Deck, DeckTheme, GroupNode, HorizontalAlign, ImageNode, Insets, NodeTransform,
    Paint, Point, PresentationListStyle, PresentationParagraph, PresentationRichText,
    PresentationTextRun, PresentationTextStyle, Rgba, SceneNode, SceneNodeKind, ShapeGeometry,
    ShapeNode, ShapeStyle, Slide, SlideBackground, SlideLayout, SlideMaster, SlideTransition,
    TableCell, TableCellStyle, TableNode, TextAutoFit, TextFrame, TextHorizontalAlign, TextNode,
    TextVerticalAlign, ThemeColorToken, ThemeFontToken, Timeline, TransitionKind,
};
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;

const PRESENTATION_PART: &str = "ppt/presentation.xml";
const EMU_PER_POINT: f64 = 12_700.0;
const MAX_ARCHIVE_ENTRIES: usize = 4_096;
const MAX_ARCHIVE_UNCOMPRESSED_BYTES: u64 = 256 * 1024 * 1024;
const MAX_XML_PART_BYTES: usize = 16 * 1024 * 1024;
const MAX_MEDIA_PART_BYTES: usize = 64 * 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum PptxError {
    #[error("不是有效的 PPTX zip 包：{0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("PPTX XML 解析失败：{0}")]
    Xml(#[from] quick_xml::Error),
    #[error("读取 PPTX 部件失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("缺少必需的 PPTX 部件 {0}")]
    MissingPart(&'static str),
    #[error("PPTX 结构无效：{0}")]
    InvalidStructure(String),
    #[error("Presentation schema 校验失败：{0}")]
    Schema(#[from] oo_schema::SchemaValidationError),
    #[error("PPTX 导出会产生未映射内容：{0}")]
    LossyExport(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PptxReportKind {
    Supported,
    Lossy,
    Unsupported,
    Invalid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PptxUnsupported {
    pub kind: PptxReportKind,
    pub capability: String,
    pub part: String,
    pub detail: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PptxLossReport {
    pub unsupported: Vec<PptxUnsupported>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct PptxImportResult {
    pub deck: Deck,
    pub loss_report: PptxLossReport,
    /// The adapter keeps imported binary assets beside the canonical Deck so the caller can send
    /// them through the Artifact asset service. They are never embedded in Deck or XML attrs.
    pub assets: Vec<PptxImportedAsset>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PptxImportedAsset {
    pub asset: AssetRef,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PptxExportResult {
    pub bytes: Vec<u8>,
    pub loss_report: PptxLossReport,
}

/// A comparison of the *interchange semantics* of two decks.
///
/// PPTX package paths, OOXML relationship ids and canonical node ids are deliberately excluded:
/// they are transport identities, not presentation semantics.  The comparison covers the subset
/// that this adapter can write without a loss report (page geometry, slide order, node tree,
/// transforms, basic text/shape/image/group data and referenced asset digests).  Features outside
/// that subset must appear in [`PptxLossReport`] before a strict export is allowed.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PptxSemanticDiff {
    pub differences: Vec<PptxSemanticDifference>,
}

impl PptxSemanticDiff {
    pub fn is_equivalent(&self) -> bool {
        self.differences.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PptxSemanticDifference {
    /// Stable semantic path, never a ZIP part or relationship id.
    pub path: String,
    pub expected: String,
    pub actual: String,
}

/// Binary asset bytes are intentionally supplied separately from `Deck`. The canonical model
/// stores stable metadata only; an Artifact asset service owns storage and authorization.
pub type PptxAssetSource = BTreeMap<String, Vec<u8>>;

/// Compare two decks after an import/export/import cycle.
///
/// This is intentionally narrower than `Deck::PartialEq`: canonical ids are allowed to change
/// at an OOXML boundary, while user-visible tree structure and values may not.  Callers must
/// inspect the export/import loss reports first; a non-empty report means this comparison is not
/// a proof of full-deck fidelity.
pub fn semantic_diff(expected: &Deck, actual: &Deck) -> PptxSemanticDiff {
    let mut differences = Vec::new();
    compare_json(
        &mut differences,
        "pageSpec",
        &serde_json::json!({
            "width": expected.page_spec.width,
            "height": expected.page_spec.height,
            "unit": expected.page_spec.unit,
        }),
        &serde_json::json!({
            "width": actual.page_spec.width,
            "height": actual.page_spec.height,
            "unit": actual.page_spec.unit,
        }),
    );

    if expected.slides.len() != actual.slides.len() {
        differences.push(PptxSemanticDifference {
            path: "slides.length".into(),
            expected: expected.slides.len().to_string(),
            actual: actual.slides.len().to_string(),
        });
    }

    for (index, (expected_slide, actual_slide)) in
        expected.slides.iter().zip(actual.slides.iter()).enumerate()
    {
        let path = format!("slides[{index}]");
        compare_json(
            &mut differences,
            &format!("{path}.name"),
            &expected_slide.name,
            &actual_slide.name,
        );
        compare_json(
            &mut differences,
            &format!("{path}.background"),
            &expected_slide.background,
            &actual_slide.background,
        );
        compare_json(
            &mut differences,
            &format!("{path}.notes"),
            &expected_slide.notes,
            &actual_slide.notes,
        );
        compare_json(
            &mut differences,
            &format!("{path}.transition"),
            &expected_slide.transition,
            &actual_slide.transition,
        );
        compare_json(
            &mut differences,
            &format!("{path}.timeline"),
            &semantic_timeline(expected_slide),
            &semantic_timeline(actual_slide),
        );
        compare_json(
            &mut differences,
            &format!("{path}.nodes"),
            &semantic_slide_nodes(expected_slide, expected),
            &semantic_slide_nodes(actual_slide, actual),
        );
    }

    PptxSemanticDiff { differences }
}

/// Inspect package-level hazards and capabilities before parsing.  This is intentionally
/// conservative: macros, OLE packages, external relationships and unknown binary payloads are
/// reported instead of entering canonical scene state.
pub fn inspect_pptx(bytes: &[u8]) -> Result<PptxLossReport, PptxError> {
    let mut archive = open_archive(bytes)?;
    let mut report = PptxLossReport::default();
    for index in 0..archive.len() {
        let name = {
            let part = archive.by_index(index)?;
            part.name().to_owned()
        };
        if name.contains("../") || name.starts_with('/') {
            return Err(PptxError::InvalidStructure(format!(
                "ZIP part 路径非法：{name}"
            )));
        }
        if name.ends_with(".rels") {
            let xml = read_part(&mut archive, &name, MAX_XML_PART_BYTES)?;
            inspect_relationships(&xml, &name, &mut report)?;
        } else if name.ends_with("vbaProject.bin") {
            report.unsupported.push(report_item(
                PptxReportKind::Unsupported,
                "macro",
                name,
                "宏不会导入或导出到 canonical Deck",
                Some("移除宏后重新导入，或保留原始文件作为 source asset"),
            ));
        } else if name.starts_with("ppt/embeddings/") {
            report.unsupported.push(report_item(
                PptxReportKind::Unsupported,
                "ole",
                name,
                "OLE/嵌入对象未映射到 v5 typed node",
                Some("使用安全的 Embed capability 单独接入"),
            ));
        } else if name.starts_with("ppt/diagrams/") {
            report.unsupported.push(report_item(
                PptxReportKind::Unsupported,
                "smartArt",
                name,
                "SmartArt 没有转换为可编辑节点",
                Some("转换为基础形状后重新导入"),
            ));
        }
    }
    Ok(report)
}

pub fn parse_pptx_with_report(bytes: &[u8]) -> Result<PptxImportResult, PptxError> {
    let mut report = inspect_pptx(bytes)?;
    let mut archive = open_archive(bytes)?;
    if archive.by_name(PRESENTATION_PART).is_err() {
        return Err(PptxError::MissingPart(PRESENTATION_PART));
    }
    let presentation_xml = read_part(&mut archive, PRESENTATION_PART, MAX_XML_PART_BYTES)?;
    let presentation_rels = relationships_for(&mut archive, PRESENTATION_PART, &mut report)?;
    let slide_parts = presentation_slide_parts(&presentation_xml, &presentation_rels)?;
    let mut deck = Deck {
        page_spec: parse_page_spec(&presentation_xml)?,
        theme: DeckTheme {
            id: "pptx-default".into(),
            ..DeckTheme::default()
        },
        ..Deck::default()
    };

    import_masters_layouts_theme(&mut archive, &presentation_rels, &mut deck, &mut report)?;
    let mut imported_assets = BTreeMap::<String, PptxImportedAsset>::new();
    for (slide_index, part) in slide_parts.iter().enumerate() {
        let xml = read_relationship_part(&mut archive, part, MAX_XML_PART_BYTES)?;
        let rels = relationships_for(&mut archive, part, &mut report)?;
        report_unmapped_slide_relationships(part, &rels, &mut report);
        let layout_id = rels
            .values()
            .find(|relationship| relationship.kind.ends_with("/slideLayout"))
            .and_then(|relationship| layout_id_from_part(&relationship.target));
        let notes = rels
            .values()
            .find(|relationship| relationship.kind.ends_with("/notesSlide"))
            .map(|relationship| parse_notes(&mut archive, &relationship.target, &mut report))
            .transpose()?
            .flatten();
        let mut image_assets = HashMap::new();
        for (relationship_id, relationship) in &rels {
            if relationship.kind.ends_with("/image") {
                let imported = read_import_asset(&mut archive, relationship)?;
                image_assets.insert(relationship_id.clone(), imported.asset.clone());
                imported_assets
                    .entry(imported.asset.asset_id.clone())
                    .or_insert(imported);
            }
        }
        let mut slide = parse_slide(&xml, slide_index, &rels, &image_assets, &mut report)?;
        slide.id = stable_part_id("slide", part);
        slide.order_key = format!("{slide_index:08}");
        slide.layout_id = layout_id;
        slide.notes = notes;
        deck.slides.push(slide);
    }
    deck.assets = imported_assets
        .values()
        .map(|value| value.asset.clone())
        .collect();
    deck.validate()?;
    Ok(PptxImportResult {
        deck,
        loss_report: report,
        assets: imported_assets.into_values().collect(),
    })
}

/// Strict import for callers that must reject any lossy/unsupported part.
pub fn parse_pptx(bytes: &[u8]) -> Result<Deck, PptxError> {
    let imported = parse_pptx_with_report(bytes)?;
    require_lossless(&imported.loss_report, "导入")?;
    Ok(imported.deck)
}

/// Export with an explicit report.  Image bytes must be provided by the Artifact asset service;
/// missing bytes are reported and not silently replaced with a URL or data URI.
pub fn write_pptx_with_assets(
    deck: &Deck,
    assets: &PptxAssetSource,
) -> Result<PptxExportResult, PptxError> {
    deck.validate()?;
    let mut report = PptxLossReport::default();
    let mut out = Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(&mut out);
    let options = SimpleFileOptions::default();
    let package = ExportPackage::build(deck, assets, &mut report)?;
    write_part(
        &mut zip,
        "[Content_Types].xml",
        &package.content_types,
        options,
    )?;
    write_part(&mut zip, "_rels/.rels", ROOT_RELS, options)?;
    for (name, body) in &package.xml_parts {
        write_part(&mut zip, name, body, options)?;
    }
    for (name, body) in &package.binary_parts {
        zip.start_file(name, options)?;
        zip.write_all(body)?;
    }
    zip.finish()?;
    Ok(PptxExportResult {
        bytes: out.into_inner(),
        loss_report: report,
    })
}

pub fn write_pptx_with_report(deck: &Deck) -> Result<PptxExportResult, PptxError> {
    write_pptx_with_assets(deck, &PptxAssetSource::default())
}

/// Strict writer used by the server export path: never quietly emit a deck with dropped data.
pub fn write_pptx(deck: &Deck) -> Result<Vec<u8>, PptxError> {
    let exported = write_pptx_with_report(deck)?;
    require_lossless(&exported.loss_report, "导出")?;
    Ok(exported.bytes)
}

fn write_part<W: Write + std::io::Seek>(
    zip: &mut zip::ZipWriter<W>,
    name: &str,
    body: &str,
    options: SimpleFileOptions,
) -> Result<(), PptxError> {
    zip.start_file(name, options)?;
    zip.write_all(body.as_bytes())?;
    Ok(())
}

fn report_item(
    kind: PptxReportKind,
    capability: impl Into<String>,
    part: impl Into<String>,
    detail: impl Into<String>,
    suggestion: Option<&str>,
) -> PptxUnsupported {
    PptxUnsupported {
        kind,
        capability: capability.into(),
        part: part.into(),
        detail: detail.into(),
        suggestion: suggestion.map(str::to_owned),
    }
}

fn require_lossless(report: &PptxLossReport, operation: &str) -> Result<(), PptxError> {
    if report.unsupported.is_empty() {
        return Ok(());
    }
    let summary = report
        .unsupported
        .iter()
        .map(|item| format!("{} ({})", item.capability, item.part))
        .collect::<Vec<_>>()
        .join(", ");
    Err(PptxError::LossyExport(format!("{operation}：{summary}")))
}

fn compare_json<T: Serialize>(
    differences: &mut Vec<PptxSemanticDifference>,
    path: &str,
    expected: &T,
    actual: &T,
) {
    let expected = serde_json::to_string(expected).expect("semantic values are serializable");
    let actual = serde_json::to_string(actual).expect("semantic values are serializable");
    if expected != actual {
        differences.push(PptxSemanticDifference {
            path: path.to_owned(),
            expected,
            actual,
        });
    }
}

fn semantic_slide_nodes(slide: &Slide, deck: &Deck) -> Vec<serde_json::Value> {
    let assets = deck
        .assets
        .iter()
        .map(|asset| (asset.asset_id.as_str(), asset))
        .collect::<HashMap<_, _>>();
    semantic_children(None, &slide.nodes, &assets)
}

fn semantic_timeline(slide: &Slide) -> Vec<serde_json::Value> {
    let mut entries = slide.timeline.entries.iter().collect::<Vec<_>>();
    entries.sort_by(|left, right| left.order_key.cmp(&right.order_key));
    entries
        .into_iter()
        .map(|entry| {
            serde_json::json!({
                "target": semantic_node_path(slide, &entry.target_node_id),
                "trigger": entry.trigger,
                "preset": entry.preset,
                "durationMs": entry.duration_ms,
                "delayMs": entry.delay_ms,
            })
        })
        .collect()
}

fn semantic_node_path(slide: &Slide, target_id: &str) -> Option<String> {
    fn find(
        nodes: &[SceneNode],
        parent_id: Option<&str>,
        target_id: &str,
        prefix: &str,
    ) -> Option<String> {
        let mut siblings = nodes
            .iter()
            .filter(|node| node.parent_id.as_deref() == parent_id)
            .collect::<Vec<_>>();
        siblings.sort_by(|left, right| left.order_key.cmp(&right.order_key));
        for (index, node) in siblings.into_iter().enumerate() {
            let path = if prefix.is_empty() {
                index.to_string()
            } else {
                format!("{prefix}.{index}")
            };
            if node.id == target_id {
                return Some(path);
            }
            if let Some(found) = find(nodes, Some(&node.id), target_id, &path) {
                return Some(found);
            }
        }
        None
    }
    find(&slide.nodes, None, target_id, "")
}

fn semantic_children(
    parent_id: Option<&str>,
    nodes: &[SceneNode],
    assets: &HashMap<&str, &AssetRef>,
) -> Vec<serde_json::Value> {
    let mut siblings = nodes
        .iter()
        .filter(|node| node.parent_id.as_deref() == parent_id)
        .collect::<Vec<_>>();
    siblings.sort_by(|left, right| left.order_key.cmp(&right.order_key));
    siblings
        .into_iter()
        .map(|node| {
            serde_json::json!({
                "node": semantic_node_value(node, assets),
                "children": semantic_children(Some(&node.id), nodes, assets),
            })
        })
        .collect()
}

fn semantic_node_value(node: &SceneNode, assets: &HashMap<&str, &AssetRef>) -> serde_json::Value {
    let kind = match &node.kind {
        SceneNodeKind::Text(text) => serde_json::json!({ "type": "text", "data": text }),
        SceneNodeKind::Shape(shape) => serde_json::json!({ "type": "shape", "data": shape }),
        SceneNodeKind::Group(_) => serde_json::json!({ "type": "group" }),
        SceneNodeKind::Image(image) => serde_json::json!({
            "type": "image",
            "assetDigest": asset_digest(assets, &image.asset_id),
            "originalAssetDigest": image
                .original_asset_id
                .as_deref()
                .map(|id| asset_digest(assets, id)),
            "crop": image.crop,
            "flipH": image.flip_h,
            "flipV": image.flip_v,
            "caption": image.caption,
        }),
        SceneNodeKind::Table(table) => serde_json::json!({ "type": "table", "data": table }),
        SceneNodeKind::Connector(connector) => {
            serde_json::json!({ "type": "connector", "data": connector })
        }
        // The writer reports these variants as unsupported. Keeping their typed values in the
        // diff still makes a caller's accidental lossy round-trip immediately visible.
        other => serde_json::json!({ "type": "unsupported", "data": other }),
    };
    serde_json::json!({
        // OOXML writers commonly materialize an empty `name` attribute.  Empty and absent are
        // equivalent in the canonical model, unlike a non-empty accessible object name.
        "name": node.name.as_deref().filter(|name| !name.is_empty()),
        "altText": node.alt_text,
        "layoutPlaceholderId": node.layout_placeholder_id,
        "transform": node.transform,
        "visible": node.visible,
        "locked": node.locked,
        "opacity": node.opacity,
        "kind": kind,
    })
}

fn asset_digest(assets: &HashMap<&str, &AssetRef>, asset_id: &str) -> String {
    assets
        .get(asset_id)
        .map(|asset| asset.digest.clone())
        .unwrap_or_else(|| format!("missing:{asset_id}"))
}

fn open_archive(bytes: &[u8]) -> Result<zip::ZipArchive<Cursor<&[u8]>>, PptxError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    if archive.len() > MAX_ARCHIVE_ENTRIES {
        return Err(PptxError::InvalidStructure(format!(
            "PPTX ZIP entry 数量超过限制 {MAX_ARCHIVE_ENTRIES}"
        )));
    }
    let mut uncompressed = 0u64;
    for index in 0..archive.len() {
        let part = archive.by_index(index)?;
        uncompressed = uncompressed.saturating_add(part.size());
        if uncompressed > MAX_ARCHIVE_UNCOMPRESSED_BYTES {
            return Err(PptxError::InvalidStructure(format!(
                "PPTX 解压总大小超过限制 {MAX_ARCHIVE_UNCOMPRESSED_BYTES}"
            )));
        }
    }
    Ok(archive)
}

fn read_part<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
    max_bytes: usize,
) -> Result<Vec<u8>, PptxError> {
    let mut part = archive.by_name(name)?;
    let size = part.size();
    if size > max_bytes as u64 || size > MAX_ARCHIVE_UNCOMPRESSED_BYTES {
        return Err(PptxError::InvalidStructure(format!(
            "PPTX part {name} 解压后大小超限"
        )));
    }
    let mut body = Vec::with_capacity(size as usize);
    part.read_to_end(&mut body)?;
    if body.len() > max_bytes {
        return Err(PptxError::InvalidStructure(format!(
            "PPTX part {name} 读取超限"
        )));
    }
    Ok(body)
}

fn read_relationship_part<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    target: &str,
    max_bytes: usize,
) -> Result<Vec<u8>, PptxError> {
    match read_part(archive, target, max_bytes) {
        Err(PptxError::Zip(zip::result::ZipError::FileNotFound)) => Err(
            PptxError::InvalidStructure(format!("relationship target 不存在：{target}")),
        ),
        result => result,
    }
}

#[derive(Debug, Clone)]
struct Relationship {
    kind: String,
    target: String,
}

fn local_name(name: &[u8]) -> &[u8] {
    name.rsplit(|byte| *byte == b':').next().unwrap_or(name)
}

fn attr(event: &quick_xml::events::BytesStart<'_>, name: &[u8]) -> Option<String> {
    event.attributes().flatten().find_map(|attribute| {
        if local_name(attribute.key.as_ref()) != name {
            return None;
        }
        let raw = String::from_utf8_lossy(attribute.value.as_ref());
        quick_xml::escape::unescape(&raw)
            .ok()
            .map(|value| value.into_owned())
    })
}

fn relationship_id_attr(event: &quick_xml::events::BytesStart<'_>) -> Option<String> {
    let attributes = event.attributes().flatten().collect::<Vec<_>>();
    attributes
        .iter()
        .find(|attribute| attribute.key.as_ref() == b"r:id")
        .or_else(|| {
            attributes.iter().find(|attribute| {
                attribute.key.as_ref() == b"id" && attribute.value.as_ref().starts_with(b"rId")
            })
        })
        .map(|attribute| String::from_utf8_lossy(attribute.value.as_ref()).into_owned())
}

fn relationship_part(source_part: &str) -> Result<String, PptxError> {
    let source = Path::new(source_part);
    let file_name = source
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| PptxError::InvalidStructure(format!("无效 source part {source_part}")))?;
    let parent = source.parent().unwrap_or_else(|| Path::new(""));
    Ok(parent
        .join("_rels")
        .join(format!("{file_name}.rels"))
        .to_string_lossy()
        .replace('\\', "/"))
}

fn normalize_target(source_part: &str, target: &str) -> Result<String, PptxError> {
    if target.contains("://") || target.starts_with('/') || target.starts_with('\\') {
        return Err(PptxError::InvalidStructure(format!(
            "禁止外部或绝对 OOXML relationship target：{target}"
        )));
    }
    let parent = Path::new(source_part)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let joined = parent.join(target);
    let mut normal = std::path::PathBuf::new();
    for component in joined.components() {
        match component {
            Component::Normal(value) => normal.push(value),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normal.pop() {
                    return Err(PptxError::InvalidStructure(format!(
                        "relationship target 越界：{target}"
                    )));
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(PptxError::InvalidStructure(format!(
                    "非法 relationship target：{target}"
                )))
            }
        }
    }
    let normalized = normal.to_string_lossy().replace('\\', "/");
    if normalized.is_empty() || normalized.starts_with("../") {
        return Err(PptxError::InvalidStructure(format!(
            "非法 relationship target：{target}"
        )));
    }
    Ok(normalized)
}

fn parse_relationships(
    xml: &[u8],
    source_part: &str,
) -> Result<HashMap<String, Relationship>, PptxError> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut relations = HashMap::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Empty(event) | Event::Start(event)
                if local_name(event.name().as_ref()) == b"Relationship" =>
            {
                let id = attr(&event, b"Id").ok_or_else(|| {
                    PptxError::InvalidStructure(format!("{source_part} relationship 缺少 Id"))
                })?;
                let kind = attr(&event, b"Type").ok_or_else(|| {
                    PptxError::InvalidStructure(format!("{source_part} relationship 缺少 Type"))
                })?;
                if attr(&event, b"TargetMode").as_deref() == Some("External") {
                    continue;
                }
                let target = attr(&event, b"Target").ok_or_else(|| {
                    PptxError::InvalidStructure(format!("{source_part} relationship 缺少 Target"))
                })?;
                let target = normalize_target(source_part, &target)?;
                if relations
                    .insert(id.clone(), Relationship { kind, target })
                    .is_some()
                {
                    return Err(PptxError::InvalidStructure(format!(
                        "{source_part} relationship Id 重复：{id}"
                    )));
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(relations)
}

fn inspect_relationships(
    xml: &[u8],
    part: &str,
    report: &mut PptxLossReport,
) -> Result<(), PptxError> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Empty(event) | Event::Start(event)
                if local_name(event.name().as_ref()) == b"Relationship" =>
            {
                if attr(&event, b"TargetMode").as_deref() == Some("External") {
                    report.unsupported.push(report_item(
                        PptxReportKind::Unsupported,
                        "externalRelationship",
                        part,
                        "外部 relationship 不会作为可编辑 Deck 状态导入",
                        Some("下载并导入受信任的本地资源"),
                    ));
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(())
}

fn relationships_for<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    source_part: &str,
    report: &mut PptxLossReport,
) -> Result<HashMap<String, Relationship>, PptxError> {
    let part = relationship_part(source_part)?;
    let xml = match read_part(archive, &part, MAX_XML_PART_BYTES) {
        Ok(value) => value,
        Err(PptxError::Zip(zip::result::ZipError::FileNotFound)) => return Ok(HashMap::new()),
        Err(error) => return Err(error),
    };
    inspect_relationships(&xml, &part, report)?;
    parse_relationships(&xml, source_part)
}

/// A relationship is not canonical state by itself.  Every slide relationship therefore needs a
/// typed importer or a report entry; silently ignoring an OOXML relationship is data loss.
fn report_unmapped_slide_relationships(
    slide_part: &str,
    relationships: &HashMap<String, Relationship>,
    report: &mut PptxLossReport,
) {
    for relationship in relationships.values() {
        if relationship.kind.ends_with("/image")
            || relationship.kind.ends_with("/slideLayout")
            || relationship.kind.ends_with("/notesSlide")
        {
            continue;
        }
        let capability = if relationship.kind.ends_with("/audio")
            || relationship.kind.ends_with("/video")
            || relationship.kind.ends_with("/media")
        {
            "media"
        } else if relationship.kind.ends_with("/chart") {
            "chart"
        } else if relationship.kind.ends_with("/oleObject")
            || relationship.kind.ends_with("/package")
        {
            "embeddedObject"
        } else if relationship.kind.ends_with("/hyperlink") {
            "hyperlink"
        } else {
            "slideRelationship"
        };
        report.unsupported.push(report_item(
            PptxReportKind::Unsupported,
            capability,
            slide_part,
            format!(
                "relationship {} -> {} 没有 typed Deck 映射",
                relationship.kind, relationship.target
            ),
            Some("保留原始 PPTX source asset，或等待对应 typed adapter"),
        ));
    }
}

fn presentation_slide_parts(
    xml: &[u8],
    relationships: &HashMap<String, Relationship>,
) -> Result<Vec<String>, PptxError> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut parts = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Empty(event) | Event::Start(event)
                if local_name(event.name().as_ref()) == b"sldId" =>
            {
                let id = relationship_id_attr(&event).ok_or_else(|| {
                    PptxError::InvalidStructure("presentation slide 缺少 r:id".into())
                })?;
                let relationship = relationships.get(&id).ok_or_else(|| {
                    PptxError::InvalidStructure(format!(
                        "presentation 缺少 slide relationship {id}"
                    ))
                })?;
                if !relationship.kind.ends_with("/slide") {
                    return Err(PptxError::InvalidStructure(format!(
                        "relationship {id} 不是 slide"
                    )));
                }
                parts.push(relationship.target.clone());
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    if parts.is_empty() {
        return Err(PptxError::InvalidStructure(
            "presentation 未声明 slide 关系".into(),
        ));
    }
    Ok(parts)
}

fn parse_page_spec(xml: &[u8]) -> Result<oo_schema::presentation_v5::SlidePageSpec, PptxError> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Empty(event) | Event::Start(event)
                if local_name(event.name().as_ref()) == b"sldSz" =>
            {
                let width = attr(&event, b"cx")
                    .ok_or_else(|| {
                        PptxError::InvalidStructure("presentation sldSz 缺少 cx".into())
                    })?
                    .parse()
                    .map_err(|_| {
                        PptxError::InvalidStructure("presentation sldSz cx 非法".into())
                    })?;
                let height = attr(&event, b"cy")
                    .ok_or_else(|| {
                        PptxError::InvalidStructure("presentation sldSz 缺少 cy".into())
                    })?
                    .parse()
                    .map_err(|_| {
                        PptxError::InvalidStructure("presentation sldSz cy 非法".into())
                    })?;
                return Ok(oo_schema::presentation_v5::SlidePageSpec {
                    width,
                    height,
                    unit: oo_schema::presentation_v5::PageUnit::Emu,
                    safe_area: None,
                });
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Err(PptxError::InvalidStructure(
        "presentation 缺少 sldSz".into(),
    ))
}

fn stable_part_id(prefix: &str, part: &str) -> String {
    let digest = Sha256::digest(part.as_bytes());
    format!("{prefix}-{}", hex::encode(&digest[..8]))
}

fn layout_id_from_part(part: &str) -> Option<String> {
    part.starts_with("ppt/slideLayouts/")
        .then(|| stable_part_id("layout", part))
}

fn import_masters_layouts_theme<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    presentation_rels: &HashMap<String, Relationship>,
    deck: &mut Deck,
    report: &mut PptxLossReport,
) -> Result<(), PptxError> {
    let master_parts = presentation_rels
        .values()
        .filter(|relationship| relationship.kind.ends_with("/slideMaster"))
        .map(|relationship| relationship.target.clone())
        .collect::<Vec<_>>();
    for master_part in master_parts {
        let master_xml = read_relationship_part(archive, &master_part, MAX_XML_PART_BYTES)?;
        let master_id = stable_part_id("master", &master_part);
        deck.masters.push(SlideMaster {
            id: master_id.clone(),
            name: part_display_name(&master_part),
            background: parse_background(&master_xml)?,
            placeholders: Vec::new(),
        });
        report_unmapped_template_content(&master_xml, &master_part, "master", report);
        let master_rels = relationships_for(archive, &master_part, report)?;
        if deck.theme.id == "pptx-default" {
            if let Some(theme_rel) = master_rels
                .values()
                .find(|relationship| relationship.kind.ends_with("/theme"))
            {
                let theme_xml =
                    read_relationship_part(archive, &theme_rel.target, MAX_XML_PART_BYTES)?;
                deck.theme = parse_theme(&theme_xml, stable_part_id("theme", &theme_rel.target))?;
                if contains_local_tag(&theme_xml, b"fmtScheme")
                    || contains_local_tag(&theme_xml, b"effectStyleLst")
                {
                    report.unsupported.push(report_item(
                        PptxReportKind::Unsupported,
                        "themeFormatting",
                        &theme_rel.target,
                        "主题效果和格式方案尚未映射为 typed Deck theme",
                        Some("保留原始 PPTX source asset，或等待 theme style adapter"),
                    ));
                }
            }
        }
        for layout_rel in master_rels
            .values()
            .filter(|relationship| relationship.kind.ends_with("/slideLayout"))
        {
            let layout_xml =
                read_relationship_part(archive, &layout_rel.target, MAX_XML_PART_BYTES)?;
            deck.layouts.push(SlideLayout {
                id: stable_part_id("layout", &layout_rel.target),
                master_id: master_id.clone(),
                name: part_display_name(&layout_rel.target),
                placeholders: Vec::new(),
            });
            report_unmapped_template_content(&layout_xml, &layout_rel.target, "layout", report);
            if contains_local_tag(&layout_xml, b"graphicFrame") {
                report.unsupported.push(report_item(
                    PptxReportKind::Unsupported,
                    "layoutGraphicFrame",
                    &layout_rel.target,
                    "layout 中的图表/表格占位符未映射为 typed placeholder",
                    Some("当前保留 layout 关系；内容节点将单独导入"),
                ));
            }
        }
    }
    Ok(())
}

fn report_unmapped_template_content(
    xml: &[u8],
    part: &str,
    scope: &str,
    report: &mut PptxLossReport,
) {
    if contains_local_tag(xml, b"ph") {
        report.unsupported.push(report_item(
            PptxReportKind::Unsupported,
            format!("{scope}Placeholder"),
            part,
            format!("{scope} placeholder 尚未映射为 typed placeholder"),
            Some("保留原始 PPTX source asset，或等待 placeholder adapter"),
        ));
    }
    if contains_local_tag(xml, b"sp")
        || contains_local_tag(xml, b"pic")
        || contains_local_tag(xml, b"grpSp")
        || contains_local_tag(xml, b"graphicFrame")
    {
        report.unsupported.push(report_item(
            PptxReportKind::Unsupported,
            format!("{scope}SceneNode"),
            part,
            format!("{scope} 中的场景节点尚未映射为 Deck scene node"),
            Some("保留原始 PPTX source asset，或等待 master/layout scene adapter"),
        ));
    }
}

fn part_display_name(part: &str) -> String {
    part.rsplit('/')
        .next()
        .unwrap_or(part)
        .trim_end_matches(".xml")
        .to_owned()
}

fn parse_background(xml: &[u8]) -> Result<SlideBackground, PptxError> {
    parse_first_color(xml).map_or(Ok(SlideBackground::None), |color| {
        Ok(SlideBackground::Solid(color))
    })
}

fn parse_theme(xml: &[u8], id: String) -> Result<DeckTheme, PptxError> {
    let mut theme = DeckTheme {
        id,
        name: "PPTX theme".into(),
        ..DeckTheme::default()
    };
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut color_stack = Vec::<ThemeColorToken>::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) | Event::Empty(event) => {
                let event_name = event.name().as_ref().to_vec();
                let local = local_name(&event_name);
                if let Some(token) = theme_color_token(local) {
                    color_stack.push(token);
                }
                if local == b"srgbClr" || local == b"sysClr" {
                    let value = attr(
                        &event,
                        if local == b"srgbClr" {
                            b"val"
                        } else {
                            b"lastClr"
                        },
                    );
                    if let (Some(token), Some(value)) = (color_stack.last(), value) {
                        if let Some(color) = parse_hex_color(&value) {
                            theme.colors.insert(token.clone(), color);
                        }
                    }
                }
                if local == b"latin" {
                    if let Some(typeface) = attr(&event, b"typeface") {
                        let token = if theme.fonts.contains_key(&ThemeFontToken::Heading) {
                            ThemeFontToken::Body
                        } else {
                            ThemeFontToken::Heading
                        };
                        if !typeface.is_empty() {
                            theme.fonts.insert(token, typeface);
                        }
                    }
                }
            }
            Event::End(event) => {
                if theme_color_token(local_name(event.name().as_ref())).is_some() {
                    color_stack.pop();
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(theme)
}

fn theme_color_token(name: &[u8]) -> Option<ThemeColorToken> {
    match name {
        b"lt1" => Some(ThemeColorToken::Background),
        b"dk1" => Some(ThemeColorToken::Text),
        b"accent1" => Some(ThemeColorToken::Accent1),
        b"accent2" => Some(ThemeColorToken::Accent2),
        b"accent3" => Some(ThemeColorToken::Accent3),
        b"accent4" => Some(ThemeColorToken::Accent4),
        b"accent5" => Some(ThemeColorToken::Accent5),
        b"accent6" => Some(ThemeColorToken::Accent6),
        b"hlink" => Some(ThemeColorToken::Hyperlink),
        b"folHlink" => Some(ThemeColorToken::FollowedHyperlink),
        _ => None,
    }
}

fn parse_hex_color(value: &str) -> Option<Rgba> {
    let value = value.trim_start_matches('#');
    if value.len() != 6 {
        return None;
    }
    Some(Rgba {
        r: u8::from_str_radix(&value[0..2], 16).ok()?,
        g: u8::from_str_radix(&value[2..4], 16).ok()?,
        b: u8::from_str_radix(&value[4..6], 16).ok()?,
        a: u8::MAX,
    })
}

fn parse_first_color(xml: &[u8]) -> Option<ColorRef> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer).ok()? {
            Event::Empty(event) | Event::Start(event)
                if local_name(event.name().as_ref()) == b"srgbClr" =>
            {
                return attr(&event, b"val")
                    .and_then(|value| parse_hex_color(&value))
                    .map(ColorRef::Rgba);
            }
            Event::Eof => return None,
            _ => {}
        }
        buffer.clear();
    }
}

fn contains_local_tag(xml: &[u8], wanted: &[u8]) -> bool {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) | Ok(Event::Empty(event))
                if local_name(event.name().as_ref()) == wanted =>
            {
                return true
            }
            Ok(Event::Eof) | Err(_) => return false,
            _ => buffer.clear(),
        }
    }
}

fn parse_notes<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    part: &str,
    report: &mut PptxLossReport,
) -> Result<Option<String>, PptxError> {
    let xml = read_relationship_part(archive, part, MAX_XML_PART_BYTES)?;
    let (text, has_formatting, has_multiple_bodies) = parse_notes_body_text(&xml)?;
    if has_formatting {
        report.unsupported.push(report_item(
            PptxReportKind::Unsupported,
            "notesFormatting",
            part,
            "notes 的 run 格式尚未映射，当前只保留纯文本",
            Some("保留原始 PPTX source asset，或等待 notes rich-text adapter"),
        ));
    }
    if has_multiple_bodies {
        report.unsupported.push(report_item(
            PptxReportKind::Unsupported,
            "notesStructure",
            part,
            "notesSlide 包含多个 body placeholder，无法无歧义映射为单个 slide.notes",
            Some("合并演讲者备注 body 后重新导入，或保留原始 PPTX source asset"),
        ));
    }
    Ok(text.filter(|value| !value.trim().is_empty()))
}

#[derive(Default)]
struct NotesShapeDraft {
    is_body: bool,
    paragraphs: Vec<String>,
    paragraph: Option<String>,
    in_text: bool,
    has_formatting: bool,
}

fn parse_notes_body_text(xml: &[u8]) -> Result<(Option<String>, bool, bool), PptxError> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut shape = None::<NotesShapeDraft>;
    let mut bodies = Vec::<(String, bool)>::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) => {
                let name = event.name().as_ref().to_vec();
                match local_name(&name) {
                    b"sp" if shape.is_none() => shape = Some(NotesShapeDraft::default()),
                    b"ph" => {
                        if let Some(shape) = shape.as_mut() {
                            shape.is_body = attr(&event, b"type").as_deref() == Some("body");
                        }
                    }
                    b"p" => {
                        if let Some(shape) = shape.as_mut() {
                            shape.paragraph = Some(String::new());
                        }
                    }
                    b"t" => {
                        if let Some(shape) = shape.as_mut() {
                            shape.in_text = true;
                        }
                    }
                    b"rPr" => {
                        if let Some(shape) = shape.as_mut() {
                            shape.has_formatting = true;
                        }
                    }
                    _ => {}
                }
            }
            Event::Empty(event) => {
                let name = event.name().as_ref().to_vec();
                match local_name(&name) {
                    b"ph" => {
                        if let Some(shape) = shape.as_mut() {
                            shape.is_body = attr(&event, b"type").as_deref() == Some("body");
                        }
                    }
                    b"p" => {
                        if let Some(shape) = shape.as_mut() {
                            shape.paragraphs.push(String::new());
                        }
                    }
                    b"br" => {
                        if let Some(paragraph) =
                            shape.as_mut().and_then(|shape| shape.paragraph.as_mut())
                        {
                            paragraph.push('\n');
                        }
                    }
                    b"rPr" => {
                        if let Some(shape) = shape.as_mut() {
                            shape.has_formatting = true;
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(text) if shape.as_ref().is_some_and(|shape| shape.in_text) => {
                if let Some(paragraph) = shape.as_mut().and_then(|shape| shape.paragraph.as_mut()) {
                    paragraph.push_str(&text.unescape()?);
                }
            }
            Event::End(event) => {
                let name = event.name().as_ref().to_vec();
                match local_name(&name) {
                    b"t" => {
                        if let Some(shape) = shape.as_mut() {
                            shape.in_text = false;
                        }
                    }
                    b"p" => {
                        if let Some(shape) = shape.as_mut() {
                            if let Some(paragraph) = shape.paragraph.take() {
                                shape.paragraphs.push(paragraph);
                            }
                        }
                    }
                    b"sp" => {
                        if let Some(mut finished) = shape.take() {
                            if let Some(paragraph) = finished.paragraph.take() {
                                finished.paragraphs.push(paragraph);
                            }
                            if finished.is_body {
                                bodies.push((
                                    finished.paragraphs.join("\n"),
                                    finished.has_formatting,
                                ));
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    let has_multiple_bodies = bodies.len() > 1;
    let has_formatting = bodies.iter().any(|(_, formatted)| *formatted);
    Ok((
        bodies.into_iter().next().map(|(text, _)| text),
        has_formatting,
        has_multiple_bodies,
    ))
}

struct SlideNodeDraft {
    id: String,
    name: Option<String>,
    parent_id: Option<String>,
    transform: NodeTransform,
    geometry: ShapeGeometry,
    text: String,
    runs: Vec<PresentationTextRun>,
    active_run: Option<ActiveTextRun>,
    paragraphs: Vec<PresentationParagraph>,
    active_paragraph: Option<ActiveParagraph>,
    vertical_align: TextVerticalAlign,
    padding: Insets,
    auto_fit: TextAutoFit,
    embed_relationship: Option<String>,
    is_picture: bool,
    is_group: bool,
    sequence: usize,
}

/// The OOXML reader only retains an active run while it is inside `a:r`.  This makes it
/// impossible for a style parsed from one run to leak into the next one, which is especially
/// important for mixed bold/colour text imported from Office.
#[derive(Debug, Clone)]
struct ActiveTextRun {
    start: usize,
    style: PresentationTextStyle,
}

#[derive(Debug, Clone)]
struct ActiveParagraph {
    start: usize,
    alignment: TextHorizontalAlign,
    list: Option<PresentationListStyle>,
    indent_level: u8,
}

impl Default for SlideNodeDraft {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: None,
            parent_id: None,
            transform: default_transform(0),
            geometry: ShapeGeometry::Rectangle,
            text: String::new(),
            runs: Vec::new(),
            active_run: None,
            paragraphs: Vec::new(),
            active_paragraph: None,
            vertical_align: TextVerticalAlign::Top,
            padding: Insets::default(),
            auto_fit: TextAutoFit::None,
            embed_relationship: None,
            is_picture: false,
            is_group: false,
            sequence: 0,
        }
    }
}

fn report_text_run_unsupported(report: &mut PptxLossReport, slide_index: usize, detail: &str) {
    report.unsupported.push(report_item(
        PptxReportKind::Unsupported,
        "textRunStyle",
        format!("slides[{}].text", slide_index),
        detail,
        Some("保留原始 PPTX source asset，或简化为基础文本 run 后重新导入"),
    ));
}

fn parse_text_body_properties(
    event: &quick_xml::events::BytesStart<'_>,
    draft: &mut SlideNodeDraft,
    report: &mut PptxLossReport,
    slide_index: usize,
) -> Result<(), PptxError> {
    draft.vertical_align = match attr(event, b"anchor").as_deref() {
        None | Some("t") => TextVerticalAlign::Top,
        Some("ctr") => TextVerticalAlign::Middle,
        Some("b") => TextVerticalAlign::Bottom,
        Some(_) => {
            report_text_run_unsupported(
                report,
                slide_index,
                "a:bodyPr anchor 不是 top/middle/bottom，不能无损映射",
            );
            TextVerticalAlign::Top
        }
    };
    for (name, target) in [
        (b"tIns".as_slice(), &mut draft.padding.top),
        (b"rIns".as_slice(), &mut draft.padding.right),
        (b"bIns".as_slice(), &mut draft.padding.bottom),
        (b"lIns".as_slice(), &mut draft.padding.left),
    ] {
        *target = parse_coordinate(event, name, 0.0)?;
        if !target.is_finite() || *target < 0.0 {
            report_text_run_unsupported(report, slide_index, "a:bodyPr inset 必须是非负有限坐标");
            *target = 0.0;
        }
    }
    Ok(())
}

fn parse_run_properties(
    event: &quick_xml::events::BytesStart<'_>,
    style: &mut PresentationTextStyle,
    report: &mut PptxLossReport,
    slide_index: usize,
) {
    for attribute in event.attributes().flatten() {
        let key = local_name(attribute.key.as_ref());
        let value = String::from_utf8_lossy(attribute.value.as_ref());
        match key {
            b"b" => style.bold = matches!(value.as_ref(), "1" | "true"),
            b"i" => style.italic = matches!(value.as_ref(), "1" | "true"),
            b"u" => match value.as_ref() {
                "none" => style.underline = false,
                "sng" => style.underline = true,
                _ => report_text_run_unsupported(
                    report,
                    slide_index,
                    "仅支持 a:rPr u=none/sng；其他下划线样式不会静默降级",
                ),
            },
            b"strike" => match value.as_ref() {
                "noStrike" => style.strikethrough = false,
                "sngStrike" => style.strikethrough = true,
                _ => report_text_run_unsupported(
                    report,
                    slide_index,
                    "仅支持 a:rPr strike=noStrike/sngStrike；其他删除线样式不会静默降级",
                ),
            },
            b"sz" => match value.parse::<f32>() {
                Ok(size) if size.is_finite() && size > 0.0 && size <= 51_200.0 => {
                    style.font_size = Some(size / 100.0)
                }
                _ => report_text_run_unsupported(
                    report,
                    slide_index,
                    "a:rPr sz 必须是 1..51200 的 1/100 point 数值",
                ),
            },
            // `dirty` is an Office cache hint, not document semantics.
            b"dirty" => {}
            _ => report_text_run_unsupported(
                report,
                slide_index,
                "a:rPr 含未映射属性；adapter 不会静默丢弃该 run 格式",
            ),
        }
    }
}

fn parse_paragraph_properties(
    event: &quick_xml::events::BytesStart<'_>,
    paragraph: &mut ActiveParagraph,
    report: &mut PptxLossReport,
    slide_index: usize,
) {
    paragraph.alignment = match attr(event, b"algn").as_deref() {
        None | Some("l") => TextHorizontalAlign::Left,
        Some("ctr") => TextHorizontalAlign::Center,
        Some("r") => TextHorizontalAlign::Right,
        Some("just") | Some("justLow") => TextHorizontalAlign::Justify,
        Some(_) => {
            report_text_run_unsupported(report, slide_index, "a:pPr algn 尚未映射");
            TextHorizontalAlign::Left
        }
    };
    if let Some(level) = attr(event, b"lvl") {
        match level.parse::<u8>() {
            Ok(level @ 0..=8) => paragraph.indent_level = level,
            _ => report_text_run_unsupported(report, slide_index, "a:pPr lvl 必须在 0 到 8 之间"),
        }
    }
}

fn start_paragraph(draft: &mut SlideNodeDraft) {
    if !draft.paragraphs.is_empty() {
        draft.text.push('\n');
        if let Some(previous) = draft.paragraphs.last_mut() {
            previous.end = draft.text.chars().count();
        }
    }
    draft.active_paragraph = Some(ActiveParagraph {
        start: draft.text.chars().count(),
        alignment: TextHorizontalAlign::Left,
        list: None,
        indent_level: 0,
    });
}

fn finish_paragraph(draft: &mut SlideNodeDraft) {
    let Some(paragraph) = draft.active_paragraph.take() else {
        return;
    };
    if paragraph.start == draft.text.chars().count() {
        draft.text.push('\n');
    }
    draft.paragraphs.push(PresentationParagraph {
        start: paragraph.start,
        end: draft.text.chars().count(),
        alignment: paragraph.alignment,
        list: paragraph.list,
        indent_level: paragraph.indent_level,
    });
}

fn take_valid_text_runs(
    draft: &mut SlideNodeDraft,
    text: &str,
    report: &mut PptxLossReport,
    slide_index: usize,
) -> Vec<PresentationTextRun> {
    if draft.active_run.is_some() {
        report_text_run_unsupported(report, slide_index, "文本节点在 a:r 未闭合时结束");
        draft.active_run = None;
        return Vec::new();
    }
    if draft.runs.is_empty() {
        return Vec::new();
    }
    let length = text.chars().count();
    let valid = draft
        .runs
        .iter()
        .scan(0usize, |cursor, run| {
            let valid = run.start >= *cursor && run.start < run.end && run.end <= length;
            *cursor = run.end;
            Some(valid)
        })
        .all(|valid| valid);
    if !valid {
        report_text_run_unsupported(
            report,
            slide_index,
            "a:r 与 text 内容未形成完整连续覆盖，不能安全构造 canonical text runs",
        );
        return Vec::new();
    }
    let mut cursor = 0;
    let mut complete = Vec::new();
    for run in std::mem::take(&mut draft.runs) {
        if cursor < run.start {
            complete.push(PresentationTextRun {
                start: cursor,
                end: run.start,
                style: PresentationTextStyle::default(),
            });
        }
        cursor = run.end;
        complete.push(run);
    }
    if cursor < length {
        complete.push(PresentationTextRun {
            start: cursor,
            end: length,
            style: PresentationTextStyle::default(),
        });
    }
    if complete
        .iter()
        .all(|run| run.style == PresentationTextStyle::default())
    {
        // Canonical rich text uses an empty run list for unstyled text.  OOXML requires an
        // `a:r` wrapper nevertheless, so normalise that transport-only default on import.
        Vec::new()
    } else {
        complete
    }
}

fn parse_slide_transition(
    xml: &[u8],
    slide_index: usize,
    report: &mut PptxLossReport,
) -> Result<Option<SlideTransition>, PptxError> {
    #[derive(Default)]
    struct Candidate {
        duration_ms: u32,
        kind: Option<TransitionKind>,
        saw_unknown_effect: bool,
    }

    fn duration(
        event: &quick_xml::events::BytesStart<'_>,
        slide_index: usize,
        report: &mut PptxLossReport,
    ) -> u32 {
        let Some(raw) = attr(event, b"dur") else {
            return 0;
        };
        match raw.parse::<u32>() {
            Ok(value) if value <= 600_000 => value,
            _ => {
                report.unsupported.push(report_item(
                    PptxReportKind::Invalid,
                    "slideTransitionDuration",
                    format!("slides[{slide_index}].transition"),
                    format!("p14:dur={raw:?} 不是 0 到 600000 毫秒的有效时长"),
                    Some("修复 transition duration 后重新导入"),
                ));
                0
            }
        }
    }

    fn set_effect(candidate: &mut Candidate, local: &[u8]) -> bool {
        let kind = match local {
            b"fade" => TransitionKind::Fade,
            b"push" => TransitionKind::Push,
            b"wipe" => TransitionKind::Wipe,
            b"cut" => TransitionKind::None,
            _ => return false,
        };
        candidate.kind.get_or_insert(kind);
        true
    }

    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut depth = 0usize;
    let mut current: Option<Candidate> = None;
    let mut candidates = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) if local_name(event.name().as_ref()) == b"transition" => {
                current = Some(Candidate {
                    duration_ms: duration(&event, slide_index, report),
                    ..Candidate::default()
                });
                depth = 1;
            }
            Event::Empty(event) if local_name(event.name().as_ref()) == b"transition" => {
                candidates.push(Candidate {
                    duration_ms: duration(&event, slide_index, report),
                    kind: Some(TransitionKind::None),
                    saw_unknown_effect: false,
                });
            }
            Event::Start(event) if current.is_some() => {
                if depth == 1 {
                    let event_name = event.name().as_ref().to_vec();
                    let local = local_name(&event_name);
                    if !set_effect(current.as_mut().expect("checked above"), local) {
                        current.as_mut().expect("checked above").saw_unknown_effect = true;
                    }
                }
                depth += 1;
            }
            Event::Empty(event) if current.is_some() && depth == 1 => {
                let event_name = event.name().as_ref().to_vec();
                let local = local_name(&event_name);
                if !set_effect(current.as_mut().expect("checked above"), local) {
                    current.as_mut().expect("checked above").saw_unknown_effect = true;
                }
            }
            Event::End(event)
                if current.is_some() && local_name(event.name().as_ref()) == b"transition" =>
            {
                candidates.push(current.take().expect("checked above"));
                depth = 0;
            }
            Event::End(_) if current.is_some() => depth = depth.saturating_sub(1),
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }

    if candidates
        .iter()
        .any(|candidate| candidate.saw_unknown_effect)
    {
        report.unsupported.push(report_item(
            PptxReportKind::Unsupported,
            "slideTransitionEffect",
            format!("slides[{slide_index}].transition"),
            "transition 包含 canonical Deck 未支持的效果、声音或扩展；仅保留可识别 fallback",
            Some("改用 fade/push/wipe/cut，或保留原始 PPTX source asset"),
        ));
    }
    Ok(candidates.into_iter().find_map(|candidate| {
        candidate.kind.map(|kind| SlideTransition {
            kind,
            duration_ms: candidate.duration_ms,
        })
    }))
}

#[derive(Debug)]
struct TimelineImportDraft {
    base_depth: usize,
    preset: AnimationPreset,
    trigger: AnimationTrigger,
    delay_ms: Option<u32>,
    duration_ms: Option<u32>,
    target_spid: Option<String>,
    behavior_depth: Option<usize>,
    saw_expected_behavior: bool,
}

fn parse_timeline_milliseconds(value: Option<String>) -> Option<u32> {
    value?.parse::<u32>().ok().filter(|value| *value <= 600_000)
}

fn animation_preset(value: &str) -> Option<AnimationPreset> {
    match value {
        "1" => Some(AnimationPreset::Appear),
        "2" => Some(AnimationPreset::FlyIn),
        "10" => Some(AnimationPreset::Fade),
        "22" => Some(AnimationPreset::Wipe),
        _ => None,
    }
}

fn animation_trigger(value: &str) -> Option<AnimationTrigger> {
    match value {
        "clickEffect" => Some(AnimationTrigger::OnClick),
        "withEffect" => Some(AnimationTrigger::WithPrevious),
        "afterEffect" => Some(AnimationTrigger::AfterPrevious),
        _ => None,
    }
}

fn animation_filter_matches(preset: AnimationPreset, filter: Option<&str>) -> bool {
    match preset {
        AnimationPreset::Appear => false,
        AnimationPreset::Fade => filter == Some("fade"),
        AnimationPreset::FlyIn => filter.is_some_and(|value| value.starts_with("fly(")),
        AnimationPreset::Wipe => filter.is_some_and(|value| value.starts_with("wipe(")),
    }
}

fn parse_slide_timeline(
    xml: &[u8],
    slide_index: usize,
    nodes: &[SceneNode],
    report: &mut PptxLossReport,
) -> Result<Timeline, PptxError> {
    if !contains_local_tag(xml, b"timing") && !contains_local_tag(xml, b"bldLst") {
        return Ok(Timeline::default());
    }

    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut depth = 0usize;
    let mut timing_depth = None;
    let mut active: Option<TimelineImportDraft> = None;
    let mut drafts = Vec::new();
    let mut unsupported = contains_local_tag(xml, b"bldLst");

    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) => {
                let name = event.name().as_ref().to_vec();
                let local = local_name(&name);
                let event_depth = depth;
                depth += 1;
                if local == b"timing" {
                    timing_depth = Some(event_depth);
                    buffer.clear();
                    continue;
                }
                if timing_depth.is_none() {
                    buffer.clear();
                    continue;
                }

                if local == b"cTn" {
                    if let Some(preset_class) = attr(&event, b"presetClass") {
                        if active.is_some() || preset_class != "entr" {
                            unsupported = true;
                        } else {
                            let preset = attr(&event, b"presetID")
                                .as_deref()
                                .and_then(animation_preset);
                            let trigger = attr(&event, b"nodeType")
                                .as_deref()
                                .and_then(animation_trigger);
                            match (preset, trigger) {
                                (Some(preset), Some(trigger)) => {
                                    active = Some(TimelineImportDraft {
                                        base_depth: event_depth,
                                        preset,
                                        trigger,
                                        delay_ms: None,
                                        duration_ms: None,
                                        target_spid: None,
                                        behavior_depth: None,
                                        saw_expected_behavior: false,
                                    });
                                }
                                _ => unsupported = true,
                            }
                        }
                    } else if let Some(draft) = active.as_mut() {
                        if draft.behavior_depth.is_some() {
                            match parse_timeline_milliseconds(attr(&event, b"dur")) {
                                Some(duration_ms) => draft.duration_ms = Some(duration_ms),
                                None => unsupported = true,
                            }
                        }
                    }
                    if attr(&event, b"nodeType").as_deref() == Some("interactiveSeq") {
                        unsupported = true;
                    }
                } else if local == b"cBhvr" {
                    if let Some(draft) = active.as_mut() {
                        draft.behavior_depth = Some(event_depth);
                    }
                } else if local == b"cond" {
                    if let Some(draft) = active.as_mut() {
                        if draft.behavior_depth.is_none() && draft.delay_ms.is_none() {
                            match parse_timeline_milliseconds(attr(&event, b"delay")) {
                                Some(delay_ms) => draft.delay_ms = Some(delay_ms),
                                None => unsupported = true,
                            }
                        }
                    }
                } else if local == b"spTgt" {
                    if let Some(draft) = active.as_mut() {
                        let target = attr(&event, b"spid");
                        if draft.target_spid.is_some() && draft.target_spid != target {
                            unsupported = true;
                        } else {
                            draft.target_spid = target;
                        }
                    }
                } else if local == b"animEffect" {
                    if let Some(draft) = active.as_mut() {
                        let matches = attr(&event, b"transition").as_deref() == Some("in")
                            && animation_filter_matches(
                                draft.preset,
                                attr(&event, b"filter").as_deref(),
                            );
                        draft.saw_expected_behavior |= matches;
                        unsupported |= !matches;
                    }
                } else if local == b"set" {
                    if let Some(draft) = active.as_mut() {
                        let matches = draft.preset == AnimationPreset::Appear;
                        draft.saw_expected_behavior |= matches;
                        unsupported |= !matches;
                    }
                } else if matches!(
                    local,
                    b"anim"
                        | b"animClr"
                        | b"animMotion"
                        | b"animRot"
                        | b"animScale"
                        | b"cmd"
                        | b"audio"
                        | b"video"
                        | b"excl"
                ) {
                    unsupported = true;
                }
            }
            Event::Empty(event) if timing_depth.is_some() => {
                let name = event.name().as_ref().to_vec();
                let local = local_name(&name);
                if local == b"cond" {
                    if let Some(draft) = active.as_mut() {
                        if draft.behavior_depth.is_none() && draft.delay_ms.is_none() {
                            match parse_timeline_milliseconds(attr(&event, b"delay")) {
                                Some(delay_ms) => draft.delay_ms = Some(delay_ms),
                                None => unsupported = true,
                            }
                        }
                    }
                } else if local == b"spTgt" {
                    if let Some(draft) = active.as_mut() {
                        let target = attr(&event, b"spid");
                        if draft.target_spid.is_some() && draft.target_spid != target {
                            unsupported = true;
                        } else {
                            draft.target_spid = target;
                        }
                    }
                } else if local == b"cTn" {
                    if let Some(draft) = active.as_mut() {
                        if draft.behavior_depth.is_some() {
                            match parse_timeline_milliseconds(attr(&event, b"dur")) {
                                Some(duration_ms) => draft.duration_ms = Some(duration_ms),
                                None => unsupported = true,
                            }
                        }
                    }
                } else if matches!(
                    local,
                    b"anim"
                        | b"animClr"
                        | b"animMotion"
                        | b"animRot"
                        | b"animScale"
                        | b"cmd"
                        | b"audio"
                        | b"video"
                        | b"excl"
                ) {
                    unsupported = true;
                }
            }
            Event::End(event) => {
                depth = depth.saturating_sub(1);
                let name = event.name().as_ref().to_vec();
                let local = local_name(&name);
                if active
                    .as_ref()
                    .is_some_and(|draft| local == b"cTn" && draft.base_depth == depth)
                {
                    drafts.push(active.take().expect("checked above"));
                } else if active
                    .as_ref()
                    .is_some_and(|draft| local == b"cBhvr" && draft.behavior_depth == Some(depth))
                {
                    active.as_mut().expect("checked above").behavior_depth = None;
                }
                if local == b"timing" && timing_depth == Some(depth) {
                    timing_depth = None;
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }

    let node_ids = nodes
        .iter()
        .map(|node| (node.id.rsplit('-').next().unwrap_or(&node.id), &node.id))
        .collect::<HashMap<_, _>>();
    let mut entries = Vec::new();
    for (index, draft) in drafts.into_iter().enumerate() {
        let target_node_id = draft
            .target_spid
            .as_deref()
            .and_then(|spid| node_ids.get(spid).copied())
            .cloned();
        let valid = draft.saw_expected_behavior
            && target_node_id.is_some()
            && draft.delay_ms.is_some()
            && draft.duration_ms.is_some();
        if !valid {
            unsupported = true;
            continue;
        }
        entries.push(AnimationEntry {
            id: format!("pptx-slide-{}-animation-{}", slide_index + 1, index + 1),
            target_node_id: target_node_id.expect("validated above"),
            trigger: draft.trigger,
            preset: draft.preset,
            duration_ms: draft.duration_ms.expect("validated above"),
            delay_ms: draft.delay_ms.expect("validated above"),
            order_key: format!("{index:08}"),
        });
    }
    if unsupported || (contains_local_tag(xml, b"timing") && entries.is_empty()) {
        report.unsupported.push(report_item(
            PptxReportKind::Unsupported,
            "timeline",
            format!("slides[{slide_index}].timeline"),
            "动画时间树超出受支持的主序列入口效果子集；仅保留可严格识别的 appear/fade/flyIn/wipe",
            Some("简化为 On Click/With Previous/After Previous 入口动画，或保留原始 PPTX source asset"),
        ));
    }
    Ok(Timeline { entries })
}

fn parse_slide(
    xml: &[u8],
    slide_index: usize,
    relationships: &HashMap<String, Relationship>,
    assets: &HashMap<String, AssetRef>,
    report: &mut PptxLossReport,
) -> Result<Slide, PptxError> {
    let transition = parse_slide_transition(xml, slide_index, report)?;
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut stack = Vec::<SlideNodeDraft>::new();
    let mut nodes = Vec::<SceneNode>::new();
    let mut sequence = 0usize;
    let mut in_text = false;
    let mut slide_name = String::new();
    let mut root_group_seen = false;
    let mut root_group_started = false;
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) => {
                let event_name = event.name().as_ref().to_vec();
                let local = local_name(&event_name);
                match local {
                    b"cSld" if stack.is_empty() => {
                        slide_name = attr(&event, b"name").unwrap_or_default();
                    }
                    b"spTree" => root_group_seen = true,
                    b"sp" | b"pic" | b"grpSp" => {
                        if local == b"grpSp" && root_group_seen && !root_group_started {
                            root_group_started = true;
                            continue;
                        }
                        let parent_id = stack
                            .last()
                            .and_then(|parent| parent.is_group.then(|| parent.id.clone()));
                        stack.push(SlideNodeDraft {
                            id: format!("slide-{}-node-{}", slide_index + 1, sequence + 1),
                            parent_id,
                            is_picture: local == b"pic",
                            is_group: local == b"grpSp",
                            sequence,
                            transform: default_transform(sequence),
                            geometry: ShapeGeometry::Rectangle,
                            ..Default::default()
                        });
                        sequence += 1;
                    }
                    b"cNvPr" => {
                        if let Some(draft) = stack.last_mut() {
                            draft.name = attr(&event, b"name");
                            if let Some(id) = attr(&event, b"id") {
                                draft.id = format!("slide-{}-node-{id}", slide_index + 1);
                            }
                        }
                    }
                    b"off" => {
                        if let Some(draft) = stack.last_mut() {
                            draft.transform.x = parse_coordinate(&event, b"x", draft.transform.x)?;
                            draft.transform.y = parse_coordinate(&event, b"y", draft.transform.y)?;
                        }
                    }
                    b"ext" => {
                        if let Some(draft) = stack.last_mut() {
                            draft.transform.width =
                                parse_coordinate(&event, b"cx", draft.transform.width)?;
                            draft.transform.height =
                                parse_coordinate(&event, b"cy", draft.transform.height)?;
                        }
                    }
                    b"prstGeom" => {
                        if let Some(draft) = stack.last_mut() {
                            draft.geometry = match attr(&event, b"prst").as_deref() {
                                Some("ellipse") => ShapeGeometry::Ellipse,
                                Some("line") => ShapeGeometry::Line,
                                Some("rightArrow") | Some("leftArrow") | Some("upArrow")
                                | Some("downArrow") => ShapeGeometry::Arrow,
                                _ => ShapeGeometry::Rectangle,
                            };
                        }
                    }
                    b"blip" => {
                        if let Some(draft) = stack.last_mut() {
                            draft.embed_relationship = attr(&event, b"embed");
                        }
                    }
                    b"r" => {
                        if let Some(draft) = stack.last_mut() {
                            if draft.active_run.is_some() {
                                report_text_run_unsupported(
                                    report,
                                    slide_index,
                                    "嵌套 a:r 不能映射为连续 canonical text run",
                                );
                            }
                            draft.active_run = Some(ActiveTextRun {
                                start: draft.text.chars().count(),
                                style: PresentationTextStyle::default(),
                            });
                        }
                    }
                    b"p" => {
                        if let Some(draft) = stack.last_mut() {
                            start_paragraph(draft);
                        }
                    }
                    b"pPr" => {
                        if let Some(paragraph) = stack
                            .last_mut()
                            .and_then(|draft| draft.active_paragraph.as_mut())
                        {
                            parse_paragraph_properties(&event, paragraph, report, slide_index);
                        }
                    }
                    b"bodyPr" => {
                        if let Some(draft) = stack.last_mut() {
                            parse_text_body_properties(&event, draft, report, slide_index)?;
                        }
                    }
                    b"noAutofit" => {
                        if let Some(draft) = stack.last_mut() {
                            draft.auto_fit = TextAutoFit::None;
                        }
                    }
                    b"normAutofit" => {
                        if let Some(draft) = stack.last_mut() {
                            draft.auto_fit = TextAutoFit::ShrinkText;
                        }
                    }
                    b"spAutoFit" => {
                        if let Some(draft) = stack.last_mut() {
                            draft.auto_fit = TextAutoFit::ResizeShape;
                        }
                    }
                    b"buChar" => {
                        if let Some(paragraph) = stack
                            .last_mut()
                            .and_then(|draft| draft.active_paragraph.as_mut())
                        {
                            paragraph.list = Some(PresentationListStyle::Bullet);
                        }
                    }
                    b"buAutoNum" => {
                        if let Some(paragraph) = stack
                            .last_mut()
                            .and_then(|draft| draft.active_paragraph.as_mut())
                        {
                            let start_at = attr(&event, b"startAt")
                                .and_then(|value| value.parse().ok())
                                .filter(|value| *value > 0)
                                .unwrap_or(1);
                            paragraph.list = Some(PresentationListStyle::Ordered { start_at });
                        }
                    }
                    b"buNone" => {
                        if let Some(paragraph) = stack
                            .last_mut()
                            .and_then(|draft| draft.active_paragraph.as_mut())
                        {
                            paragraph.list = None;
                        }
                    }
                    b"rPr" => {
                        if let Some(run) =
                            stack.last_mut().and_then(|draft| draft.active_run.as_mut())
                        {
                            parse_run_properties(&event, &mut run.style, report, slide_index);
                        }
                    }
                    b"latin" | b"ea" | b"cs" => {
                        if let Some(run) =
                            stack.last_mut().and_then(|draft| draft.active_run.as_mut())
                        {
                            if let Some(typeface) = attr(&event, b"typeface") {
                                if typeface.trim().is_empty() {
                                    report_text_run_unsupported(
                                        report,
                                        slide_index,
                                        "a:latin typeface 为空，不能安全映射",
                                    );
                                } else {
                                    set_run_font(&mut run.style, typeface, report, slide_index);
                                }
                            }
                        }
                    }
                    b"srgbClr" => {
                        if let Some(run) =
                            stack.last_mut().and_then(|draft| draft.active_run.as_mut())
                        {
                            match attr(&event, b"val").and_then(|value| parse_hex_color(&value)) {
                                Some(color) => run.style.color = Some(ColorRef::Rgba(color)),
                                None => report_text_run_unsupported(
                                    report,
                                    slide_index,
                                    "a:srgbClr 不是有效的 6 位十六进制颜色",
                                ),
                            }
                        }
                    }
                    b"schemeClr" => {
                        if let Some(run) =
                            stack.last_mut().and_then(|draft| draft.active_run.as_mut())
                        {
                            match attr(&event, b"val")
                                .as_deref()
                                .and_then(|value| theme_color_token(value.as_bytes()))
                            {
                                Some(token) => run.style.color = Some(ColorRef::Theme(token)),
                                None => report_text_run_unsupported(
                                    report,
                                    slide_index,
                                    "a:schemeClr 未映射到 canonical theme colour",
                                ),
                            }
                        }
                    }
                    // Container only; its supported colour children are handled above.
                    b"solidFill" => {}
                    b"t" => in_text = true,
                    _ if stack.last().is_some_and(|draft| draft.active_run.is_some()) => {
                        report_text_run_unsupported(
                            report,
                            slide_index,
                            "a:r 包含未映射子元素；adapter 不会静默丢弃 run 内容或格式",
                        );
                    }
                    _ => {}
                }
            }
            Event::Empty(event) => {
                let event_name = event.name().as_ref().to_vec();
                let local = local_name(&event_name);
                if local == b"cNvPr" {
                    if let Some(draft) = stack.last_mut() {
                        draft.name = attr(&event, b"name");
                        if let Some(id) = attr(&event, b"id") {
                            draft.id = format!("slide-{}-node-{id}", slide_index + 1);
                        }
                    }
                } else if local == b"off" {
                    if let Some(draft) = stack.last_mut() {
                        draft.transform.x = parse_coordinate(&event, b"x", draft.transform.x)?;
                        draft.transform.y = parse_coordinate(&event, b"y", draft.transform.y)?;
                    }
                } else if local == b"ext" {
                    if let Some(draft) = stack.last_mut() {
                        draft.transform.width =
                            parse_coordinate(&event, b"cx", draft.transform.width)?;
                        draft.transform.height =
                            parse_coordinate(&event, b"cy", draft.transform.height)?;
                    }
                } else if local == b"blip" {
                    if let Some(draft) = stack.last_mut() {
                        draft.embed_relationship = attr(&event, b"embed");
                    }
                } else if local == b"p" {
                    if let Some(draft) = stack.last_mut() {
                        start_paragraph(draft);
                        finish_paragraph(draft);
                    }
                } else if local == b"pPr" {
                    if let Some(paragraph) = stack
                        .last_mut()
                        .and_then(|draft| draft.active_paragraph.as_mut())
                    {
                        parse_paragraph_properties(&event, paragraph, report, slide_index);
                    }
                } else if local == b"bodyPr" {
                    if let Some(draft) = stack.last_mut() {
                        parse_text_body_properties(&event, draft, report, slide_index)?;
                    }
                } else if local == b"noAutofit" {
                    if let Some(draft) = stack.last_mut() {
                        draft.auto_fit = TextAutoFit::None;
                    }
                } else if local == b"normAutofit" {
                    if let Some(draft) = stack.last_mut() {
                        draft.auto_fit = TextAutoFit::ShrinkText;
                    }
                } else if local == b"spAutoFit" {
                    if let Some(draft) = stack.last_mut() {
                        draft.auto_fit = TextAutoFit::ResizeShape;
                    }
                } else if local == b"buChar" {
                    if let Some(paragraph) = stack
                        .last_mut()
                        .and_then(|draft| draft.active_paragraph.as_mut())
                    {
                        paragraph.list = Some(PresentationListStyle::Bullet);
                    }
                } else if local == b"buAutoNum" {
                    if let Some(paragraph) = stack
                        .last_mut()
                        .and_then(|draft| draft.active_paragraph.as_mut())
                    {
                        let start_at = attr(&event, b"startAt")
                            .and_then(|value| value.parse().ok())
                            .filter(|value| *value > 0)
                            .unwrap_or(1);
                        paragraph.list = Some(PresentationListStyle::Ordered { start_at });
                    }
                } else if local == b"buNone" {
                    if let Some(paragraph) = stack
                        .last_mut()
                        .and_then(|draft| draft.active_paragraph.as_mut())
                    {
                        paragraph.list = None;
                    }
                } else if local == b"rPr" {
                    if let Some(run) = stack.last_mut().and_then(|draft| draft.active_run.as_mut())
                    {
                        parse_run_properties(&event, &mut run.style, report, slide_index);
                    }
                } else if matches!(local, b"latin" | b"ea" | b"cs") {
                    if let Some(run) = stack.last_mut().and_then(|draft| draft.active_run.as_mut())
                    {
                        if let Some(typeface) = attr(&event, b"typeface") {
                            if typeface.trim().is_empty() {
                                report_text_run_unsupported(
                                    report,
                                    slide_index,
                                    "a:latin typeface 为空，不能安全映射",
                                );
                            } else {
                                set_run_font(&mut run.style, typeface, report, slide_index);
                            }
                        }
                    }
                } else if local == b"srgbClr" {
                    if let Some(run) = stack.last_mut().and_then(|draft| draft.active_run.as_mut())
                    {
                        match attr(&event, b"val").and_then(|value| parse_hex_color(&value)) {
                            Some(color) => run.style.color = Some(ColorRef::Rgba(color)),
                            None => report_text_run_unsupported(
                                report,
                                slide_index,
                                "a:srgbClr 不是有效的 6 位十六进制颜色",
                            ),
                        }
                    }
                } else if local == b"schemeClr" {
                    if let Some(run) = stack.last_mut().and_then(|draft| draft.active_run.as_mut())
                    {
                        match attr(&event, b"val")
                            .as_deref()
                            .and_then(|value| theme_color_token(value.as_bytes()))
                        {
                            Some(token) => run.style.color = Some(ColorRef::Theme(token)),
                            None => report_text_run_unsupported(
                                report,
                                slide_index,
                                "a:schemeClr 未映射到 canonical theme colour",
                            ),
                        }
                    }
                } else if stack.last().is_some_and(|draft| draft.active_run.is_some()) {
                    report_text_run_unsupported(
                        report,
                        slide_index,
                        "a:r 包含未映射子元素；adapter 不会静默丢弃 run 内容或格式",
                    );
                }
            }
            Event::Text(value) if in_text => {
                if let Some(draft) = stack.last_mut() {
                    draft.text.push_str(&value.unescape()?);
                }
            }
            Event::End(event) => {
                let event_name = event.name().as_ref().to_vec();
                let local = local_name(&event_name);
                if local == b"t" {
                    in_text = false;
                }
                if local == b"r" {
                    if let Some(draft) = stack.last_mut() {
                        let Some(run) = draft.active_run.take() else {
                            report_text_run_unsupported(
                                report,
                                slide_index,
                                "a:r 结束时没有开始 run",
                            );
                            continue;
                        };
                        let end = draft.text.chars().count();
                        if run.start == end {
                            report_text_run_unsupported(
                                report,
                                slide_index,
                                "空 a:r 不能映射为 canonical text run",
                            );
                        } else {
                            draft.runs.push(PresentationTextRun {
                                start: run.start,
                                end,
                                style: run.style,
                            });
                        }
                    }
                }
                if local == b"p" {
                    if let Some(draft) = stack.last_mut() {
                        finish_paragraph(draft);
                    }
                }
                if matches!(local, b"sp" | b"pic" | b"grpSp") {
                    if local == b"grpSp" && stack.is_empty() && root_group_started {
                        root_group_started = false;
                        continue;
                    }
                    let Some(mut draft) = stack.pop() else {
                        continue;
                    };
                    if draft.is_picture {
                        let Some(rel_id) = draft.embed_relationship.as_deref() else {
                            report.unsupported.push(report_item(
                                PptxReportKind::Unsupported,
                                "image",
                                "slide",
                                "图片缺少 embed relationship",
                                Some("修复 PPTX relationship 后重试"),
                            ));
                            continue;
                        };
                        let Some(rel) = relationships.get(rel_id) else {
                            report.unsupported.push(report_item(
                                PptxReportKind::Invalid,
                                "image",
                                "slide",
                                format!("图片 relationship {rel_id} 不存在"),
                                Some("修复 PPTX relationship 后重试"),
                            ));
                            continue;
                        };
                        let Some(asset) = assets.get(rel_id) else {
                            report.unsupported.push(report_item(
                                PptxReportKind::Unsupported,
                                "image",
                                &rel.target,
                                "该图片不是已支持的嵌入图片类型",
                                Some("使用嵌入的 PNG/JPEG/GIF/WebP 图片"),
                            ));
                            continue;
                        };
                        nodes.push(scene_node_from_draft(
                            draft,
                            SceneNodeKind::Image(ImageNode {
                                asset_id: asset.asset_id.clone(),
                                original_asset_id: None,
                                crop: Default::default(),
                                flip_h: false,
                                flip_v: false,
                                caption: None,
                            }),
                        ));
                    } else if draft.is_group {
                        nodes.push(scene_node_from_draft(
                            draft,
                            SceneNodeKind::Group(GroupNode {}),
                        ));
                    } else if !draft.text.is_empty() {
                        let text = std::mem::take(&mut draft.text);
                        let runs = take_valid_text_runs(&mut draft, &text, report, slide_index);
                        let body = rich_text_with_runs_and_paragraphs(
                            text,
                            runs,
                            std::mem::take(&mut draft.paragraphs),
                        );
                        let vertical_align = draft.vertical_align;
                        let padding = draft.padding.clone();
                        let auto_fit = draft.auto_fit;
                        nodes.push(scene_node_from_draft(
                            draft,
                            SceneNodeKind::Text(TextNode {
                                frame: TextFrame {
                                    body,
                                    vertical_align,
                                    padding,
                                    auto_fit,
                                },
                            }),
                        ));
                    } else {
                        let geometry = draft.geometry;
                        nodes.push(scene_node_from_draft(
                            draft,
                            SceneNodeKind::Shape(ShapeNode {
                                geometry,
                                style: ShapeStyle {
                                    fill: Paint::None,
                                    stroke: None,
                                },
                            }),
                        ));
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    // Tables and connectors live in DrawingML containers (`p:graphicFrame` and `p:cxnSp`),
    // rather than `p:sp`.  They are intentionally parsed by dedicated, typed readers: letting
    // the generic shape reader see their text would flatten a grid or a connection into a
    // misleading text/shape node.
    nodes.extend(parse_supported_tables(xml, slide_index, report)?);
    nodes.extend(parse_supported_connectors(xml, slide_index, report)?);
    nodes.sort_by(|left, right| left.order_key.cmp(&right.order_key));
    let timeline = parse_slide_timeline(xml, slide_index, &nodes, report)?;

    Ok(Slide {
        id: format!("slide-{}", slide_index + 1),
        order_key: format!("{slide_index:08}"),
        name: slide_name,
        layout_id: None,
        background: SlideBackground::None,
        notes: None,
        transition,
        nodes,
        timeline,
    })
}

#[derive(Debug, Default)]
struct TableImportCell {
    row: u32,
    column: u32,
    content: String,
    runs: Vec<PresentationTextRun>,
    active_run: Option<ActiveTextRun>,
    fill: Paint,
    horizontal_align: HorizontalAlign,
    vertical_align: TextVerticalAlign,
    unsupported: bool,
}

#[derive(Debug, Default)]
struct TableImportDraft {
    id: String,
    name: Option<String>,
    transform: NodeTransform,
    columns: Vec<f64>,
    row_heights: Vec<f64>,
    cells: Vec<TableImportCell>,
    current_row: u32,
    current_cell: Option<TableImportCell>,
    in_tc_pr: bool,
    in_solid_fill: bool,
    unsupported: bool,
}

/// Read the deliberate lossless table subset: a rectangular grid with equal row/column sizes,
/// no merged cells, basic rich-text runs and a solid/no cell fill.  v5 does not yet model grid
/// track sizes, borders or cell margins, so accepting those would make a later export lie.
fn parse_supported_tables(
    xml: &[u8],
    slide_index: usize,
    report: &mut PptxLossReport,
) -> Result<Vec<SceneNode>, PptxError> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut graphic: Option<(String, Option<String>, NodeTransform)> = None;
    let mut table: Option<TableImportDraft> = None;
    let mut in_text = false;
    let mut sequence = 50_000usize;
    let mut nodes = Vec::new();

    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) => {
                let name = event.name().as_ref().to_vec();
                match local_name(&name) {
                    b"graphicFrame" => {
                        graphic = Some((
                            format!("slide-{}-table-{}", slide_index + 1, sequence),
                            None,
                            default_transform(sequence),
                        ));
                        sequence += 1;
                    }
                    b"cNvPr" if graphic.is_some() && table.is_none() => {
                        if let Some((id, label, _)) = graphic.as_mut() {
                            if let Some(raw) = attr(&event, b"id") {
                                *id = format!("slide-{}-node-{raw}", slide_index + 1);
                            }
                            *label = attr(&event, b"name");
                        }
                    }
                    b"off" if table.is_none() => {
                        if let Some((_, _, transform)) = graphic.as_mut() {
                            transform.x = parse_coordinate(&event, b"x", transform.x)?;
                            transform.y = parse_coordinate(&event, b"y", transform.y)?;
                        }
                    }
                    b"ext" if table.is_none() => {
                        if let Some((_, _, transform)) = graphic.as_mut() {
                            transform.width = parse_coordinate(&event, b"cx", transform.width)?;
                            transform.height = parse_coordinate(&event, b"cy", transform.height)?;
                        }
                    }
                    b"tbl" => {
                        let Some((id, name, transform)) = graphic.take() else {
                            continue;
                        };
                        table = Some(TableImportDraft {
                            id,
                            name,
                            transform,
                            ..Default::default()
                        });
                    }
                    b"gridCol" => {
                        if let Some(table) = table.as_mut() {
                            table.columns.push(parse_coordinate(&event, b"w", 0.0)?);
                        }
                    }
                    b"tr" => {
                        if let Some(table) = table.as_mut() {
                            table.row_heights.push(parse_coordinate(&event, b"h", 0.0)?);
                        }
                    }
                    b"tc" => {
                        if let Some(table) = table.as_mut() {
                            let column = table
                                .current_cell
                                .as_ref()
                                .map(|cell| cell.column + 1)
                                .unwrap_or_else(|| {
                                    table
                                        .cells
                                        .iter()
                                        .filter(|cell| cell.row == table.current_row)
                                        .count() as u32
                                });
                            let mut cell = TableImportCell {
                                row: table.current_row,
                                column,
                                ..Default::default()
                            };
                            if attr(&event, b"gridSpan")
                                .as_deref()
                                .is_some_and(|value| value != "1")
                                || attr(&event, b"rowSpan")
                                    .as_deref()
                                    .is_some_and(|value| value != "1")
                                || attr(&event, b"hMerge")
                                    .as_deref()
                                    .is_some_and(|value| value != "0")
                                || attr(&event, b"vMerge")
                                    .as_deref()
                                    .is_some_and(|value| value != "0")
                            {
                                cell.unsupported = true;
                                table.unsupported = true;
                            }
                            table.current_cell = Some(cell);
                        }
                    }
                    b"tcPr" => {
                        if let Some(table) = table.as_mut() {
                            table.in_tc_pr = true;
                            if let Some(cell) = table.current_cell.as_mut() {
                                cell.vertical_align = match attr(&event, b"anchor").as_deref() {
                                    Some("ctr") => TextVerticalAlign::Middle,
                                    Some("b") => TextVerticalAlign::Bottom,
                                    Some("t") | None => TextVerticalAlign::Top,
                                    _ => {
                                        cell.unsupported = true;
                                        table.unsupported = true;
                                        TextVerticalAlign::Top
                                    }
                                };
                            }
                        }
                    }
                    b"pPr" => {
                        if let Some(cell) =
                            table.as_mut().and_then(|table| table.current_cell.as_mut())
                        {
                            cell.horizontal_align = match attr(&event, b"algn").as_deref() {
                                None | Some("l") => HorizontalAlign::Left,
                                Some("ctr") => HorizontalAlign::Center,
                                Some("r") => HorizontalAlign::Right,
                                _ => {
                                    cell.unsupported = true;
                                    HorizontalAlign::Left
                                }
                            };
                        }
                    }
                    b"solidFill" => {
                        if let Some(table) = table.as_mut() {
                            if table.in_tc_pr {
                                table.in_solid_fill = true;
                            }
                        }
                    }
                    b"srgbClr" => {
                        if let Some(table) = table.as_mut() {
                            if table.in_solid_fill {
                                match attr(&event, b"val").and_then(|value| parse_hex_color(&value))
                                {
                                    Some(color) => {
                                        if let Some(cell) = table.current_cell.as_mut() {
                                            cell.fill = Paint::Solid(ColorRef::Rgba(color));
                                        }
                                    }
                                    None => table.unsupported = true,
                                }
                            }
                        }
                    }
                    b"schemeClr" => {
                        if let Some(table) = table.as_mut() {
                            if table.in_solid_fill {
                                match attr(&event, b"val")
                                    .as_deref()
                                    .and_then(|value| theme_color_token(value.as_bytes()))
                                {
                                    Some(color) => {
                                        if let Some(cell) = table.current_cell.as_mut() {
                                            cell.fill = Paint::Solid(ColorRef::Theme(color));
                                        }
                                    }
                                    None => table.unsupported = true,
                                }
                            }
                        }
                    }
                    b"r" => {
                        if let Some(cell) =
                            table.as_mut().and_then(|table| table.current_cell.as_mut())
                        {
                            cell.active_run = Some(ActiveTextRun {
                                start: cell.content.chars().count(),
                                style: PresentationTextStyle::default(),
                            });
                        }
                    }
                    b"rPr" => {
                        if let Some(cell) =
                            table.as_mut().and_then(|table| table.current_cell.as_mut())
                        {
                            if let Some(run) = cell.active_run.as_mut() {
                                parse_run_properties(&event, &mut run.style, report, slide_index);
                            }
                        }
                    }
                    b"latin" | b"ea" | b"cs" => {
                        if let Some(cell) =
                            table.as_mut().and_then(|table| table.current_cell.as_mut())
                        {
                            if let Some(run) = cell.active_run.as_mut() {
                                if let Some(typeface) = attr(&event, b"typeface") {
                                    set_run_font(&mut run.style, typeface, report, slide_index);
                                }
                            }
                        }
                    }
                    b"t" if table.is_some() => in_text = true,
                    b"p" if table.is_some() => {
                        if let Some(cell) =
                            table.as_mut().and_then(|table| table.current_cell.as_mut())
                        {
                            if !cell.content.is_empty() {
                                cell.unsupported = true;
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Empty(event) => {
                let name = event.name().as_ref().to_vec();
                match local_name(&name) {
                    b"cNvPr" if graphic.is_some() && table.is_none() => {
                        if let Some((id, label, _)) = graphic.as_mut() {
                            if let Some(raw) = attr(&event, b"id") {
                                *id = format!("slide-{}-node-{raw}", slide_index + 1);
                            }
                            *label = attr(&event, b"name");
                        }
                    }
                    b"off" if table.is_none() => {
                        if let Some((_, _, transform)) = graphic.as_mut() {
                            transform.x = parse_coordinate(&event, b"x", transform.x)?;
                            transform.y = parse_coordinate(&event, b"y", transform.y)?;
                        }
                    }
                    b"ext" if table.is_none() => {
                        if let Some((_, _, transform)) = graphic.as_mut() {
                            transform.width = parse_coordinate(&event, b"cx", transform.width)?;
                            transform.height = parse_coordinate(&event, b"cy", transform.height)?;
                        }
                    }
                    b"gridCol" => {
                        if let Some(table) = table.as_mut() {
                            table.columns.push(parse_coordinate(&event, b"w", 0.0)?);
                        }
                    }
                    b"pPr" => {
                        if let Some(cell) =
                            table.as_mut().and_then(|table| table.current_cell.as_mut())
                        {
                            cell.horizontal_align = match attr(&event, b"algn").as_deref() {
                                None | Some("l") => HorizontalAlign::Left,
                                Some("ctr") => HorizontalAlign::Center,
                                Some("r") => HorizontalAlign::Right,
                                _ => {
                                    cell.unsupported = true;
                                    HorizontalAlign::Left
                                }
                            };
                        }
                    }
                    b"rPr" => {
                        if let Some(cell) =
                            table.as_mut().and_then(|table| table.current_cell.as_mut())
                        {
                            if let Some(run) = cell.active_run.as_mut() {
                                parse_run_properties(&event, &mut run.style, report, slide_index);
                            }
                        }
                    }
                    b"latin" | b"ea" | b"cs" => {
                        if let Some(cell) =
                            table.as_mut().and_then(|table| table.current_cell.as_mut())
                        {
                            if let Some(run) = cell.active_run.as_mut() {
                                if let Some(typeface) = attr(&event, b"typeface") {
                                    set_run_font(&mut run.style, typeface, report, slide_index);
                                }
                            }
                        }
                    }
                    b"srgbClr" => {
                        if let Some(table) = table.as_mut() {
                            if table.in_solid_fill {
                                match attr(&event, b"val").and_then(|value| parse_hex_color(&value))
                                {
                                    Some(color) => {
                                        if let Some(cell) = table.current_cell.as_mut() {
                                            cell.fill = Paint::Solid(ColorRef::Rgba(color));
                                        }
                                    }
                                    None => table.unsupported = true,
                                }
                            }
                        }
                    }
                    b"schemeClr" => {
                        if let Some(table) = table.as_mut() {
                            if table.in_solid_fill {
                                match attr(&event, b"val")
                                    .as_deref()
                                    .and_then(|value| theme_color_token(value.as_bytes()))
                                {
                                    Some(color) => {
                                        if let Some(cell) = table.current_cell.as_mut() {
                                            cell.fill = Paint::Solid(ColorRef::Theme(color));
                                        }
                                    }
                                    None => table.unsupported = true,
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            Event::Text(value) if in_text => {
                if let Some(cell) = table.as_mut().and_then(|table| table.current_cell.as_mut()) {
                    cell.content.push_str(&value.unescape()?);
                }
            }
            Event::End(event) => {
                let name = event.name().as_ref().to_vec();
                match local_name(&name) {
                    b"t" => in_text = false,
                    b"r" => {
                        if let Some(cell) =
                            table.as_mut().and_then(|table| table.current_cell.as_mut())
                        {
                            if let Some(run) = cell.active_run.take() {
                                let end = cell.content.chars().count();
                                if run.start < end {
                                    cell.runs.push(PresentationTextRun {
                                        start: run.start,
                                        end,
                                        style: run.style,
                                    });
                                } else {
                                    cell.unsupported = true;
                                }
                            }
                        }
                    }
                    b"solidFill" => {
                        if let Some(table) = table.as_mut() {
                            table.in_solid_fill = false;
                        }
                    }
                    b"tcPr" => {
                        if let Some(table) = table.as_mut() {
                            table.in_tc_pr = false;
                        }
                    }
                    b"tc" => {
                        if let Some(table) = table.as_mut() {
                            if let Some(cell) = table.current_cell.take() {
                                table.cells.push(cell);
                            }
                        }
                    }
                    b"tr" => {
                        if let Some(table) = table.as_mut() {
                            table.current_row += 1;
                        }
                    }
                    b"tbl" => {
                        let Some(table) = table.take() else { continue };
                        if let Some(node) = finish_table_import(table, slide_index, report) {
                            nodes.push(node);
                        }
                    }
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(nodes)
}

fn finish_table_import(
    table: TableImportDraft,
    slide_index: usize,
    report: &mut PptxLossReport,
) -> Option<SceneNode> {
    let columns = table.columns.len();
    let rows = table.row_heights.len();
    let equal_columns = table
        .columns
        .first()
        .is_some_and(|first| table.columns.iter().all(|value| value == first));
    let equal_rows = table
        .row_heights
        .first()
        .is_some_and(|first| table.row_heights.iter().all(|value| value == first));
    let valid = !table.unsupported
        && columns > 0
        && rows > 0
        && equal_columns
        && equal_rows
        && table.cells.len() == rows * columns
        && table.cells.iter().all(|cell| {
            !cell.unsupported
                && cell.column
                    == (table
                        .cells
                        .iter()
                        .filter(|candidate| {
                            candidate.row == cell.row && candidate.column < cell.column
                        })
                        .count() as u32)
        });
    if !valid {
        report.unsupported.push(report_item(
            PptxReportKind::Unsupported,
            "presentationTable",
            format!("slides[{slide_index}].table"),
            "仅支持等宽等高、无合并、基础文本和填充的严格 TableNode 子集；该表格未导入",
            Some("简化表格后重新导入，或保留原始 PPTX source asset"),
        ));
        return None;
    }
    let cells = table
        .cells
        .into_iter()
        .map(|cell| TableCell {
            row: cell.row,
            column: cell.column,
            row_span: 1,
            column_span: 1,
            content: rich_text_with_runs(cell.content, normalize_runs(cell.runs)),
            style: TableCellStyle {
                fill: cell.fill,
                horizontal_align: cell.horizontal_align,
                vertical_align: cell.vertical_align,
            },
        })
        .collect();
    Some(SceneNode {
        id: table.id,
        parent_id: None,
        order_key: format!("{:08}", 50_000 + slide_index),
        name: table.name,
        alt_text: None,
        layout_placeholder_id: None,
        transform: table.transform,
        visible: true,
        locked: false,
        opacity: 1.0,
        kind: SceneNodeKind::Table(TableNode {
            rows: rows as u32,
            columns: columns as u32,
            cells,
        }),
    })
}

fn normalize_runs(runs: Vec<PresentationTextRun>) -> Vec<PresentationTextRun> {
    if runs
        .iter()
        .all(|run| run.style == PresentationTextStyle::default())
    {
        Vec::new()
    } else {
        runs
    }
}

fn rich_text_with_runs(text: String, runs: Vec<PresentationTextRun>) -> PresentationRichText {
    let mut body = PresentationRichText::plain(text);
    body.runs = runs;
    body
}

fn rich_text_with_runs_and_paragraphs(
    text: String,
    runs: Vec<PresentationTextRun>,
    paragraphs: Vec<PresentationParagraph>,
) -> PresentationRichText {
    let mut body = PresentationRichText::plain(text);
    body.runs = runs;
    if !paragraphs.is_empty() {
        body.paragraphs = paragraphs;
    }
    body
}

/// A connector can be losslessly represented by OOXML only when both endpoints are free points
/// derived from its unrotated transform.  Connected-shape `idx` values are vendor-specific and
/// cannot be truthfully converted to v5's semantic anchors without a mapping contract.
fn parse_supported_connectors(
    xml: &[u8],
    slide_index: usize,
    report: &mut PptxLossReport,
) -> Result<Vec<SceneNode>, PptxError> {
    let mut reader = Reader::from_reader(xml);
    let mut buffer = Vec::new();
    let mut current: Option<(String, Option<String>, NodeTransform, bool)> = None;
    let mut nodes = Vec::new();
    let mut sequence = 60_000usize;
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) | Event::Empty(event) => {
                let name = event.name().as_ref().to_vec();
                match local_name(&name) {
                    b"cxnSp" => {
                        current = Some((
                            format!("slide-{}-connector-{}", slide_index + 1, sequence),
                            None,
                            default_transform(sequence),
                            false,
                        ));
                        sequence += 1;
                    }
                    b"cNvPr" => {
                        if let Some((id, label, _, _)) = current.as_mut() {
                            if let Some(raw) = attr(&event, b"id") {
                                *id = format!("slide-{}-node-{raw}", slide_index + 1);
                            }
                            *label = attr(&event, b"name");
                        }
                    }
                    b"off" => {
                        if let Some((_, _, transform, _)) = current.as_mut() {
                            transform.x = parse_coordinate(&event, b"x", transform.x)?;
                            transform.y = parse_coordinate(&event, b"y", transform.y)?;
                        }
                    }
                    b"ext" => {
                        if let Some((_, _, transform, _)) = current.as_mut() {
                            transform.width = parse_coordinate(&event, b"cx", transform.width)?;
                            transform.height = parse_coordinate(&event, b"cy", transform.height)?;
                        }
                    }
                    b"stCxn" | b"endCxn" => {
                        if let Some((_, _, _, attached)) = current.as_mut() {
                            *attached = true;
                        }
                    }
                    _ => {}
                }
            }
            Event::End(event) if local_name(event.name().as_ref()) == b"cxnSp" => {
                let Some((id, name, transform, attached)) = current.take() else {
                    continue;
                };
                if attached || transform.rotation != 0.0 {
                    report.unsupported.push(report_item(PptxReportKind::Unsupported, "connector", format!("slides[{slide_index}].{id}"), "仅支持未旋转、两端均为 free point 的 connector；连接到节点的 idx 无法无损映射为 semantic anchor", Some("改为自由端点 connector，或保留原始 PPTX source asset")));
                } else {
                    nodes.push(SceneNode {
                        id,
                        parent_id: None,
                        order_key: format!("{:08}", sequence),
                        name,
                        alt_text: None,
                        layout_placeholder_id: None,
                        transform: transform.clone(),
                        visible: true,
                        locked: false,
                        opacity: 1.0,
                        kind: SceneNodeKind::Connector(ConnectorNode {
                            start: ConnectorEndpoint::Free(Point {
                                x: transform.x,
                                y: transform.y,
                            }),
                            end: ConnectorEndpoint::Free(Point {
                                x: transform.x + transform.width,
                                y: transform.y + transform.height,
                            }),
                        }),
                    });
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(nodes)
}

fn parse_coordinate(
    event: &quick_xml::events::BytesStart<'_>,
    name: &[u8],
    fallback: f64,
) -> Result<f64, PptxError> {
    attr(event, name).map_or(Ok(fallback), |value| {
        value
            .parse()
            .map_err(|_| PptxError::InvalidStructure(format!("OOXML 坐标 {value} 非法")))
    })
}

fn default_transform(sequence: usize) -> NodeTransform {
    NodeTransform {
        x: 0.0,
        y: sequence as f64 * 24.0 * EMU_PER_POINT,
        width: 240.0 * EMU_PER_POINT,
        height: 20.0 * EMU_PER_POINT,
        rotation: 0.0,
    }
}

fn scene_node_from_draft(draft: SlideNodeDraft, kind: SceneNodeKind) -> SceneNode {
    SceneNode {
        id: draft.id,
        parent_id: draft.parent_id,
        order_key: format!("{:08}", draft.sequence),
        name: draft.name,
        alt_text: None,
        layout_placeholder_id: None,
        transform: draft.transform,
        visible: true,
        locked: false,
        opacity: 1.0,
        kind,
    }
}

fn read_import_asset<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    relationship: &Relationship,
) -> Result<PptxImportedAsset, PptxError> {
    let bytes = read_relationship_part(archive, &relationship.target, MAX_MEDIA_PART_BYTES)?;
    let mime_type = mime_from_part(&relationship.target).ok_or_else(|| {
        PptxError::InvalidStructure(format!("不支持的嵌入图片类型：{}", relationship.target))
    })?;
    let detected_extension = image_extension(&bytes).ok_or_else(|| {
        PptxError::InvalidStructure(format!(
            "嵌入图片内容不是允许的 PNG/JPEG/GIF/WebP：{}",
            relationship.target
        ))
    })?;
    let declared_extension = relationship
        .target
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .expect("mime_from_part already checked an extension");
    let normalized_declared = if declared_extension == "jpg" {
        "jpeg"
    } else {
        declared_extension.as_str()
    };
    if detected_extension != normalized_declared {
        return Err(PptxError::InvalidStructure(format!(
            "嵌入图片扩展名与内容不匹配：{} 声明为 {}, 实际为 {}",
            relationship.target, normalized_declared, detected_extension
        )));
    }
    let digest = hex::encode(Sha256::digest(&bytes));
    Ok(PptxImportedAsset {
        asset: AssetRef {
            asset_id: format!("asset-{digest}"),
            digest,
            mime_type: mime_type.into(),
            width: None,
            height: None,
            original_asset_id: None,
        },
        bytes,
    })
}

fn mime_from_part(part: &str) -> Option<&'static str> {
    match part.rsplit('.').next()?.to_ascii_lowercase().as_str() {
        "png" => Some("image/png"),
        "jpg" | "jpeg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

/// The minimal writer intentionally has a small surface.  Enumerating every unsupported typed
/// field here is more important than producing a visually plausible but semantically damaged
/// package.  `write_pptx` turns any entry in this report into a hard error.
fn report_unsupported_export_fields(deck: &Deck, report: &mut PptxLossReport) {
    if deck.page_spec.safe_area.is_some() {
        report.unsupported.push(report_item(
            PptxReportKind::Unsupported,
            "pageSafeArea",
            "deck.pageSpec.safeArea",
            "writer 不会把 editor safe area 编码为 OOXML 页面属性",
            Some("保留在 canonical Deck，或在安全区域 writer 完成后导出"),
        ));
    }
    if !deck.masters.is_empty() {
        report.unsupported.push(report_item(
            PptxReportKind::Unsupported,
            "master",
            "deck.masters",
            "writer 尚未生成可交换的 slide master parts",
            Some("使用 write_pptx_with_report 获取报告；不要调用严格 write_pptx"),
        ));
    }
    if !deck.layouts.is_empty() {
        report.unsupported.push(report_item(
            PptxReportKind::Unsupported,
            "layout",
            "deck.layouts",
            "writer 尚未生成可交换的 slide layout parts",
            Some("使用 write_pptx_with_report 获取报告；不要调用严格 write_pptx"),
        ));
    }
    if !deck.theme.name.is_empty() || !deck.theme.colors.is_empty() || !deck.theme.fonts.is_empty()
    {
        report.unsupported.push(report_item(
            PptxReportKind::Unsupported,
            "theme",
            "deck.theme",
            "writer 尚未生成 OOXML theme part，不能安全写回 theme 元数据、颜色或字体",
            Some("保留 canonical Deck theme，等待 theme writer 完成"),
        ));
    }

    for slide in &deck.slides {
        let slide_path = format!("slides/{}", slide.id);
        if slide.layout_id.is_some() {
            report.unsupported.push(report_item(
                PptxReportKind::Unsupported,
                "slideLayoutReference",
                &slide_path,
                "writer 不会写出 slideLayout relationship",
                Some("移除 layout 引用后导出，或等待 master/layout writer"),
            ));
        }
        if slide.background != SlideBackground::None {
            report.unsupported.push(report_item(
                PptxReportKind::Unsupported,
                "slideBackground",
                &slide_path,
                "writer 尚未写出 slide background fill",
                Some("使用默认背景，或等待 background writer"),
            ));
        }
        for node in &slide.nodes {
            report_unsupported_node_export_fields(node, report);
        }
    }
}

fn report_unsupported_node_export_fields(node: &SceneNode, report: &mut PptxLossReport) {
    if node.alt_text.is_some() {
        report.unsupported.push(report_item(
            PptxReportKind::Unsupported,
            "nodeAltText",
            &node.id,
            "writer 尚未写出 cNvPr descr",
            Some("清空 altText 后导出，或等待 accessibility metadata writer"),
        ));
    }
    if node.layout_placeholder_id.is_some() {
        report.unsupported.push(report_item(
            PptxReportKind::Unsupported,
            "layoutPlaceholderReference",
            &node.id,
            "writer 尚未写出 placeholder relationship",
            Some("移除 placeholder 引用后导出，或等待 master/layout writer"),
        ));
    }
    if !node.visible || node.locked || node.opacity != 1.0 {
        report.unsupported.push(report_item(
            PptxReportKind::Unsupported,
            "nodeViewState",
            &node.id,
            "writer 尚未写出 visible/locked/opacity 状态",
            Some("恢复默认节点视图状态后导出"),
        ));
    }
    match &node.kind {
        SceneNodeKind::Text(text) => {
            report_unsupported_text_runs(node, &text.frame.body, report);
        }
        SceneNodeKind::Shape(shape) if shape.style != ShapeStyle::default() => {
            report.unsupported.push(report_item(
                PptxReportKind::Unsupported,
                "shapeStyle",
                &node.id,
                "writer 尚未写出 shape fill/stroke",
                Some("使用无 fill/无 stroke 的基础 shape，或等待 style writer"),
            ));
        }
        SceneNodeKind::Image(image)
            if image.crop != Default::default()
                || image.flip_h
                || image.flip_v
                || image.caption.is_some()
                || image.original_asset_id.is_some() =>
        {
            report.unsupported.push(report_item(
                PptxReportKind::Unsupported,
                "imageConfig",
                &node.id,
                "writer 尚未写出 crop/flip/caption/originalAsset",
                Some("使用原始图片配置，或等待 image config writer"),
            ));
        }
        // ChartSpec is canonical and editable, but the PPTX adapter does not
        // yet own a chart-part writer.  Keep that loss explicit rather than
        // emitting a visually plausible shape or silently dropping series.
        SceneNodeKind::Chart(_) => report.unsupported.push(report_item(
            PptxReportKind::Unsupported,
            "chartSpec",
            &node.id,
            "writer 尚未生成可交换的 OOXML chart part；不会静默丢弃 categories 或 series",
            Some("使用 write_pptx_with_report 获取 loss report，或等待 chart-part writer"),
        )),
        SceneNodeKind::Table(table) if !table_is_lossless_pptx_subset(table) => {
            report.unsupported.push(report_item(
                PptxReportKind::Unsupported,
                "presentationTable",
                &node.id,
                "writer 仅支持等宽等高、无合并、基础文本与 no/solid fill 的 TableNode 子集",
                Some("简化表格或使用 write_pptx_with_report 检查不可逆字段"),
            ))
        }
        SceneNodeKind::Connector(connector)
            if !connector_is_lossless_pptx_subset(node, connector) =>
        {
            report.unsupported.push(report_item(
                PptxReportKind::Unsupported,
                "connector",
                &node.id,
                "writer 仅支持未旋转、两端为 transform 起止点的 free connector",
                Some("改为自由端点 connector，或保留原始 PPTX source asset"),
            ))
        }
        SceneNodeKind::Shape(_)
        | SceneNodeKind::Image(_)
        | SceneNodeKind::Group(_)
        | SceneNodeKind::Table(_)
        | SceneNodeKind::Connector(_) => {}
        kind => report.unsupported.push(report_item(
            PptxReportKind::Unsupported,
            "sceneNode",
            &node.id,
            format!("{kind:?} 尚未定义 OOXML writer"),
            Some("保留在 Deck；不要静默导出"),
        )),
    }
}

fn table_is_lossless_pptx_subset(table: &TableNode) -> bool {
    table.rows > 0
        && table.columns > 0
        && table.cells.len() == (table.rows as usize) * (table.columns as usize)
        && table.cells.iter().all(|cell| {
            cell.row_span == 1
                && cell.column_span == 1
                && !cell.content.text.contains('\n')
                && cell.content.runs.iter().all(|run| {
                    run.style.color.as_ref().is_none_or(
                        |color| !matches!(color, ColorRef::Rgba(value) if value.a != u8::MAX),
                    )
                })
        })
}

fn connector_is_lossless_pptx_subset(node: &SceneNode, connector: &ConnectorNode) -> bool {
    if node.transform.rotation != 0.0 {
        return false;
    }
    let (ConnectorEndpoint::Free(start), ConnectorEndpoint::Free(end)) =
        (&connector.start, &connector.end)
    else {
        return false;
    };
    start.x == node.transform.x
        && start.y == node.transform.y
        && end.x == node.transform.x + node.transform.width
        && end.y == node.transform.y + node.transform.height
}

fn report_unsupported_text_runs(
    node: &SceneNode,
    body: &PresentationRichText,
    report: &mut PptxLossReport,
) {
    if body.runs.is_empty() {
        return;
    }
    if body.text.contains('\n') {
        report.unsupported.push(report_item(
            PptxReportKind::Unsupported,
            "richTextParagraph",
            &node.id,
            "带 run 样式的多段文本尚未映射为 OOXML a:p run tree",
            Some("移除换行或等待 paragraph writer；不会静默扁平化样式"),
        ));
    }
    for run in &body.runs {
        if run
            .style
            .color
            .as_ref()
            .is_some_and(|color| matches!(color, ColorRef::Rgba(value) if value.a != u8::MAX))
        {
            report.unsupported.push(report_item(
                PptxReportKind::Unsupported,
                "textRunAlpha",
                &node.id,
                "DrawingML text alpha 尚未纳入严格 run writer，不能以近似值导出",
                Some("使用不透明颜色，或等待 alpha writer"),
            ));
            break;
        }
    }
}

struct ExportPackage {
    content_types: String,
    xml_parts: Vec<(String, String)>,
    binary_parts: Vec<(String, Vec<u8>)>,
}

impl ExportPackage {
    fn build(
        deck: &Deck,
        assets: &PptxAssetSource,
        report: &mut PptxLossReport,
    ) -> Result<Self, PptxError> {
        report_unsupported_export_fields(deck, report);
        let has_notes = deck.slides.iter().any(|slide| slide.notes.is_some());
        let mut xml_parts = vec![
            (PRESENTATION_PART.into(), presentation_xml(deck)),
            (
                "ppt/_rels/presentation.xml.rels".into(),
                presentation_rels(deck.slides.len(), has_notes),
            ),
        ];
        if has_notes {
            xml_parts.push((
                "ppt/notesMasters/notesMaster1.xml".into(),
                notes_master_xml(),
            ));
        }
        let mut binary_parts = Vec::new();
        for (index, slide) in deck.slides.iter().enumerate() {
            let (xml, rels) = slide_xml(slide, index, assets, &mut binary_parts, report);
            xml_parts.push((format!("ppt/slides/slide{}.xml", index + 1), xml));
            xml_parts.push((
                format!("ppt/slides/_rels/slide{}.xml.rels", index + 1),
                rels,
            ));
            if let Some(notes) = &slide.notes {
                xml_parts.push((
                    format!("ppt/notesSlides/notesSlide{}.xml", index + 1),
                    notes_slide_xml(notes),
                ));
                xml_parts.push((
                    format!("ppt/notesSlides/_rels/notesSlide{}.xml.rels", index + 1),
                    notes_slide_rels(index),
                ));
            }
        }
        Ok(Self {
            content_types: content_types(deck),
            xml_parts,
            binary_parts,
        })
    }
}

fn presentation_xml(deck: &Deck) -> String {
    let ids = deck
        .slides
        .iter()
        .enumerate()
        .map(|(index, _)| {
            format!(
                "<p:sldId id=\"{}\" r:id=\"rId{}\"/>",
                256 + index,
                index + 1
            )
        })
        .collect::<String>();
    let notes_master = if deck.slides.iter().any(|slide| slide.notes.is_some()) {
        format!(
            "<p:notesMasterIdLst><p:notesMasterId r:id=\"rId{}\"/></p:notesMasterIdLst>",
            deck.slides.len() + 1
        )
    } else {
        String::new()
    };
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:presentation xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\">{notes_master}<p:sldIdLst>{ids}</p:sldIdLst><p:sldSz cx=\"{}\" cy=\"{}\" type=\"screen16x9\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>", deck.page_spec.width.round(), deck.page_spec.height.round())
}

fn presentation_rels(count: usize, has_notes: bool) -> String {
    let mut rels = (0..count).map(|index| format!("<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"slides/slide{}.xml\"/>", index + 1, index + 1)).collect::<String>();
    if has_notes {
        rels.push_str(&format!("<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster\" Target=\"notesMasters/notesMaster1.xml\"/>", count + 1));
    }
    relationships_xml(&rels)
}

fn relationships_xml(rels: &str) -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{rels}</Relationships>")
}

fn slide_xml(
    slide: &Slide,
    slide_index: usize,
    assets: &PptxAssetSource,
    binary_parts: &mut Vec<(String, Vec<u8>)>,
    report: &mut PptxLossReport,
) -> (String, String) {
    let mut nodes = String::new();
    let mut relationships = String::new();
    {
        let mut context = SlideExportContext {
            all_nodes: &slide.nodes,
            assets,
            binary_parts,
            relationships: &mut relationships,
            image_index: 1,
            report,
        };
        for node in slide.nodes.iter().filter(|node| node.parent_id.is_none()) {
            append_node_xml(node, &mut nodes, &mut context);
        }
    }
    if slide.notes.is_some() {
        relationships.push_str(&format!("<Relationship Id=\"rIdNotes\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesSlide\" Target=\"../notesSlides/notesSlide{}.xml\"/>", slide_index + 1));
    }
    let transition = slide
        .transition
        .as_ref()
        .map(transition_xml)
        .unwrap_or_default();
    let timing = timeline_xml(slide);
    let slide_name = if slide.name.is_empty() {
        String::new()
    } else {
        format!(" name=\"{}\"", xml_escaped(&slide.name))
    };
    let xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sld xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\" xmlns:p14=\"http://schemas.microsoft.com/office/powerpoint/2010/main\"><p:cSld{slide_name}><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{nodes}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr>{transition}{timing}</p:sld>");
    (xml, relationships_xml(&relationships))
}

fn transition_xml(transition: &SlideTransition) -> String {
    let effect = match transition.kind {
        TransitionKind::None => "cut",
        TransitionKind::Fade => "fade",
        TransitionKind::Push => "push",
        TransitionKind::Wipe => "wipe",
    };
    format!(
        "<p:transition p14:dur=\"{}\"><p:{effect}/></p:transition>",
        transition.duration_ms
    )
}

fn timeline_xml(slide: &Slide) -> String {
    if slide.timeline.entries.is_empty() {
        return String::new();
    }
    let mut entries = slide.timeline.entries.iter().collect::<Vec<_>>();
    entries.sort_by(|left, right| left.order_key.cmp(&right.order_key));
    let mut groups = Vec::<(bool, Vec<&AnimationEntry>)>::new();
    for entry in entries {
        if entry.trigger == AnimationTrigger::OnClick || groups.is_empty() {
            groups.push((entry.trigger != AnimationTrigger::OnClick, vec![entry]));
        } else {
            groups.last_mut().expect("created above").1.push(entry);
        }
    }

    let mut next_id = 3u32;
    let groups = groups
        .into_iter()
        .map(|(automatic, entries)| {
            let group_id = next_id;
            let container_id = next_id + 1;
            next_id += 2;
            let effects = entries
                .into_iter()
                .map(|entry| animation_entry_xml(entry, &mut next_id))
                .collect::<String>();
            let start_delay = if automatic { "0" } else { "indefinite" };
            format!("<p:par><p:cTn id=\"{group_id}\" fill=\"hold\"><p:stCondLst><p:cond delay=\"{start_delay}\"/></p:stCondLst><p:childTnLst><p:par><p:cTn id=\"{container_id}\" fill=\"hold\"><p:stCondLst><p:cond delay=\"0\"/></p:stCondLst><p:childTnLst>{effects}</p:childTnLst></p:cTn></p:par></p:childTnLst></p:cTn></p:par>")
        })
        .collect::<String>();
    format!("<p:timing><p:tnLst><p:par><p:cTn id=\"1\" dur=\"indefinite\" restart=\"never\" nodeType=\"tmRoot\"><p:childTnLst><p:seq concurrent=\"1\" nextAc=\"seek\"><p:cTn id=\"2\" dur=\"indefinite\" nodeType=\"mainSeq\"><p:childTnLst>{groups}</p:childTnLst></p:cTn></p:seq></p:childTnLst></p:cTn></p:par></p:tnLst></p:timing>")
}

fn animation_entry_xml(entry: &AnimationEntry, next_id: &mut u32) -> String {
    let effect_id = *next_id;
    let behavior_id = *next_id + 1;
    *next_id += 2;
    let (preset_id, behavior) = match entry.preset {
        AnimationPreset::Appear => (
            1,
            format!("<p:set><p:cBhvr><p:cTn id=\"{behavior_id}\" dur=\"{}\" fill=\"hold\"/><p:tgtEl><p:spTgt spid=\"{}\"/></p:tgtEl><p:attrNameLst><p:attrName>style.visibility</p:attrName></p:attrNameLst></p:cBhvr><p:to><p:strVal val=\"visible\"/></p:to></p:set>", entry.duration_ms, numeric_id(&entry.target_node_id)),
        ),
        AnimationPreset::Fade => (
            10,
            animation_effect_xml("fade", entry, behavior_id),
        ),
        AnimationPreset::FlyIn => (
            2,
            animation_effect_xml("fly(right)", entry, behavior_id),
        ),
        AnimationPreset::Wipe => (
            22,
            animation_effect_xml("wipe(right)", entry, behavior_id),
        ),
    };
    let node_type = match entry.trigger {
        AnimationTrigger::OnClick => "clickEffect",
        AnimationTrigger::WithPrevious => "withEffect",
        AnimationTrigger::AfterPrevious => "afterEffect",
    };
    format!("<p:par><p:cTn id=\"{effect_id}\" presetID=\"{preset_id}\" presetClass=\"entr\" presetSubtype=\"0\" fill=\"hold\" nodeType=\"{node_type}\"><p:stCondLst><p:cond delay=\"{}\"/></p:stCondLst><p:childTnLst>{behavior}</p:childTnLst></p:cTn></p:par>", entry.delay_ms)
}

fn animation_effect_xml(filter: &str, entry: &AnimationEntry, behavior_id: u32) -> String {
    format!("<p:animEffect transition=\"in\" filter=\"{filter}\"><p:cBhvr><p:cTn id=\"{behavior_id}\" dur=\"{}\" fill=\"hold\"/><p:tgtEl><p:spTgt spid=\"{}\"/></p:tgtEl></p:cBhvr></p:animEffect>", entry.duration_ms, numeric_id(&entry.target_node_id))
}

fn notes_master_xml() -> String {
    "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:notesMaster xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/></p:spTree></p:cSld><p:clrMap accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" bg1=\"lt1\" bg2=\"lt2\" folHlink=\"folHlink\" hlink=\"hlink\" tx1=\"dk1\" tx2=\"dk2\"/></p:notesMaster>".into()
}

fn notes_slide_xml(notes: &str) -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:notes xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/><p:sp><p:nvSpPr><p:cNvPr id=\"2\" name=\"Notes Placeholder\"/><p:cNvSpPr/><p:nvPr><p:ph type=\"body\"/></p:nvPr></p:nvSpPr><p:spPr/><p:txBody><a:bodyPr/><a:lstStyle/><a:p><a:r><a:t xml:space=\"preserve\">{}</a:t></a:r></a:p></p:txBody></p:sp></p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:notes>", xml_escaped(notes))
}

fn notes_slide_rels(slide_index: usize) -> String {
    relationships_xml(&format!("<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"../slides/slide{}.xml\"/><Relationship Id=\"rId2\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/notesMaster\" Target=\"../notesMasters/notesMaster1.xml\"/>", slide_index + 1))
}

struct SlideExportContext<'a> {
    all_nodes: &'a [SceneNode],
    assets: &'a PptxAssetSource,
    binary_parts: &'a mut Vec<(String, Vec<u8>)>,
    relationships: &'a mut String,
    image_index: usize,
    report: &'a mut PptxLossReport,
}

fn append_node_xml(node: &SceneNode, output: &mut String, context: &mut SlideExportContext<'_>) {
    match &node.kind {
        SceneNodeKind::Text(text) => output.push_str(&text_xml(node, &text.frame)),
        SceneNodeKind::Shape(shape) => output.push_str(&shape_xml(node, shape.geometry)),
        SceneNodeKind::Group(_) => {
            let children = context
                .all_nodes
                .iter()
                .filter(|candidate| candidate.parent_id.as_deref() == Some(&node.id))
                .map(|candidate| {
                    let mut child = String::new();
                    append_node_xml(candidate, &mut child, context);
                    child
                })
                .collect::<String>();
            output.push_str(&format!("<p:grpSp><p:nvGrpSpPr><p:cNvPr id=\"{}\" name=\"{}\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr>{}</p:grpSpPr>{children}</p:grpSp>", numeric_id(&node.id), xml_escaped(&node.name.clone().unwrap_or_default()), xfrm_xml(node)));
        }
        SceneNodeKind::Image(image) => {
            let Some(bytes) = context.assets.get(&image.asset_id) else {
                context.report.unsupported.push(report_item(
                    PptxReportKind::Unsupported,
                    "image",
                    &node.id,
                    format!("asset {} 缺少 writer bytes", image.asset_id),
                    Some("调用 write_pptx_with_assets 并提供 asset bytes"),
                ));
                return;
            };
            let Some(extension) = image_extension(bytes) else {
                context.report.unsupported.push(report_item(
                    PptxReportKind::Invalid,
                    "image",
                    &node.id,
                    format!("asset {} 不是允许的 PNG/JPEG/GIF/WebP 数据", image.asset_id),
                    Some("通过 asset service 提供受支持的已验证图片"),
                ));
                return;
            };
            context.binary_parts.push((
                format!("ppt/media/image{}.{}", context.image_index, extension),
                bytes.clone(),
            ));
            context.relationships.push_str(&format!("<Relationship Id=\"rIdImage{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"../media/image{}.{}\"/>", context.image_index, context.image_index, extension));
            output.push_str(&format!("<p:pic><p:nvPicPr><p:cNvPr id=\"{}\" name=\"{}\"/><p:cNvPicPr/><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"rIdImage{}\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr></p:pic>", numeric_id(&node.id), xml_escaped(&node.name.clone().unwrap_or_default()), context.image_index, xfrm_xml(node)));
            context.image_index += 1;
        }
        SceneNodeKind::Table(table) if table_is_lossless_pptx_subset(table) => {
            output.push_str(&table_xml(node, table));
        }
        SceneNodeKind::Connector(connector)
            if connector_is_lossless_pptx_subset(node, connector) =>
        {
            output.push_str(&connector_xml(node));
        }
        // Unsupported variants have already been recorded by
        // `report_unsupported_node_export_fields` before package construction.  Do not add a
        // second, less precise report here and do not emit a placeholder XML node.
        _ => {}
    }
}

fn connector_xml(node: &SceneNode) -> String {
    format!(
        "<p:cxnSp><p:nvCxnSpPr><p:cNvPr id=\"{}\" name=\"{}\"/><p:cNvCxnSpPr/><p:nvPr/></p:nvCxnSpPr><p:spPr>{}<a:prstGeom prst=\"line\"><a:avLst/></a:prstGeom></p:spPr></p:cxnSp>",
        numeric_id(&node.id),
        xml_escaped(&node.name.clone().unwrap_or_default()),
        xfrm_xml(node),
    )
}

fn table_xml(node: &SceneNode, table: &TableNode) -> String {
    let column_width = node.transform.width / table.columns as f64;
    let row_height = node.transform.height / table.rows as f64;
    let grid = (0..table.columns)
        .map(|_| format!("<a:gridCol w=\"{}\"/>", column_width.round()))
        .collect::<String>();
    let rows = (0..table.rows)
        .map(|row| {
            let cells = (0..table.columns)
                .map(|column| {
                    let cell = table
                        .cells
                        .iter()
                        .find(|cell| cell.row == row && cell.column == column)
                        .expect("validated strict table contains every cell");
                    table_cell_xml(cell)
                })
                .collect::<String>();
            format!("<a:tr h=\"{}\">{cells}</a:tr>", row_height.round())
        })
        .collect::<String>();
    format!(
        "<p:graphicFrame><p:nvGraphicFramePr><p:cNvPr id=\"{}\" name=\"{}\"/><p:cNvGraphicFramePr/><p:nvPr/></p:nvGraphicFramePr><p:xfrm>{}</p:xfrm><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/table\"><a:tbl><a:tblPr firstRow=\"0\" firstCol=\"0\" lastRow=\"0\" lastCol=\"0\" bandRow=\"0\" bandCol=\"0\"/><a:tblGrid>{grid}</a:tblGrid>{rows}</a:tbl></a:graphicData></a:graphic></p:graphicFrame>",
        numeric_id(&node.id),
        xml_escaped(&node.name.clone().unwrap_or_default()),
        graphic_frame_xfrm_xml(node),
    )
}

fn table_cell_xml(cell: &TableCell) -> String {
    let anchor = match cell.style.vertical_align {
        TextVerticalAlign::Top => "t",
        TextVerticalAlign::Middle => "ctr",
        TextVerticalAlign::Bottom => "b",
    };
    let mut content = cell.content.clone();
    let alignment = match cell.style.horizontal_align {
        HorizontalAlign::Left => TextHorizontalAlign::Left,
        HorizontalAlign::Center => TextHorizontalAlign::Center,
        HorizontalAlign::Right => TextHorizontalAlign::Right,
    };
    for paragraph in &mut content.paragraphs {
        paragraph.alignment = alignment;
    }
    let body = rich_text_paragraph_xml(&content);
    format!(
        "<a:tc><a:txBody><a:bodyPr/><a:lstStyle/>{body}</a:txBody><a:tcPr anchor=\"{anchor}\">{}</a:tcPr></a:tc>",
        paint_xml(&cell.style.fill),
    )
}

fn rich_text_paragraph_xml(body: &PresentationRichText) -> String {
    let runs = if body.runs.is_empty() {
        vec![PresentationTextRun {
            start: 0,
            end: body.text.chars().count(),
            style: PresentationTextStyle::default(),
        }]
    } else {
        body.runs.clone()
    };
    let chars = body.text.chars().collect::<Vec<_>>();
    let paragraphs = if body.paragraphs.is_empty() {
        vec![PresentationParagraph {
            start: 0,
            end: chars.len(),
            alignment: TextHorizontalAlign::Left,
            list: None,
            indent_level: 0,
        }]
    } else {
        body.paragraphs.clone()
    };
    paragraphs
        .iter()
        .map(|paragraph| {
            let content_end =
                if paragraph.end > paragraph.start && chars.get(paragraph.end - 1) == Some(&'\n') {
                    paragraph.end - 1
                } else {
                    paragraph.end
                };
            let content = runs
                .iter()
                .filter_map(|run| {
                    let start = run.start.max(paragraph.start);
                    let end = run.end.min(content_end);
                    (start < end).then(|| {
                        text_run_xml(&chars[start..end].iter().collect::<String>(), &run.style)
                    })
                })
                .collect::<String>();
            format!(
                "<a:p>{}{}</a:p>",
                paragraph_properties_xml(paragraph),
                content
            )
        })
        .collect()
}

fn paragraph_properties_xml(paragraph: &PresentationParagraph) -> String {
    let alignment = match paragraph.alignment {
        TextHorizontalAlign::Left => "l",
        TextHorizontalAlign::Center => "ctr",
        TextHorizontalAlign::Right => "r",
        TextHorizontalAlign::Justify => "just",
    };
    let list = match &paragraph.list {
        None => "<a:buNone/>".to_owned(),
        Some(PresentationListStyle::Bullet) => "<a:buChar char=\"•\"/>".to_owned(),
        Some(PresentationListStyle::Ordered { start_at }) => {
            format!("<a:buAutoNum type=\"arabicPeriod\" startAt=\"{start_at}\"/>")
        }
    };
    format!(
        "<a:pPr algn=\"{alignment}\" lvl=\"{}\">{list}</a:pPr>",
        paragraph.indent_level
    )
}

fn graphic_frame_xfrm_xml(node: &SceneNode) -> String {
    format!(
        "<a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/>",
        node.transform.x.round(),
        node.transform.y.round(),
        node.transform.width.round(),
        node.transform.height.round(),
    )
}

fn paint_xml(paint: &Paint) -> String {
    match paint {
        Paint::None => "<a:noFill/>".into(),
        Paint::Solid(ColorRef::Rgba(value)) => format!(
            "<a:solidFill><a:srgbClr val=\"{:02X}{:02X}{:02X}\"/></a:solidFill>",
            value.r, value.g, value.b
        ),
        Paint::Solid(ColorRef::Theme(token)) => format!(
            "<a:solidFill><a:schemeClr val=\"{}\"/></a:solidFill>",
            theme_color_name(token)
        ),
    }
}

fn image_extension(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("gif")
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some("webp")
    } else {
        None
    }
}

fn numeric_id(id: &str) -> u32 {
    let digest = Sha256::digest(id.as_bytes());
    u32::from_be_bytes([digest[0], digest[1], digest[2], digest[3]]).max(2)
}

fn xfrm_xml(node: &SceneNode) -> String {
    format!(
        "<a:xfrm rot=\"{}\"><a:off x=\"{}\" y=\"{}\"/><a:ext cx=\"{}\" cy=\"{}\"/></a:xfrm>",
        (node.transform.rotation * 60_000.0).round(),
        node.transform.x.round(),
        node.transform.y.round(),
        node.transform.width.round(),
        node.transform.height.round()
    )
}

fn text_xml(node: &SceneNode, frame: &TextFrame) -> String {
    let content = rich_text_paragraph_xml(&frame.body);
    let body_properties = text_body_properties_xml(frame);
    format!("<p:sp><p:nvSpPr><p:cNvPr id=\"{}\" name=\"{}\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr><p:txBody>{body_properties}<a:lstStyle/>{content}</p:txBody></p:sp>", numeric_id(&node.id), xml_escaped(&node.name.clone().unwrap_or_default()), xfrm_xml(node))
}

fn text_body_properties_xml(frame: &TextFrame) -> String {
    let anchor = match frame.vertical_align {
        TextVerticalAlign::Top => "t",
        TextVerticalAlign::Middle => "ctr",
        TextVerticalAlign::Bottom => "b",
    };
    let auto_fit = match frame.auto_fit {
        TextAutoFit::None => "<a:noAutofit/>",
        TextAutoFit::ShrinkText => "<a:normAutofit/>",
        TextAutoFit::ResizeShape => "<a:spAutoFit/>",
    };
    format!(
        "<a:bodyPr anchor=\"{anchor}\" tIns=\"{}\" rIns=\"{}\" bIns=\"{}\" lIns=\"{}\">{auto_fit}</a:bodyPr>",
        frame.padding.top.round(),
        frame.padding.right.round(),
        frame.padding.bottom.round(),
        frame.padding.left.round(),
    )
}

fn text_run_xml(text: &str, style: &PresentationTextStyle) -> String {
    let properties = text_run_properties_xml(style);
    format!("<a:r>{properties}<a:t>{}</a:t></a:r>", xml_escaped(text))
}

fn set_run_font(
    style: &mut PresentationTextStyle,
    family: String,
    report: &mut PptxLossReport,
    slide_index: usize,
) {
    if family.trim().is_empty() {
        return;
    }
    if style
        .font_family
        .as_ref()
        .is_some_and(|existing| existing != &family)
    {
        report_text_run_unsupported(report, slide_index, "同一文字 run 使用不同的 Latin/East Asian/complex-script 字体；canonical run 仅支持一个字体");
    } else {
        style.font_family = Some(family);
    }
}

fn text_run_properties_xml(style: &PresentationTextStyle) -> String {
    let mut attributes = String::new();
    if style.bold {
        attributes.push_str(" b=\"1\"");
    }
    if style.italic {
        attributes.push_str(" i=\"1\"");
    }
    if style.underline {
        attributes.push_str(" u=\"sng\"");
    }
    if style.strikethrough {
        attributes.push_str(" strike=\"sngStrike\"");
    }
    if let Some(size) = style.font_size {
        attributes.push_str(&format!(" sz=\"{}\"", (size * 100.0).round()));
    }
    let latin = style
        .font_family
        .as_ref()
        .map(|family| {
            let family = xml_escaped(&oo_schema::font_family::primary_font_family(family));
            format!("<a:latin typeface=\"{family}\"/><a:ea typeface=\"{family}\"/><a:cs typeface=\"{family}\"/>")
        })
        .unwrap_or_default();
    let color = style.color.as_ref().map(text_color_xml).unwrap_or_default();
    format!("<a:rPr{attributes}>{color}{latin}</a:rPr>")
}

fn text_color_xml(color: &ColorRef) -> String {
    match color {
        ColorRef::Rgba(value) => format!(
            "<a:solidFill><a:srgbClr val=\"{:02X}{:02X}{:02X}\"/></a:solidFill>",
            value.r, value.g, value.b
        ),
        ColorRef::Theme(token) => format!(
            "<a:solidFill><a:schemeClr val=\"{}\"/></a:solidFill>",
            theme_color_name(token)
        ),
    }
}

fn theme_color_name(token: &ThemeColorToken) -> &'static str {
    match token {
        ThemeColorToken::Background => "lt1",
        ThemeColorToken::Text => "dk1",
        ThemeColorToken::Accent1 => "accent1",
        ThemeColorToken::Accent2 => "accent2",
        ThemeColorToken::Accent3 => "accent3",
        ThemeColorToken::Accent4 => "accent4",
        ThemeColorToken::Accent5 => "accent5",
        ThemeColorToken::Accent6 => "accent6",
        ThemeColorToken::Hyperlink => "hlink",
        ThemeColorToken::FollowedHyperlink => "folHlink",
    }
}

fn shape_xml(node: &SceneNode, geometry: ShapeGeometry) -> String {
    let geometry = match geometry {
        ShapeGeometry::Rectangle => "rect",
        ShapeGeometry::Ellipse => "ellipse",
        ShapeGeometry::Line => "line",
        ShapeGeometry::Arrow => "rightArrow",
    };
    format!("<p:sp><p:nvSpPr><p:cNvPr id=\"{}\" name=\"{}\"/><p:cNvSpPr/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst=\"{}\"><a:avLst/></a:prstGeom></p:spPr></p:sp>", numeric_id(&node.id), xml_escaped(&node.name.clone().unwrap_or_default()), xfrm_xml(node), geometry)
}

fn xml_escaped(source: &str) -> String {
    let mut value = String::new();
    xml_escape(source, &mut value);
    value
}
fn xml_escape(source: &str, output: &mut String) {
    for c in source.chars() {
        match c {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '\"' => output.push_str("&quot;"),
            '\'' => output.push_str("&apos;"),
            _ => output.push(c),
        }
    }
}

const ROOT_RELS: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\"><Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument\" Target=\"ppt/presentation.xml\"/></Relationships>";
fn content_types(deck: &Deck) -> String {
    let slides = deck.slides.iter().enumerate().map(|(index, _)| format!("<Override PartName=\"/ppt/slides/slide{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>", index + 1)).collect::<String>();
    let notes_slides = deck
        .slides
        .iter()
        .enumerate()
        .filter(|(_, slide)| slide.notes.is_some())
        .map(|(index, _)| format!("<Override PartName=\"/ppt/notesSlides/notesSlide{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.notesSlide+xml\"/>", index + 1))
        .collect::<String>();
    let notes_master = if notes_slides.is_empty() {
        ""
    } else {
        "<Override PartName=\"/ppt/notesMasters/notesMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.notesMaster+xml\"/>"
    };
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Default Extension=\"jpg\" ContentType=\"image/jpeg\"/><Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/><Default Extension=\"gif\" ContentType=\"image/gif\"/><Default Extension=\"webp\" ContentType=\"image/webp\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/>{slides}{notes_slides}{notes_master}</Types>")
}

#[cfg(test)]
mod tests {
    use super::*;
    use oo_schema::presentation_v5::{ChartNode, ChartSeries, ChartSpec, ChartType};

    fn package(parts: impl IntoIterator<Item = (String, Vec<u8>)>) -> Vec<u8> {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in parts {
            archive
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            archive.write_all(&bytes).unwrap();
        }
        archive.finish().unwrap().into_inner()
    }

    #[test]
    fn office_font_fields_use_a_family_name_for_all_scripts() {
        let xml = text_run_properties_xml(&PresentationTextStyle {
            font_family: Some("\"Noto Serif SC\", serif".into()),
            ..PresentationTextStyle::default()
        });
        for script in ["latin", "ea", "cs"] {
            assert!(xml.contains(&format!("<a:{script} typeface=\"Noto Serif SC\"/>")));
        }
        assert!(!xml.contains(", serif"));
    }

    #[test]
    fn notes_parser_reads_only_body_placeholder_and_concatenates_runs_per_paragraph() {
        let xml = br#"<p:notes xmlns:p="p" xmlns:a="a"><p:cSld><p:spTree>
          <p:sp><p:nvSpPr><p:nvPr><p:ph type="dt"/></p:nvPr></p:nvSpPr>
            <p:txBody><a:p><a:r><a:t>2026-09-10</a:t></a:r></a:p></p:txBody></p:sp>
          <p:sp><p:nvSpPr><p:nvPr><p:ph type="body"/></p:nvPr></p:nvSpPr>
            <p:txBody><a:p><a:r><a:t>first </a:t></a:r><a:r><a:t>line</a:t></a:r></a:p>
            <a:p><a:r><a:t>second</a:t></a:r></a:p></p:txBody></p:sp>
          <p:sp><p:nvSpPr><p:nvPr><p:ph type="sldNum"/></p:nvPr></p:nvSpPr>
            <p:txBody><a:p><a:r><a:t>42</a:t></a:r></a:p></p:txBody></p:sp>
        </p:spTree></p:cSld></p:notes>"#;
        let (text, formatting, multiple) = parse_notes_body_text(xml).unwrap();
        assert_eq!(text.as_deref(), Some("first line\nsecond"));
        assert!(!formatting);
        assert!(!multiple);
    }

    #[test]
    fn unknown_transition_and_animation_are_structured_losses() {
        let transition_xml = br#"<p:sld xmlns:p="p"><p:transition p14:dur="500" xmlns:p14="p14"><p:zoom/></p:transition></p:sld>"#;
        let mut transition_report = PptxLossReport::default();
        assert!(
            parse_slide_transition(transition_xml, 0, &mut transition_report)
                .unwrap()
                .is_none()
        );
        assert!(transition_report
            .unsupported
            .iter()
            .any(|item| item.capability == "slideTransitionEffect"));

        let timeline_xml = br#"<p:sld xmlns:p="p"><p:timing><p:tnLst><p:par><p:cTn id="1" presetID="99" presetClass="entr" nodeType="clickEffect"/></p:par></p:tnLst></p:timing></p:sld>"#;
        let mut timeline_report = PptxLossReport::default();
        let timeline = parse_slide_timeline(timeline_xml, 0, &[], &mut timeline_report).unwrap();
        assert!(timeline.entries.is_empty());
        assert!(timeline_report
            .unsupported
            .iter()
            .any(|item| item.capability == "timeline"));
    }

    #[test]
    fn package_hazards_and_dangling_relationships_are_rejected_or_reported() {
        let macro_package = package([("ppt/vbaProject.bin".into(), b"untrusted macro".to_vec())]);
        let macro_report = inspect_pptx(&macro_package).unwrap();
        assert!(macro_report
            .unsupported
            .iter()
            .any(|item| item.capability == "macro"));

        let presentation = br#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="256" r:id="rIdMissing"/></p:sldIdLst><p:sldSz cx="12192000" cy="6858000"/></p:presentation>"#;
        let dangling = package([
            (PRESENTATION_PART.into(), presentation.to_vec()),
            (
                "ppt/_rels/presentation.xml.rels".into(),
                relationships_xml("").into_bytes(),
            ),
        ]);
        assert!(matches!(
            parse_pptx_with_report(&dangling),
            Err(PptxError::InvalidStructure(message)) if message.contains("rIdMissing")
        ));

        let rels = relationships_xml("<Relationship Id=\"rId1\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"slides/missing.xml\"/>");
        let missing_target_presentation = br#"<p:presentation xmlns:p="p" xmlns:r="r"><p:sldIdLst><p:sldId id="256" r:id="rId1"/></p:sldIdLst><p:sldSz cx="12192000" cy="6858000"/></p:presentation>"#;
        let missing_target = package([
            (
                PRESENTATION_PART.into(),
                missing_target_presentation.to_vec(),
            ),
            ("ppt/_rels/presentation.xml.rels".into(), rels.into_bytes()),
        ]);
        assert!(matches!(
            parse_pptx_with_report(&missing_target),
            Err(PptxError::InvalidStructure(message)) if message.contains("slides/missing.xml")
        ));
    }

    #[test]
    fn archive_entry_and_xml_part_limits_fail_closed() {
        let too_many_entries = package(
            (0..=MAX_ARCHIVE_ENTRIES).map(|index| (format!("custom/item-{index}"), Vec::new())),
        );
        assert!(matches!(
            inspect_pptx(&too_many_entries),
            Err(PptxError::InvalidStructure(message)) if message.contains("entry 数量")
        ));

        let oversized_xml =
            package([(PRESENTATION_PART.into(), vec![b' '; MAX_XML_PART_BYTES + 1])]);
        assert!(matches!(
            parse_pptx_with_report(&oversized_xml),
            Err(PptxError::InvalidStructure(message)) if message.contains("大小超限")
        ));
    }

    fn text_node(id: &str, text: &str) -> SceneNode {
        SceneNode {
            id: id.into(),
            parent_id: None,
            order_key: "00000000".into(),
            name: None,
            alt_text: None,
            layout_placeholder_id: None,
            transform: default_transform(0),
            visible: true,
            locked: false,
            opacity: 1.0,
            kind: SceneNodeKind::Text(TextNode {
                frame: TextFrame {
                    body: PresentationRichText::plain(text),
                    vertical_align: TextVerticalAlign::Top,
                    padding: Insets::default(),
                    auto_fit: TextAutoFit::ShrinkText,
                },
            }),
        }
    }

    #[test]
    fn minimal_roundtrip_uses_deck_not_generic_scene_elements() {
        let deck = Deck {
            theme: DeckTheme {
                id: "theme".into(),
                ..Default::default()
            },
            slides: vec![Slide {
                id: "s".into(),
                order_key: "a".into(),
                name: "季度 & 计划".into(),
                layout_id: None,
                background: Default::default(),
                notes: None,
                transition: None,
                timeline: Default::default(),
                nodes: vec![text_node("text-1", "hello")],
            }],
            ..Default::default()
        };
        let bytes = write_pptx(&deck).unwrap();
        let imported = parse_pptx(&bytes).unwrap();
        assert_eq!(imported.slides.len(), 1);
        assert!(semantic_diff(&deck, &imported).is_equivalent());
        assert!(matches!(
            imported.slides[0].nodes[0].kind,
            SceneNodeKind::Text(_)
        ));
    }

    #[test]
    fn paragraph_alignment_lists_and_unicode_ranges_roundtrip() {
        let mut node = text_node("text-1", "第一项\n🚀第二项");
        let SceneNodeKind::Text(text) = &mut node.kind else {
            unreachable!()
        };
        text.frame.body.paragraphs[0].alignment = TextHorizontalAlign::Center;
        text.frame.body.paragraphs[0].list = Some(PresentationListStyle::Bullet);
        text.frame.body.paragraphs[0].indent_level = 1;
        text.frame.body.paragraphs[1].alignment = TextHorizontalAlign::Right;
        text.frame.body.paragraphs[1].list = Some(PresentationListStyle::Ordered { start_at: 3 });
        let deck = Deck {
            slides: vec![Slide {
                id: "slide".into(),
                order_key: "00000000".into(),
                name: String::new(),
                layout_id: None,
                background: SlideBackground::None,
                notes: None,
                transition: None,
                nodes: vec![node],
                timeline: Default::default(),
            }],
            ..Deck::default()
        };
        deck.validate().unwrap();
        let exported = write_pptx_with_report(&deck).unwrap();
        assert!(exported.loss_report.unsupported.is_empty());
        let imported = parse_pptx_with_report(&exported.bytes).unwrap();
        assert!(
            imported.loss_report.unsupported.is_empty(),
            "report={:?}, entries={:?}",
            imported.loss_report,
            imported.deck.slides[0].timeline.entries
        );
        assert!(semantic_diff(&deck, &imported.deck).is_equivalent());
    }

    #[test]
    fn strict_table_and_free_connector_roundtrip_without_semantic_loss() {
        let table = SceneNode {
            id: "table-1".into(),
            parent_id: None,
            order_key: "00000000".into(),
            name: Some("计划表".into()),
            alt_text: None,
            layout_placeholder_id: None,
            transform: NodeTransform {
                x: 1_000.0,
                y: 2_000.0,
                width: 4_000.0,
                height: 2_000.0,
                rotation: 0.0,
            },
            visible: true,
            locked: false,
            opacity: 1.0,
            kind: SceneNodeKind::Table(TableNode {
                rows: 2,
                columns: 2,
                cells: vec![
                    TableCell {
                        row: 0,
                        column: 0,
                        row_span: 1,
                        column_span: 1,
                        content: PresentationRichText::plain("任务"),
                        style: TableCellStyle {
                            fill: Paint::Solid(ColorRef::Rgba(Rgba {
                                r: 1,
                                g: 2,
                                b: 3,
                                a: u8::MAX,
                            })),
                            horizontal_align: HorizontalAlign::Center,
                            vertical_align: TextVerticalAlign::Middle,
                        },
                    },
                    TableCell {
                        row: 0,
                        column: 1,
                        row_span: 1,
                        column_span: 1,
                        content: PresentationRichText::plain("负责人"),
                        style: TableCellStyle {
                            fill: Paint::None,
                            horizontal_align: HorizontalAlign::Left,
                            vertical_align: TextVerticalAlign::Top,
                        },
                    },
                    TableCell {
                        row: 1,
                        column: 0,
                        row_span: 1,
                        column_span: 1,
                        content: PresentationRichText::plain("完成"),
                        style: TableCellStyle {
                            fill: Paint::None,
                            horizontal_align: HorizontalAlign::Right,
                            vertical_align: TextVerticalAlign::Bottom,
                        },
                    },
                    TableCell {
                        row: 1,
                        column: 1,
                        row_span: 1,
                        column_span: 1,
                        content: PresentationRichText::plain(""),
                        style: TableCellStyle::default(),
                    },
                ],
            }),
        };
        let connector = SceneNode {
            id: "connector-1".into(),
            parent_id: None,
            order_key: "00000001".into(),
            name: Some("连接".into()),
            alt_text: None,
            layout_placeholder_id: None,
            transform: NodeTransform {
                x: 7_000.0,
                y: 8_000.0,
                width: 500.0,
                height: 800.0,
                rotation: 0.0,
            },
            visible: true,
            locked: false,
            opacity: 1.0,
            kind: SceneNodeKind::Connector(ConnectorNode {
                start: ConnectorEndpoint::Free(Point {
                    x: 7_000.0,
                    y: 8_000.0,
                }),
                end: ConnectorEndpoint::Free(Point {
                    x: 7_500.0,
                    y: 8_800.0,
                }),
            }),
        };
        let deck = Deck {
            slides: vec![Slide {
                id: "slide".into(),
                order_key: "00000000".into(),
                name: String::new(),
                layout_id: None,
                background: SlideBackground::None,
                notes: None,
                transition: None,
                nodes: vec![table, connector],
                timeline: Default::default(),
            }],
            ..Deck::default()
        };
        deck.validate().unwrap();
        let exported = write_pptx_with_report(&deck).unwrap();
        assert!(exported.loss_report.unsupported.is_empty());
        let imported = parse_pptx_with_report(&exported.bytes).unwrap();
        assert!(imported.loss_report.unsupported.is_empty());
        assert_eq!(
            semantic_diff(&deck, &imported.deck),
            PptxSemanticDiff::default()
        );
    }

    #[test]
    fn strict_table_connector_fixture_has_empty_loss_report_and_semantic_diff() {
        let deck: Deck = serde_json::from_str(include_str!(
            "../../../fixtures/presentation/pptx/strict-table-connector-deck.json"
        ))
        .unwrap();
        deck.validate().unwrap();
        let exported = write_pptx_with_report(&deck).unwrap();
        assert!(exported.loss_report.unsupported.is_empty());
        let imported = parse_pptx_with_report(&exported.bytes).unwrap();
        assert!(imported.loss_report.unsupported.is_empty());
        assert!(semantic_diff(&deck, &imported.deck).is_equivalent());
    }

    #[test]
    fn strict_writer_reports_merged_table_and_attached_connector() {
        let mut node = text_node("table", "");
        node.kind = SceneNodeKind::Table(TableNode {
            rows: 1,
            columns: 2,
            cells: vec![TableCell {
                row: 0,
                column: 0,
                row_span: 1,
                column_span: 2,
                content: PresentationRichText::plain("merged"),
                style: TableCellStyle::default(),
            }],
        });
        let connector = SceneNode {
            id: "connector".into(),
            parent_id: None,
            order_key: "00000001".into(),
            name: None,
            alt_text: None,
            layout_placeholder_id: None,
            transform: default_transform(1),
            visible: true,
            locked: false,
            opacity: 1.0,
            kind: SceneNodeKind::Connector(ConnectorNode {
                start: ConnectorEndpoint::Node {
                    node_id: "table".into(),
                    anchor: oo_schema::presentation_v5::Anchor::Center,
                },
                end: ConnectorEndpoint::Free(Point { x: 3.0, y: 4.0 }),
            }),
        };
        let deck = Deck {
            slides: vec![Slide {
                id: "slide".into(),
                order_key: "00000000".into(),
                name: String::new(),
                layout_id: None,
                background: SlideBackground::None,
                notes: None,
                transition: None,
                nodes: vec![node, connector],
                timeline: Default::default(),
            }],
            ..Deck::default()
        };
        deck.validate().unwrap();
        let report = write_pptx_with_report(&deck).unwrap().loss_report;
        assert!(report
            .unsupported
            .iter()
            .any(|item| item.capability == "presentationTable"));
        assert!(report
            .unsupported
            .iter()
            .any(|item| item.capability == "connector"));
        assert!(matches!(write_pptx(&deck), Err(PptxError::LossyExport(_))));
    }

    #[test]
    fn chart_spec_export_is_reported_and_strict_export_rejects_it() {
        let chart = SceneNode {
            id: "chart".into(),
            parent_id: None,
            order_key: "00000000".into(),
            name: Some("季度营收".into()),
            alt_text: None,
            layout_placeholder_id: None,
            transform: default_transform(0),
            visible: true,
            locked: false,
            opacity: 1.0,
            kind: SceneNodeKind::Chart(ChartNode {
                spec: ChartSpec {
                    chart_type: ChartType::Column,
                    title: Some("季度营收".into()),
                    categories: vec!["Q1".into(), "Q2".into()],
                    series: vec![ChartSeries {
                        name: "营收".into(),
                        values: vec![10.0, 20.0],
                        color: None,
                    }],
                },
            }),
        };
        let deck = Deck {
            theme: DeckTheme {
                id: "theme".into(),
                ..Default::default()
            },
            slides: vec![Slide {
                id: "slide".into(),
                order_key: "00000000".into(),
                name: String::new(),
                layout_id: None,
                background: Default::default(),
                notes: None,
                transition: None,
                nodes: vec![chart],
                timeline: Default::default(),
            }],
            ..Default::default()
        };
        let exported = write_pptx_with_report(&deck).unwrap();
        assert!(exported
            .loss_report
            .unsupported
            .iter()
            .any(|item| item.capability == "chartSpec"));
        assert!(matches!(write_pptx(&deck), Err(PptxError::LossyExport(_))));
    }

    #[test]
    fn supported_fixture_roundtrips_with_an_empty_semantic_diff() {
        let deck: Deck = serde_json::from_str(include_str!(
            "../../../fixtures/presentation/pptx/basic-supported-deck.json"
        ))
        .unwrap();
        deck.validate().unwrap();

        let exported = write_pptx_with_report(&deck).unwrap();
        assert!(exported.loss_report.unsupported.is_empty());
        let imported = parse_pptx_with_report(&exported.bytes).unwrap();
        assert!(imported.loss_report.unsupported.is_empty());
        assert!(semantic_diff(&deck, &imported.deck).is_equivalent());
    }

    #[test]
    fn rich_text_runs_roundtrip_without_semantic_diff() {
        let mut node = text_node("text-1", "Bold plain blue");
        if let SceneNodeKind::Text(text) = &mut node.kind {
            text.frame.body.runs = vec![
                PresentationTextRun {
                    start: 0,
                    end: 4,
                    style: PresentationTextStyle {
                        bold: true,
                        font_family: Some("Aptos".into()),
                        font_size: Some(18.0),
                        ..PresentationTextStyle::default()
                    },
                },
                PresentationTextRun {
                    start: 4,
                    end: 10,
                    style: PresentationTextStyle::default(),
                },
                PresentationTextRun {
                    start: 10,
                    end: 15,
                    style: PresentationTextStyle {
                        italic: true,
                        underline: true,
                        strikethrough: true,
                        color: Some(ColorRef::Theme(ThemeColorToken::Accent1)),
                        ..PresentationTextStyle::default()
                    },
                },
            ];
        }
        let deck = Deck {
            slides: vec![Slide {
                id: "slide".into(),
                order_key: "00000000".into(),
                name: String::new(),
                layout_id: None,
                background: SlideBackground::None,
                notes: None,
                transition: None,
                nodes: vec![node],
                timeline: Default::default(),
            }],
            ..Deck::default()
        };
        deck.validate().unwrap();

        let exported = write_pptx_with_report(&deck).unwrap();
        assert!(exported.loss_report.unsupported.is_empty());
        let imported = parse_pptx_with_report(&exported.bytes).unwrap();
        assert!(imported.loss_report.unsupported.is_empty());
        assert!(semantic_diff(&deck, &imported.deck).is_equivalent());
    }

    #[test]
    fn rich_text_alpha_is_reported_and_strict_writer_rejects_it() {
        let mut node = text_node("text-1", "alpha");
        if let SceneNodeKind::Text(text) = &mut node.kind {
            text.frame.body.runs = vec![PresentationTextRun {
                start: 0,
                end: 5,
                style: PresentationTextStyle {
                    color: Some(ColorRef::Rgba(Rgba {
                        r: 10,
                        g: 20,
                        b: 30,
                        a: 127,
                    })),
                    ..PresentationTextStyle::default()
                },
            }];
        }
        let deck = Deck {
            slides: vec![Slide {
                id: "slide".into(),
                order_key: "00000000".into(),
                name: String::new(),
                layout_id: None,
                background: SlideBackground::None,
                notes: None,
                transition: None,
                nodes: vec![node],
                timeline: Default::default(),
            }],
            ..Deck::default()
        };
        let report = write_pptx_with_report(&deck).unwrap().loss_report;
        assert!(report
            .unsupported
            .iter()
            .any(|item| item.capability == "textRunAlpha"));
        assert!(matches!(write_pptx(&deck), Err(PptxError::LossyExport(_))));
    }

    #[test]
    fn semantic_diff_uses_semantics_not_transport_node_ids() {
        let expected = Deck {
            slides: vec![Slide {
                id: "expected-slide".into(),
                order_key: "00000000".into(),
                name: String::new(),
                layout_id: None,
                background: Default::default(),
                notes: None,
                transition: None,
                nodes: vec![text_node("expected-node", "hello")],
                timeline: Default::default(),
            }],
            ..Deck::default()
        };
        let mut changed = expected.clone();
        if let SceneNodeKind::Text(text) = &mut changed.slides[0].nodes[0].kind {
            text.frame.body.text = "changed".into();
        }
        let diff = semantic_diff(&expected, &changed);
        assert_eq!(diff.differences.len(), 1);
        assert_eq!(diff.differences[0].path, "slides[0].nodes");
    }

    #[test]
    fn image_roundtrip_keeps_binary_outside_deck_and_restores_asset_reference() {
        let image_bytes = b"\x89PNG\r\n\x1a\nminimal".to_vec();
        let digest = hex::encode(Sha256::digest(&image_bytes));
        let asset = AssetRef {
            asset_id: "asset-image".into(),
            digest,
            mime_type: "image/png".into(),
            width: None,
            height: None,
            original_asset_id: None,
        };
        let mut image = text_node("image", "");
        image.kind = SceneNodeKind::Image(ImageNode {
            asset_id: asset.asset_id.clone(),
            original_asset_id: None,
            crop: Default::default(),
            flip_h: false,
            flip_v: false,
            caption: None,
        });
        let deck = Deck {
            theme: DeckTheme {
                id: "theme".into(),
                ..Default::default()
            },
            assets: vec![asset],
            slides: vec![Slide {
                id: "slide".into(),
                order_key: "00000000".into(),
                name: String::new(),
                layout_id: None,
                background: Default::default(),
                notes: None,
                transition: None,
                nodes: vec![image],
                timeline: Default::default(),
            }],
            ..Default::default()
        };
        let bytes = write_pptx_with_assets(
            &deck,
            &BTreeMap::from([("asset-image".into(), image_bytes)]),
        )
        .unwrap();
        assert!(bytes.loss_report.unsupported.is_empty());
        let imported = parse_pptx_with_report(&bytes.bytes).unwrap();
        assert_eq!(imported.assets.len(), 1);
        assert!(semantic_diff(&deck, &imported.deck).is_equivalent());
        assert!(matches!(
            imported.deck.slides[0].nodes[0].kind,
            SceneNodeKind::Image(_)
        ));
    }

    #[test]
    fn imported_theme_name_does_not_block_asset_backed_reexport() {
        let image_bytes = b"\x89PNG\r\n\x1a\nminimal".to_vec();
        let digest = hex::encode(Sha256::digest(&image_bytes));
        let asset = AssetRef {
            asset_id: "asset-image".into(),
            digest,
            mime_type: "image/png".into(),
            width: None,
            height: None,
            original_asset_id: None,
        };
        let mut image = text_node("image", "");
        image.kind = SceneNodeKind::Image(ImageNode {
            asset_id: asset.asset_id.clone(),
            original_asset_id: None,
            crop: Default::default(),
            flip_h: false,
            flip_v: false,
            caption: None,
        });
        let deck = Deck {
            theme: DeckTheme {
                id: "theme".into(),
                ..Default::default()
            },
            assets: vec![asset],
            slides: vec![Slide {
                id: "slide".into(),
                order_key: "00000000".into(),
                name: String::new(),
                layout_id: None,
                background: Default::default(),
                notes: None,
                transition: None,
                nodes: vec![image],
                timeline: Default::default(),
            }],
            ..Default::default()
        };
        let initial = write_pptx_with_assets(
            &deck,
            &BTreeMap::from([("asset-image".into(), image_bytes)]),
        )
        .unwrap();
        let imported = parse_pptx_with_report(&initial.bytes).unwrap();
        assert!(imported.deck.theme.name.is_empty());
        let assets = imported
            .assets
            .into_iter()
            .map(|asset| (asset.asset.asset_id, asset.bytes))
            .collect::<PptxAssetSource>();
        let reexported = write_pptx_with_assets(&imported.deck, &assets).unwrap();
        assert!(reexported.loss_report.unsupported.is_empty());
    }

    #[test]
    fn strict_writer_rejects_not_yet_mapped_master_instead_of_dropping_it() {
        let deck = Deck {
            theme: DeckTheme {
                id: "theme".into(),
                ..Default::default()
            },
            masters: vec![SlideMaster {
                id: "master".into(),
                name: "master".into(),
                background: Default::default(),
                placeholders: vec![],
            }],
            slides: vec![Slide {
                id: "slide".into(),
                order_key: "00000000".into(),
                name: String::new(),
                layout_id: None,
                background: Default::default(),
                notes: None,
                transition: None,
                nodes: vec![text_node("text", "hello")],
                timeline: Default::default(),
            }],
            ..Default::default()
        };
        let exported = write_pptx_with_report(&deck).unwrap();
        assert!(exported
            .loss_report
            .unsupported
            .iter()
            .any(|item| item.capability == "master"));
        assert!(matches!(write_pptx(&deck), Err(PptxError::LossyExport(_))));
    }

    #[test]
    fn notes_and_supported_transitions_have_strict_semantic_roundtrip() {
        let transitions = [
            TransitionKind::None,
            TransitionKind::Fade,
            TransitionKind::Push,
            TransitionKind::Wipe,
        ];
        let deck = Deck {
            theme: DeckTheme {
                id: "theme".into(),
                ..Default::default()
            },
            slides: transitions
                .into_iter()
                .enumerate()
                .map(|(index, kind)| Slide {
                    id: format!("slide-{index}"),
                    order_key: format!("{index:08}"),
                    name: String::new(),
                    layout_id: None,
                    background: Default::default(),
                    notes: Some(format!("  speaker note {index}\nsecond line  ")),
                    transition: Some(SlideTransition {
                        kind,
                        duration_ms: 250 + index as u32 * 125,
                    }),
                    nodes: vec![text_node(&format!("text-{index}"), "hello")],
                    timeline: Default::default(),
                })
                .collect(),
            ..Default::default()
        };

        let exported = write_pptx_with_report(&deck).unwrap();
        assert!(exported.loss_report.unsupported.is_empty());
        let imported = parse_pptx_with_report(&exported.bytes).unwrap();
        assert!(imported.loss_report.unsupported.is_empty());
        assert!(semantic_diff(&deck, &imported.deck).is_equivalent());

        let mut archive = open_archive(&exported.bytes).unwrap();
        assert!(archive.by_name("ppt/notesMasters/notesMaster1.xml").is_ok());
        for index in 1..=transitions.len() {
            assert!(archive
                .by_name(&format!("ppt/notesSlides/notesSlide{index}.xml"))
                .is_ok());
        }
    }

    #[test]
    fn text_frame_alignment_insets_and_autofit_roundtrip() {
        let variants = [
            (TextVerticalAlign::Top, TextAutoFit::None, 0.0),
            (TextVerticalAlign::Middle, TextAutoFit::ShrinkText, 48_000.0),
            (
                TextVerticalAlign::Bottom,
                TextAutoFit::ResizeShape,
                96_000.0,
            ),
        ];
        let nodes = variants
            .into_iter()
            .enumerate()
            .map(|(index, (vertical_align, auto_fit, inset))| {
                let mut node = text_node(&format!("frame-{index}"), "frame");
                node.order_key = format!("{index:08}");
                let SceneNodeKind::Text(text) = &mut node.kind else {
                    unreachable!()
                };
                text.frame.vertical_align = vertical_align;
                text.frame.auto_fit = auto_fit;
                text.frame.padding = Insets {
                    top: inset,
                    right: inset + 1.0,
                    bottom: inset + 2.0,
                    left: inset + 3.0,
                };
                node
            })
            .collect();
        let deck = Deck {
            slides: vec![Slide {
                id: "slide".into(),
                order_key: "00000000".into(),
                name: String::new(),
                layout_id: None,
                background: Default::default(),
                notes: None,
                transition: None,
                nodes,
                timeline: Default::default(),
            }],
            ..Deck::default()
        };

        let exported = write_pptx_with_report(&deck).unwrap();
        assert!(exported.loss_report.unsupported.is_empty());
        let imported = parse_pptx_with_report(&exported.bytes).unwrap();
        assert!(imported.loss_report.unsupported.is_empty());
        assert!(semantic_diff(&deck, &imported.deck).is_equivalent());
    }

    #[test]
    fn supported_entrance_timeline_has_strict_semantic_roundtrip() {
        let presets = [
            AnimationPreset::Appear,
            AnimationPreset::Fade,
            AnimationPreset::FlyIn,
            AnimationPreset::Wipe,
        ];
        let triggers = [
            AnimationTrigger::WithPrevious,
            AnimationTrigger::OnClick,
            AnimationTrigger::WithPrevious,
            AnimationTrigger::AfterPrevious,
        ];
        let nodes = (0..presets.len())
            .map(|index| {
                let mut node = text_node(&format!("animated-{index}"), &format!("node {index}"));
                node.order_key = format!("{index:08}");
                node
            })
            .collect::<Vec<_>>();
        let timeline = Timeline {
            entries: presets
                .into_iter()
                .zip(triggers)
                .enumerate()
                .map(|(index, (preset, trigger))| AnimationEntry {
                    id: format!("animation-{index}"),
                    target_node_id: nodes[index].id.clone(),
                    trigger,
                    preset,
                    duration_ms: 150 + index as u32 * 100,
                    delay_ms: 25 + index as u32 * 10,
                    order_key: format!("{index:08}"),
                })
                .collect(),
        };
        let deck = Deck {
            theme: DeckTheme {
                id: "theme".into(),
                ..Default::default()
            },
            slides: vec![Slide {
                id: "slide".into(),
                order_key: "00000000".into(),
                name: String::new(),
                layout_id: None,
                background: Default::default(),
                notes: None,
                transition: None,
                nodes,
                timeline,
            }],
            ..Default::default()
        };

        let exported = write_pptx_with_report(&deck).unwrap();
        assert!(exported.loss_report.unsupported.is_empty());
        let imported = parse_pptx_with_report(&exported.bytes).unwrap();
        assert!(
            imported.loss_report.unsupported.is_empty(),
            "report={:?}, entries={:?}",
            imported.loss_report,
            imported.deck.slides[0].timeline.entries
        );
        assert_eq!(imported.deck.slides[0].timeline.entries.len(), 4);
        assert!(semantic_diff(&deck, &imported.deck).is_equivalent());
    }

    #[test]
    fn target_v5_fixture_is_reported_and_rejected_not_silently_flattened() {
        let deck: Deck = serde_json::from_str(include_str!(
            "../../../fixtures/presentation/v5/minimal-deck.json"
        ))
        .unwrap();
        deck.validate().unwrap();

        let exported = write_pptx_with_report(&deck).unwrap();
        let capabilities = exported
            .loss_report
            .unsupported
            .iter()
            .map(|item| item.capability.as_str())
            .collect::<Vec<_>>();
        assert!(capabilities.contains(&"master"));
        assert!(capabilities.contains(&"layout"));
        assert!(capabilities.contains(&"theme"));
        assert!(capabilities.contains(&"slideLayoutReference"));
        assert!(capabilities.contains(&"layoutPlaceholderReference"));
        assert!(matches!(write_pptx(&deck), Err(PptxError::LossyExport(_))));
    }

    #[test]
    fn external_relationship_is_reported_and_never_becomes_canonical_data() {
        let mut cursor = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut cursor);
        zip.start_file(
            "ppt/_rels/presentation.xml.rels",
            SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"<Relationships><Relationship Id=\"rId1\" Type=\"x\" Target=\"https://example.invalid/a\" TargetMode=\"External\"/></Relationships>").unwrap();
        zip.finish().unwrap();
        let report = inspect_pptx(&cursor.into_inner()).unwrap();
        assert!(report
            .unsupported
            .iter()
            .any(|item| item.capability == "externalRelationship"));
    }

    #[test]
    fn unmapped_media_relationship_is_reported_before_scene_parse() {
        let relationships = HashMap::from([(
            "rIdMedia".into(),
            Relationship {
                kind: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/video"
                    .into(),
                target: "ppt/media/video1.mp4".into(),
            },
        )]);
        let mut report = PptxLossReport::default();
        report_unmapped_slide_relationships("ppt/slides/slide1.xml", &relationships, &mut report);
        assert_eq!(report.unsupported.len(), 1);
        assert_eq!(report.unsupported[0].capability, "media");
        assert_eq!(report.unsupported[0].part, "ppt/slides/slide1.xml");
    }

    #[test]
    fn mismatched_image_extension_is_rejected_before_asset_ingest() {
        let mut cursor = Cursor::new(Vec::new());
        let mut zip = zip::ZipWriter::new(&mut cursor);
        zip.start_file("ppt/media/image1.png", SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"GIF89a not actually png").unwrap();
        zip.finish().unwrap();

        let package = cursor.into_inner();
        let mut archive = open_archive(&package).unwrap();
        let relationship = Relationship {
            kind: "http://schemas.openxmlformats.org/officeDocument/2006/relationships/image"
                .into(),
            target: "ppt/media/image1.png".into(),
        };
        assert!(matches!(
            read_import_asset(&mut archive, &relationship),
            Err(PptxError::InvalidStructure(_))
        ));
    }
}
