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
    AssetRef, ColorRef, Deck, DeckTheme, GroupNode, ImageNode, Insets, NodeTransform, Paint,
    PresentationRichText, PresentationTextRun, PresentationTextStyle, Rgba, SceneNode,
    SceneNodeKind, ShapeGeometry, ShapeNode, ShapeStyle, Slide, SlideBackground, SlideLayout,
    SlideMaster, TextAutoFit, TextFrame, TextNode, TextVerticalAlign, ThemeColorToken,
    ThemeFontToken,
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
            &format!("{path}.background"),
            &expected_slide.background,
            &actual_slide.background,
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
        let xml = read_part(&mut archive, part, MAX_XML_PART_BYTES)?;
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
        (local_name(attribute.key.as_ref()) == name)
            .then(|| String::from_utf8_lossy(attribute.value.as_ref()).into_owned())
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
        let master_xml = read_part(archive, &master_part, MAX_XML_PART_BYTES)?;
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
                deck.theme = parse_theme(
                    &read_part(archive, &theme_rel.target, MAX_XML_PART_BYTES)?,
                    stable_part_id("theme", &theme_rel.target),
                )?;
                let theme_xml = read_part(archive, &theme_rel.target, MAX_XML_PART_BYTES)?;
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
            let layout_xml = read_part(archive, &layout_rel.target, MAX_XML_PART_BYTES)?;
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
    let xml = read_part(archive, part, MAX_XML_PART_BYTES)?;
    if contains_local_tag(&xml, b"rPr") {
        report.unsupported.push(report_item(
            PptxReportKind::Unsupported,
            "notesFormatting",
            part,
            "notes 的 run 格式尚未映射，当前只保留纯文本",
            Some("保留原始 PPTX source asset，或等待 notes rich-text adapter"),
        ));
    }
    let text = collect_text(&xml)?;
    Ok((!text.trim().is_empty()).then_some(text))
}

fn collect_text(xml: &[u8]) -> Result<String, PptxError> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut in_text = false;
    let mut values = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) if local_name(event.name().as_ref()) == b"t" => in_text = true,
            Event::End(event) if local_name(event.name().as_ref()) == b"t" => in_text = false,
            Event::Text(text) if in_text => values.push(text.unescape()?.into_owned()),
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(values.join("\n"))
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
    let contiguous = draft
        .runs
        .iter()
        .scan(0usize, |cursor, run| {
            let valid = run.start == *cursor && run.start < run.end && run.end <= length;
            *cursor = run.end;
            Some(valid)
        })
        .all(|valid| valid)
        && draft.runs.last().is_some_and(|run| run.end == length);
    if !contiguous {
        report_text_run_unsupported(
            report,
            slide_index,
            "a:r 与 text 内容未形成完整连续覆盖，不能安全构造 canonical text runs",
        );
        return Vec::new();
    }
    if draft
        .runs
        .iter()
        .all(|run| run.style == PresentationTextStyle::default())
    {
        // Canonical rich text uses an empty run list for unstyled text.  OOXML requires an
        // `a:r` wrapper nevertheless, so normalise that transport-only default on import.
        Vec::new()
    } else {
        std::mem::take(&mut draft.runs)
    }
}

fn parse_slide(
    xml: &[u8],
    slide_index: usize,
    relationships: &HashMap<String, Relationship>,
    assets: &HashMap<String, AssetRef>,
    report: &mut PptxLossReport,
) -> Result<Slide, PptxError> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut stack = Vec::<SlideNodeDraft>::new();
    let mut nodes = Vec::<SceneNode>::new();
    let mut sequence = 0usize;
    let mut in_text = false;
    let mut root_group_seen = false;
    let mut root_group_started = false;
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) => {
                let event_name = event.name().as_ref().to_vec();
                let local = local_name(&event_name);
                match local {
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
                    b"rPr" => {
                        if let Some(run) =
                            stack.last_mut().and_then(|draft| draft.active_run.as_mut())
                        {
                            parse_run_properties(&event, &mut run.style, report, slide_index);
                        }
                    }
                    b"latin" => {
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
                                    run.style.font_family = Some(typeface);
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
                if local == b"off" {
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
                } else if local == b"rPr" {
                    if let Some(run) = stack.last_mut().and_then(|draft| draft.active_run.as_mut())
                    {
                        parse_run_properties(&event, &mut run.style, report, slide_index);
                    }
                } else if local == b"latin" {
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
                                run.style.font_family = Some(typeface);
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
                        nodes.push(scene_node_from_draft(
                            draft,
                            SceneNodeKind::Text(TextNode {
                                frame: TextFrame {
                                    body: PresentationRichText { text, runs },
                                    vertical_align: TextVerticalAlign::Top,
                                    padding: Insets::default(),
                                    auto_fit: TextAutoFit::ShrinkText,
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
    Ok(Slide {
        id: format!("slide-{}", slide_index + 1),
        order_key: format!("{slide_index:08}"),
        name: String::new(),
        layout_id: None,
        background: SlideBackground::None,
        notes: None,
        transition: None,
        nodes,
        timeline: Default::default(),
    })
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
    let bytes = read_part(archive, &relationship.target, MAX_MEDIA_PART_BYTES)?;
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
        if !slide.name.is_empty() {
            report.unsupported.push(report_item(
                PptxReportKind::Unsupported,
                "slideName",
                &slide_path,
                "writer 尚未写出 cSld name",
                Some("清空 slide 名称后导出，或等待 slide metadata writer"),
            ));
        }
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
        if slide.notes.is_some() {
            report.unsupported.push(report_item(
                PptxReportKind::Unsupported,
                "notes",
                &slide_path,
                "writer 尚未生成 notesSlide/notesMaster package",
                Some("保留 slide.notes，等待 notes writer 完成"),
            ));
        }
        if slide.transition.is_some() {
            report.unsupported.push(report_item(
                PptxReportKind::Unsupported,
                "slideTransition",
                &slide_path,
                "writer 尚未写出 slide transition",
                Some("移除 transition 后导出，或等待 timeline writer"),
            ));
        }
        if !slide.timeline.entries.is_empty() {
            report.unsupported.push(report_item(
                PptxReportKind::Unsupported,
                "timeline",
                &slide_path,
                "writer 尚未写出动画时间线",
                Some("移除 timeline 后导出，或等待 timeline writer"),
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
            let frame = &text.frame;
            if frame.vertical_align != TextVerticalAlign::Top
                || frame.padding != Insets::default()
                || frame.auto_fit != TextAutoFit::ShrinkText
            {
                report.unsupported.push(report_item(
                    PptxReportKind::Unsupported,
                    "richTextFrame",
                    &node.id,
                    "writer 仅支持基础 run 样式、Top/ShrinkText、零 padding 的文本 frame",
                    Some("简化 text frame 属性后导出，或等待完整 text-frame writer"),
                ));
            }
            report_unsupported_text_runs(node, &frame.body, report);
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
        SceneNodeKind::Shape(_) | SceneNodeKind::Image(_) | SceneNodeKind::Group(_) => {}
        kind => report.unsupported.push(report_item(
            PptxReportKind::Unsupported,
            "sceneNode",
            &node.id,
            format!("{kind:?} 尚未定义 OOXML writer"),
            Some("保留在 Deck；不要静默导出"),
        )),
    }
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
        let mut xml_parts = vec![
            (PRESENTATION_PART.into(), presentation_xml(deck)),
            (
                "ppt/_rels/presentation.xml.rels".into(),
                presentation_rels(deck.slides.len()),
            ),
        ];
        let mut binary_parts = Vec::new();
        for (index, slide) in deck.slides.iter().enumerate() {
            let (xml, rels) = slide_xml(slide, assets, &mut binary_parts, report);
            xml_parts.push((format!("ppt/slides/slide{}.xml", index + 1), xml));
            xml_parts.push((
                format!("ppt/slides/_rels/slide{}.xml.rels", index + 1),
                rels,
            ));
        }
        Ok(Self {
            content_types: content_types(deck.slides.len()),
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
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:presentation xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:sldIdLst>{ids}</p:sldIdLst><p:sldSz cx=\"{}\" cy=\"{}\" type=\"screen16x9\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>", deck.page_spec.width.round(), deck.page_spec.height.round())
}

fn presentation_rels(count: usize) -> String {
    let rels = (0..count).map(|index| format!("<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/slide\" Target=\"slides/slide{}.xml\"/>", index + 1, index + 1)).collect::<String>();
    relationships_xml(&rels)
}

fn relationships_xml(rels: &str) -> String {
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Relationships xmlns=\"http://schemas.openxmlformats.org/package/2006/relationships\">{rels}</Relationships>")
}

fn slide_xml(
    slide: &Slide,
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
    let xml = format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><p:sld xmlns:a=\"http://schemas.openxmlformats.org/drawingml/2006/main\" xmlns:r=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"><p:cSld><p:spTree><p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr/>{nodes}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>");
    (xml, relationships_xml(&relationships))
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
        SceneNodeKind::Text(text) => output.push_str(&text_xml(node, &text.frame.body)),
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
        // Unsupported variants have already been recorded by
        // `report_unsupported_node_export_fields` before package construction.  Do not add a
        // second, less precise report here and do not emit a placeholder XML node.
        _ => {}
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

fn text_xml(node: &SceneNode, body: &PresentationRichText) -> String {
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
    let content = runs
        .iter()
        .map(|run| {
            text_run_xml(
                &chars[run.start..run.end].iter().collect::<String>(),
                &run.style,
            )
        })
        .collect::<String>();
    format!("<p:sp><p:nvSpPr><p:cNvPr id=\"{}\" name=\"{}\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr><p:spPr>{}<a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr><p:txBody><a:bodyPr/><a:lstStyle/><a:p>{content}</a:p></p:txBody></p:sp>", numeric_id(&node.id), xml_escaped(&node.name.clone().unwrap_or_default()), xfrm_xml(node))
}

fn text_run_xml(text: &str, style: &PresentationTextStyle) -> String {
    let properties = text_run_properties_xml(style);
    format!("<a:r>{properties}<a:t>{}</a:t></a:r>", xml_escaped(text))
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
        .map(|family| format!("<a:latin typeface=\"{}\"/>", xml_escaped(family)))
        .unwrap_or_default();
    let color = style.color.as_ref().map(text_color_xml).unwrap_or_default();
    format!("<a:rPr{attributes}>{latin}{color}</a:rPr>")
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
fn content_types(slide_count: usize) -> String {
    let slides = (0..slide_count).map(|index| format!("<Override PartName=\"/ppt/slides/slide{}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>", index + 1)).collect::<String>();
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?><Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Default Extension=\"jpg\" ContentType=\"image/jpeg\"/><Default Extension=\"jpeg\" ContentType=\"image/jpeg\"/><Default Extension=\"gif\" ContentType=\"image/gif\"/><Default Extension=\"webp\" ContentType=\"image/webp\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/>{slides}</Types>")
}

#[cfg(test)]
mod tests {
    use super::*;

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
                    body: PresentationRichText {
                        text: text.into(),
                        runs: vec![],
                    },
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
                name: String::new(),
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
    fn strict_writer_rejects_not_yet_mapped_master_and_notes_instead_of_dropping_them() {
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
                notes: Some("speaker note".into()),
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
        assert!(exported
            .loss_report
            .unsupported
            .iter()
            .any(|item| item.capability == "notes"));
        assert!(matches!(write_pptx(&deck), Err(PptxError::LossyExport(_))));
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
        assert!(capabilities.contains(&"slideName"));
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
