//! 把 .docx（ECMA-376 WordprocessingML）解析成 canonical `oo_schema::DocumentModel`。
//!
//! .docx 是一个 zip 包，当前 importer 读取其中两个部件：
//! - `word/document.xml`：正文内容与页面设置
//! - `word/styles.xml`：文档默认值与具名段落样式
//!
//! 尚未支持的页眉页脚、脚注和尾注部件会进入结构化 loss report；它们不会被静默丢弃。
//! 表格结构当前会被拍平成段落，列表编号和批注仍属于后续交换能力。
//! 图片会从 `document.xml.rels` 提取为稳定 assetId；写出端通过显式资产参数恢复
//! `word/media` 与关系部件，避免把二进制内容塞回 Document schema。

mod document;
mod props;
mod styles;
mod xml;

pub use props::{ParaProps, TextProps};
pub use styles::{parse_styles, StyleSheet};

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::io::{Cursor, Read};

use oo_schema::{
    BlockData, DocumentBlock, DocumentBlockKind, DocumentModel, InlineStyle, RichText,
};
use quick_xml::events::Event;
use quick_xml::Reader;
use serde::Serialize;
use sha2::{Digest, Sha256};
use zip::write::SimpleFileOptions;

const DOCUMENT_PART: &str = "word/document.xml";
const STYLES_PART: &str = "word/styles.xml";
const DOCUMENT_RELATIONSHIPS_PART: &str = "word/_rels/document.xml.rels";

#[derive(Debug, thiserror::Error)]
pub enum DocxError {
    #[error("不是有效的 zip 包：{0}")]
    Zip(#[from] zip::result::ZipError),

    #[error("XML 解析失败：{0}")]
    Xml(#[from] quick_xml::Error),

    #[error("读取部件失败：{0}")]
    Io(#[from] std::io::Error),

    #[error("缺少必需的部件 {0}，这可能不是一个 .docx 文件")]
    MissingPart(&'static str),

    #[error("部件 {0} 不是合法的 UTF-8")]
    NotUtf8(&'static str),

    #[error("XML 在 {0} 内提前结束")]
    UnexpectedEof(&'static str),

    #[error("DOCX 关系 {0} 指向了不受支持的媒体部件")]
    InvalidRelationship(String),

    #[error("DOCX 关系 {0} 引用的媒体不存在")]
    MissingMedia(String),

    #[error("Document schema 校验失败：{0}")]
    Schema(#[from] oo_schema::SchemaValidationError),

    #[error("DOCX 导出暂不支持 {0} block；请先使用原始 Artifact 保存")]
    UnsupportedBlock(&'static str),

    #[error("DOCX 导出缺少图片资产 {0}")]
    MissingAsset(String),

    #[error("DOCX 导出不支持媒体类型 {0}")]
    UnsupportedMedia(String),
}

/// DOCX 导入时从 ZIP 中提取的二进制资产。
///
/// `asset_id` 使用媒体内容的 SHA-256 派生，因此同一文档重复引用同一媒体时不会
/// 生成重复对象；服务端可以直接把它映射为 Artifact Asset 的稳定主键。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocxAsset {
    pub asset_id: String,
    pub content_type: String,
    pub file_name: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocxImport {
    pub document: DocumentModel,
    pub assets: Vec<DocxAsset>,
    pub loss_report: DocxLossReport,
}

/// 解析一份 .docx 的字节内容，直接产出 canonical `DocumentModel`。
///
/// `doc_id` 会进入 Artifact 外层，由调用方决定其取值。
pub fn parse_docx(bytes: &[u8], doc_id: impl Into<String>) -> Result<DocumentModel, DocxError> {
    Ok(parse_docx_with_assets(bytes, doc_id)?.document)
}

/// 解析 .docx，同时提取 `document.xml.rels` 声明的图片媒体。
pub fn parse_docx_with_assets(
    bytes: &[u8],
    doc_id: impl Into<String>,
) -> Result<DocxImport, DocxError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    let part_names = archive
        .file_names()
        .map(str::to_owned)
        .collect::<HashSet<_>>();
    let document_xml =
        read_part(&mut archive, DOCUMENT_PART)?.ok_or(DocxError::MissingPart(DOCUMENT_PART))?;
    let sheet = match read_part(&mut archive, STYLES_PART)? {
        Some(xml) => parse_styles(&xml)?,
        None => StyleSheet::default(),
    };
    let relationship_xml = read_part_bytes(&mut archive, DOCUMENT_RELATIONSHIPS_PART)?;
    let relationships = relationship_xml
        .as_deref()
        .map(parse_media_relationships)
        .transpose()?;
    let mut media_relations = HashMap::new();
    let mut assets = Vec::new();
    let mut known_asset_ids = HashSet::new();
    for (rel_id, target) in relationships.unwrap_or_default() {
        let media_path = resolve_media_target(&target)?;
        let media = read_part_bytes(&mut archive, &media_path)?
            .ok_or_else(|| DocxError::MissingMedia(media_path.clone()))?;
        let digest = Sha256::digest(&media);
        let asset_id = format!("docx-{}", hex::encode(digest));
        media_relations.insert(rel_id, asset_id.clone());
        if known_asset_ids.insert(asset_id.clone()) {
            assets.push(DocxAsset {
                asset_id,
                content_type: content_type_for_path(&media_path).into(),
                file_name: media_path
                    .rsplit('/')
                    .next()
                    .filter(|name| !name.is_empty())
                    .unwrap_or("asset.bin")
                    .into(),
                bytes: media,
            });
        }
    }
    let (model, losses) =
        document::parse_document(&document_xml, &sheet, doc_id, &media_relations)?;
    model.validate()?;
    let loss_report = collect_docx_import_losses(&part_names, &losses);
    Ok(DocxImport {
        document: model,
        assets,
        loss_report,
    })
}

fn collect_docx_import_losses(
    part_names: &HashSet<String>,
    body: &document::ImportLossCounters,
) -> DocxLossReport {
    let mut unsupported = Vec::new();
    // 正文解析阶段就已经降级的结构：文字还在，结构语义没了。这些以前完全不上报，
    // 导入方只能靠肉眼发现「表格变成了一堆段落」。
    if body.flattened_tables > 0 {
        unsupported.push(DocxLoss {
            capability: "tableStructure",
            count: body.flattened_tables,
            detail: "DOCX 表格尚未建模，单元格文字已按段落导入，表格结构丢失".into(),
        });
    }
    if body.dropped_list_numbering > 0 {
        unsupported.push(DocxLoss {
            capability: "listNumbering",
            count: body.dropped_list_numbering,
            detail:
                "DOCX 编号定义（numbering.xml）尚未解析，列表段落只保留缩进层级，项目符号与编号丢失"
                    .into(),
        });
    }
    if body.dropped_hyperlink_targets > 0 {
        unsupported.push(DocxLoss {
            capability: "linkTarget",
            count: body.dropped_hyperlink_targets,
            detail: "DOCX 超链接文字已导入，链接目标（外部关系或书签锚点）未捕获".into(),
        });
    }
    let header_footer_count = part_names
        .iter()
        .filter(|name| name.starts_with("word/header") || name.starts_with("word/footer"))
        .count();
    if header_footer_count > 0 {
        unsupported.push(DocxLoss {
            capability: "headerFooter",
            count: header_footer_count,
            detail: "DOCX 页眉/页脚部件尚未导入，正文与节页面设置已保留".into(),
        });
    }
    if part_names.contains("word/footnotes.xml") {
        unsupported.push(DocxLoss {
            capability: "footnotes",
            count: 1,
            detail: "DOCX 脚注部件尚未导入".into(),
        });
    }
    if part_names.contains("word/endnotes.xml") {
        unsupported.push(DocxLoss {
            capability: "endnotes",
            count: 1,
            detail: "DOCX 尾注部件尚未导入".into(),
        });
    }
    DocxLossReport { unsupported }
}

fn parse_media_relationships(xml: &[u8]) -> Result<Vec<(String, String)>, DocxError> {
    let mut reader = Reader::from_reader(xml);
    reader.config_mut().trim_text(true);
    let mut relationships = Vec::new();
    loop {
        match reader.read_event()? {
            Event::Start(e) | Event::Empty(e)
                if crate::xml::local_name(&e).as_slice() == b"Relationship" =>
            {
                let relation_type = crate::xml::attr(&e, "Type").unwrap_or_default();
                if relation_type.ends_with("/image") {
                    let id = crate::xml::attr(&e, "Id")
                        .ok_or_else(|| DocxError::InvalidRelationship("图片关系缺少 Id".into()))?;
                    let target = crate::xml::attr(&e, "Target").ok_or_else(|| {
                        DocxError::InvalidRelationship(format!("{id} 缺少 Target"))
                    })?;
                    relationships.push((id, target));
                }
            }
            Event::Eof => break,
            _ => {}
        }
    }
    Ok(relationships)
}

fn resolve_media_target(target: &str) -> Result<String, DocxError> {
    let mut parts = vec!["word".to_string()];
    let target = target.split('#').next().unwrap_or(target);
    for part in target.trim_start_matches('/').split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts
                    .pop()
                    .ok_or_else(|| DocxError::InvalidRelationship(target.to_string()))?;
            }
            segment if segment.contains('\\') => {
                return Err(DocxError::InvalidRelationship(target.to_string()))
            }
            segment => parts.push(segment.to_string()),
        }
    }
    let path = parts.join("/");
    if !path.starts_with("word/media/") {
        return Err(DocxError::InvalidRelationship(target.to_string()));
    }
    Ok(path)
}

fn content_type_for_path(path: &str) -> &'static str {
    match path
        .rsplit('.')
        .next()
        .map(str::to_ascii_lowercase)
        .as_deref()
    {
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("svg") => "image/svg+xml",
        Some("webp") => "image/webp",
        Some("bmp") => "image/bmp",
        Some("tif") | Some("tiff") => "image/tiff",
        _ => "application/octet-stream",
    }
}

/// 将不含媒体的 canonical DocumentModel 导出为可被 Word/WPS 打开的最小 DOCX 包。
/// 一类被导出器近似或丢弃的文档能力。文本内容总是保留；这里列出的是
/// 无法在 DOCX 中保真的语义（如 todo 勾选态、链接目标）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocxLoss {
    /// 稳定能力码，供 UI/SDK 直接消费：todoState | linkTarget | calloutStyle |
    /// structuralContainer | unknownKind，以及导入侧的 tableStructure | listNumbering |
    /// headerFooter | footnotes | endnotes。
    pub capability: &'static str,
    /// 受影响的 block 数量。
    pub count: usize,
    /// 面向用户的中文说明。
    pub detail: String,
}

/// DOCX 导出的能力损失报告。与 PPTX 的 `PptxLossReport` 同构：调用方可以
/// 把它展示给用户，而不是让导出器静默近似。表格、extension block 与缺失
/// 图片资产仍然是硬错误，不会进入报告。
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DocxLossReport {
    pub unsupported: Vec<DocxLoss>,
}

impl DocxLossReport {
    pub fn is_empty(&self) -> bool {
        self.unsupported.is_empty()
    }

    /// 适合放进 HTTP 响应头的紧凑 ASCII 摘要：`capability:count` 对。
    pub fn header_summary(&self) -> Option<String> {
        if self.unsupported.is_empty() {
            return None;
        }
        let summary = self
            .unsupported
            .iter()
            .map(|loss| format!("{}:{}", loss.capability, loss.count))
            .collect::<Vec<_>>()
            .join(",");
        Some(summary)
    }
}

fn collect_docx_losses(document: &DocumentModel) -> DocxLossReport {
    fn push(entries: &mut Vec<DocxLoss>, capability: &'static str, detail: &str) {
        if let Some(existing) = entries
            .iter_mut()
            .find(|entry| entry.capability == capability)
        {
            existing.count += 1;
        } else {
            entries.push(DocxLoss {
                capability,
                count: 1,
                detail: detail.to_string(),
            });
        }
    }
    let mut unsupported = Vec::new();
    for block in &document.blocks {
        match (&block.kind, &block.data) {
            (_, BlockData::Todo { .. }) => {
                push(
                    &mut unsupported,
                    "todoState",
                    "todo 的勾选状态无法写入 DOCX，仅保留文本",
                );
            }
            (DocumentBlockKind::Link, _) => push(
                &mut unsupported,
                "linkTarget",
                "link block 的目标 URL 无法写回 DOCX，仅保留可见文本",
            ),
            (DocumentBlockKind::Callout, _) => push(
                &mut unsupported,
                "calloutStyle",
                "callout 的容器样式被导出为普通段落",
            ),
            (
                DocumentBlockKind::Page | DocumentBlockKind::Columns | DocumentBlockKind::Column,
                _,
            ) => push(
                &mut unsupported,
                "structuralContainer",
                "分页/多栏容器结构在 DOCX 中被展平为顺序段落",
            ),
            (DocumentBlockKind::Unknown { type_id, .. }, _) => push(
                &mut unsupported,
                "unknownKind",
                &format!("未知块 {type_id} 仅保留原始文本"),
            ),
            _ => {}
        }
    }
    if document.page_semantics.sections.len() > 1 {
        unsupported.push(DocxLoss {
            capability: "sectionBreaks",
            count: document.page_semantics.sections.len() - 1,
            detail: "当前 DOCX writer 只写出最终节属性，其余节边界会被展平".into(),
        });
    }
    for section in &document.page_semantics.sections {
        if section.header.is_some() || section.footer.is_some() {
            push(
                &mut unsupported,
                "headerFooter",
                "页眉/页脚已保留在 Artifact，但当前 DOCX writer 尚不写出关联部件",
            );
        }
    }
    if !document.page_semantics.footnotes.is_empty() {
        unsupported.push(DocxLoss {
            capability: "footnotes",
            count: document.page_semantics.footnotes.len(),
            detail: "脚注已保留在 Artifact，但当前 DOCX writer 尚不写出 footnotes 部件".into(),
        });
    }
    if !document.page_semantics.endnotes.is_empty() {
        unsupported.push(DocxLoss {
            capability: "endnotes",
            count: document.page_semantics.endnotes.len(),
            detail: "尾注已保留在 Artifact，但当前 DOCX writer 尚不写出 endnotes 部件".into(),
        });
    }
    DocxLossReport { unsupported }
}

/// 导出 DOCX 并返回能力损失报告。字节与 [`write_docx`] 完全一致；表格、
/// extension 与资产错误仍按原样硬失败，不会出现在报告中。
pub fn write_docx_with_report(
    document: &DocumentModel,
    assets: &[DocxAsset],
) -> Result<DocxExport, DocxError> {
    let loss_report = collect_docx_losses(document);
    let bytes = write_docx_with_assets(document, assets)?;
    Ok(DocxExport { bytes, loss_report })
}

/// [`write_docx_with_report`] 的返回值。
#[derive(Debug)]
pub struct DocxExport {
    pub bytes: Vec<u8>,
    pub loss_report: DocxLossReport,
}

pub fn write_docx(document: &DocumentModel) -> Result<Vec<u8>, DocxError> {
    write_docx_internal(document, &[])
}

/// 导出包含图片 block 的 DOCX。
///
/// 图片字节必须由上层资产存储按 `asset_id` 显式提供；导出器不读取文件系统，也不
/// 把二进制对象写进 DocumentModel。缺少资产或媒体类型不可逆时会稳定失败。
pub fn write_docx_with_assets(
    document: &DocumentModel,
    assets: &[DocxAsset],
) -> Result<Vec<u8>, DocxError> {
    let mut asset_by_id = HashMap::new();
    for asset in assets {
        asset_by_id.insert(asset.asset_id.as_str(), asset);
    }
    let image_ids = image_asset_ids(document);
    let mut media = Vec::with_capacity(image_ids.len());
    for (index, asset_id) in image_ids.iter().enumerate() {
        let asset = asset_by_id
            .get(asset_id.as_str())
            .ok_or(DocxError::MissingAsset(asset_id.clone()))?;
        let extension = media_extension(&asset.content_type)
            .ok_or_else(|| DocxError::UnsupportedMedia(asset.content_type.clone()))?;
        media.push((
            asset_id.clone(),
            format!("rId{}", index + 3),
            format!("word/media/asset-{}.{}", index + 1, extension),
            *asset,
        ));
    }
    write_docx_internal(document, &media)
}

type ExportMedia<'a> = (String, String, String, &'a DocxAsset);

fn write_docx_internal(
    document: &DocumentModel,
    media: &[ExportMedia<'_>],
) -> Result<Vec<u8>, DocxError> {
    document.validate()?;
    if media.is_empty()
        && document
            .blocks
            .iter()
            .any(|block| matches!(&block.data, BlockData::Image(_)))
    {
        return Err(DocxError::UnsupportedBlock("image"));
    }
    if document
        .blocks
        .iter()
        .any(|block| matches!(&block.data, BlockData::Table(_)))
    {
        return Err(DocxError::UnsupportedBlock("table"));
    }
    if document
        .blocks
        .iter()
        .any(|block| matches!(&block.data, BlockData::Extension(_)))
    {
        return Err(DocxError::UnsupportedBlock("extension"));
    }
    let content_types = build_content_types_xml(media.iter().map(MediaExport::extension));
    let document_xml = build_document_xml(document, media);
    let relationships =
        build_document_relationships_xml(media.iter().map(MediaExport::relationship));
    let parts = [
        ("[Content_Types].xml", content_types),
        ("_rels/.rels", build_root_relationships_xml()),
        ("word/document.xml", document_xml),
        ("word/styles.xml", build_styles_xml()),
        ("word/numbering.xml", build_numbering_xml()),
        ("word/_rels/document.xml.rels", relationships),
    ];
    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in parts {
        archive.start_file(name, SimpleFileOptions::default())?;
        std::io::Write::write_all(&mut archive, content.as_bytes())?;
    }
    for item in media {
        archive.start_file(item.path(), SimpleFileOptions::default())?;
        std::io::Write::write_all(&mut archive, item.bytes())?;
    }
    Ok(archive.finish()?.into_inner())
}

trait MediaExport {
    fn asset_id(&self) -> &str;
    fn extension(&self) -> &str;
    fn relationship(&self) -> (&str, &str);
    fn path(&self) -> &str;
    fn bytes(&self) -> &[u8];
}

impl<'a> MediaExport for ExportMedia<'a> {
    fn asset_id(&self) -> &str {
        self.0.as_str()
    }

    fn extension(&self) -> &str {
        self.2.rsplit('.').next().unwrap_or("bin")
    }

    fn relationship(&self) -> (&str, &str) {
        (
            self.1.as_str(),
            self.2.strip_prefix("word/").unwrap_or(self.2.as_str()),
        )
    }

    fn path(&self) -> &str {
        &self.2
    }

    fn bytes(&self) -> &[u8] {
        &self.3.bytes
    }
}

fn build_document_xml<T: MediaExport>(document: &DocumentModel, media: &[T]) -> String {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships" xmlns:wp="http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing" xmlns:a="http://schemas.openxmlformats.org/drawingml/2006/main" xmlns:pic="http://schemas.openxmlformats.org/drawingml/2006/picture"><w:body>"#,
    );
    let block_map: std::collections::HashMap<&str, &DocumentBlock> = document
        .blocks
        .iter()
        .map(|block| (block.id.as_str(), block))
        .collect();
    for id in &document.root {
        append_block(&mut xml, &block_map, id, media);
    }
    let final_section = document.page_semantics.sections.last();
    append_section_properties(
        &mut xml,
        final_section
            .and_then(|section| section.page_setup.as_ref())
            .or(document.page_setup.as_ref()),
        final_section.and_then(|section| section.page_numbering.as_ref()),
    );
    xml.push_str("</w:body></w:document>");
    xml
}

fn append_block(
    xml: &mut String,
    block_map: &std::collections::HashMap<&str, &DocumentBlock>,
    id: &str,
    media: &[impl MediaExport],
) {
    let Some(block) = block_map.get(id) else {
        return;
    };
    if matches!(block.kind, DocumentBlockKind::Divider) {
        xml.push_str("<w:p><w:pPr><w:pBdr><w:bottom w:val=\"single\" w:sz=\"6\" w:space=\"1\" w:color=\"B7B7B7\"/></w:pBdr></w:pPr></w:p>");
    } else if let BlockData::Image(image) = &block.data {
        if let Some((index, relation)) = media
            .iter()
            .enumerate()
            .find(|(_, item)| item.asset_id() == image.asset_id)
        {
            append_image_run(xml, relation.relationship().0, index as u32 + 1);
        }
    } else {
        xml.push_str("<w:p>");
        append_paragraph_properties(xml, block);
        if let Some(content) = &block.content {
            append_rich_text(xml, content);
        }
        xml.push_str("</w:p>");
    }
    for child in &block.children {
        append_block(xml, block_map, child, media);
    }
}

fn append_image_run(xml: &mut String, relation_id: &str, drawing_id: u32) {
    xml.push_str(
        "<w:p><w:r><w:drawing><wp:inline><wp:extent cx=\"4572000\" cy=\"2286000\"/><wp:docPr id=\"",
    );
    let _ = write!(xml, "{}", drawing_id);
    xml.push_str("\" name=\"Open Office image\"/><a:graphic><a:graphicData uri=\"http://schemas.openxmlformats.org/drawingml/2006/picture\"><pic:pic><pic:nvPicPr><pic:cNvPr id=\"");
    let _ = write!(xml, "{}", drawing_id);
    xml.push_str(
        "\" name=\"image\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"",
    );
    xml.push_str(&xml_escape(relation_id));
    xml.push_str("\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"4572000\" cy=\"2286000\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>");
}

fn image_asset_ids(document: &DocumentModel) -> Vec<String> {
    let blocks = document
        .blocks
        .iter()
        .map(|block| (block.id.as_str(), block))
        .collect::<HashMap<_, _>>();
    let mut result = Vec::new();
    fn visit(id: &str, blocks: &HashMap<&str, &DocumentBlock>, result: &mut Vec<String>) {
        let Some(block) = blocks.get(id) else {
            return;
        };
        if let BlockData::Image(image) = &block.data {
            result.push(image.asset_id.clone());
        }
        for child in &block.children {
            visit(child, blocks, result);
        }
    }
    for id in &document.root {
        visit(id, &blocks, &mut result);
    }
    result
}

fn append_paragraph_properties(xml: &mut String, block: &DocumentBlock) {
    let mut properties = String::new();
    match block.kind {
        DocumentBlockKind::Heading { level } => {
            let _ = write!(
                properties,
                "<w:pStyle w:val=\"Heading{}\"/>",
                level.clamp(1, 6)
            );
        }
        DocumentBlockKind::Quote => properties.push_str("<w:pStyle w:val=\"Quote\"/>"),
        DocumentBlockKind::Code => properties.push_str("<w:pStyle w:val=\"Code\"/>"),
        _ => {}
    }
    if matches!(block.kind, DocumentBlockKind::Paragraph) {
        if let Some(named_style) = &block.presentation.named_style {
            let name = xml_escape(&named_style.name);
            let _ = write!(properties, "<w:pStyle w:val=\"{name}\"/>");
        }
    }
    let presentation = &block.presentation;
    let alignment = match presentation.align {
        oo_schema::BlockAlignment::Center => "center",
        oo_schema::BlockAlignment::Right => "right",
        oo_schema::BlockAlignment::Justify => "both",
        oo_schema::BlockAlignment::Left => "left",
    };
    let _ = write!(properties, "<w:jc w:val=\"{alignment}\"/>");

    let spacing = [
        ("before", presentation.spacing_before),
        ("after", presentation.spacing_after),
        ("line", presentation.line_height * 240.0),
    ];
    properties.push_str("<w:spacing");
    for (name, value) in spacing {
        let value = if name == "line" {
            value.round() as i64
        } else {
            twips(value as f64)
        };
        let _ = write!(properties, " w:{name}=\"{value}\"");
    }
    properties.push_str(" w:lineRule=\"auto\"/>");

    if presentation.indent_start > 0 || presentation.indent_end > 0.0 {
        properties.push_str("<w:ind");
        if presentation.indent_start > 0 {
            let _ = write!(
                properties,
                " w:left=\"{}\"",
                twips(presentation.indent_start as f64)
            );
        }
        if presentation.indent_end > 0.0 {
            let _ = write!(
                properties,
                " w:right=\"{}\"",
                twips(presentation.indent_end as f64)
            );
        }
        properties.push_str("/>");
    }
    if let Some(list) = &presentation.list {
        let level = u64::from(list.level).min(8);
        let num_id = if matches!(list.kind, oo_schema::ListKind::Ordered) {
            2
        } else {
            1
        };
        let _ = write!(
            properties,
            "<w:numPr><w:ilvl w:val=\"{level}\"/><w:numId w:val=\"{num_id}\"/></w:numPr>"
        );
    }
    if !properties.is_empty() {
        xml.push_str("<w:pPr>");
        xml.push_str(&properties);
        xml.push_str("</w:pPr>");
    }
}

fn append_rich_text(xml: &mut String, content: &RichText) {
    let chars: Vec<char> = content.text.chars().collect();
    if content.runs.is_empty() {
        append_run(xml, &chars, &InlineStyle::default());
        return;
    }
    for run in &content.runs {
        append_run(xml, &chars[run.start..run.end], &run.style);
    }
}

fn append_run(xml: &mut String, chars: &[char], style: &InlineStyle) {
    if chars.is_empty() {
        return;
    }
    xml.push_str("<w:r>");
    append_run_properties(xml, style);
    let mut text = String::new();
    for &character in chars {
        match character {
            '\n' => {
                append_text_node(xml, &text);
                text.clear();
                xml.push_str("<w:br/>");
            }
            '\t' => {
                append_text_node(xml, &text);
                text.clear();
                xml.push_str("<w:tab/>");
            }
            character if is_xml_char(character) => text.push(character),
            _ => {}
        }
    }
    append_text_node(xml, &text);
    xml.push_str("</w:r>");
}

fn append_run_properties(xml: &mut String, style: &InlineStyle) {
    let mut properties = String::new();
    if style.bold {
        properties.push_str("<w:b/>");
    }
    if style.italic {
        properties.push_str("<w:i/>");
    }
    if style.underline {
        properties.push_str("<w:u w:val=\"single\"/>");
    }
    if style.strikethrough {
        properties.push_str("<w:strike/>");
    }
    if let Some(font) = style.font_family.as_deref() {
        let font = xml_escape(&oo_schema::font_family::primary_font_family(font));
        let _ = write!(
            properties,
            "<w:rFonts w:ascii=\"{font}\" w:hAnsi=\"{font}\" w:eastAsia=\"{font}\" w:cs=\"{font}\"/>"
        );
    }
    if let Some(size) = style.font_size {
        let _ = write!(properties, "<w:sz w:val=\"{}\"/>", half_points(size as f64));
    }
    if let Some(color) = style.color.as_deref() {
        let color = color.trim_start_matches('#');
        if color.len() == 6 && color.chars().all(|character| character.is_ascii_hexdigit()) {
            let _ = write!(properties, "<w:color w:val=\"{color}\"/>");
        }
    }
    if let Some(highlight) = style.highlight.as_deref() {
        let value = match highlight
            .trim_start_matches('#')
            .to_ascii_lowercase()
            .as_str()
        {
            "ffff00" | "yellow" => Some("yellow"),
            "00ff00" | "green" => Some("green"),
            "00ffff" | "cyan" => Some("cyan"),
            "ff00ff" | "magenta" => Some("magenta"),
            "ff0000" | "red" => Some("red"),
            "0000ff" | "blue" => Some("blue"),
            "ffffff" | "white" => Some("white"),
            "000000" | "black" => Some("black"),
            _ => None,
        };
        if let Some(value) = value {
            let _ = write!(properties, "<w:highlight w:val=\"{value}\"/>");
        }
    }
    match style.vertical_align {
        Some(oo_schema::VerticalAlign::Superscript) => {
            properties.push_str("<w:vertAlign w:val=\"superscript\"/>")
        }
        Some(oo_schema::VerticalAlign::Subscript) => {
            properties.push_str("<w:vertAlign w:val=\"subscript\"/>")
        }
        _ => {}
    }
    if !properties.is_empty() {
        xml.push_str("<w:rPr>");
        xml.push_str(&properties);
        xml.push_str("</w:rPr>");
    }
}

fn append_text_node(xml: &mut String, text: &str) {
    if text.is_empty() {
        return;
    }
    let escaped = xml_escape(text);
    if text.starts_with(' ') || text.ends_with(' ') {
        let _ = write!(xml, "<w:t xml:space=\"preserve\">{escaped}</w:t>");
    } else {
        let _ = write!(xml, "<w:t>{escaped}</w:t>");
    }
}

fn append_section_properties(
    xml: &mut String,
    page_setup: Option<&oo_schema::PageSetup>,
    page_numbering: Option<&oo_schema::DocumentPageNumbering>,
) {
    let page = page_setup.cloned().unwrap_or(oo_schema::PageSetup {
        width: 612.0,
        height: 792.0,
        margin_top: 72.0,
        margin_right: 72.0,
        margin_bottom: 72.0,
        margin_left: 72.0,
    });
    let _ = write!(
        xml,
        "<w:sectPr><w:pgSz w:w=\"{}\" w:h=\"{}\"/><w:pgMar w:top=\"{}\" w:right=\"{}\" w:bottom=\"{}\" w:left=\"{}\"/>",
        twips(page.width as f64),
        twips(page.height as f64),
        twips(page.margin_top as f64),
        twips(page.margin_right as f64),
        twips(page.margin_bottom as f64),
        twips(page.margin_left as f64)
    );
    if let Some(numbering) = page_numbering {
        let format = match numbering.format {
            oo_schema::PageNumberFormat::Decimal => "decimal",
            oo_schema::PageNumberFormat::UpperRoman => "upperRoman",
            oo_schema::PageNumberFormat::LowerRoman => "lowerRoman",
            oo_schema::PageNumberFormat::UpperLetter => "upperLetter",
            oo_schema::PageNumberFormat::LowerLetter => "lowerLetter",
        };
        let _ = write!(
            xml,
            "<w:pgNumType w:start=\"{}\" w:fmt=\"{}\"/>",
            numbering.start_at, format
        );
    }
    xml.push_str("</w:sectPr>");
}

fn twips(points: f64) -> i64 {
    (points * 20.0).round() as i64
}

fn half_points(points: f64) -> i64 {
    (points * 2.0).round() as i64
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn is_xml_char(character: char) -> bool {
    matches!(
        character,
        '\u{9}'
            | '\u{A}'
            | '\u{D}'
            | '\u{20}'..='\u{D7FF}'
            | '\u{E000}'..='\u{FFFD}'
            | '\u{10000}'..='\u{10FFFF}'
    )
}

fn build_styles_xml() -> String {
    String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
<w:docDefaults><w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri" w:hAnsi="Calibri" w:eastAsia="等线"/><w:sz w:val="22"/></w:rPr></w:rPrDefault></w:docDefaults>
<w:style w:type="paragraph" w:default="1" w:styleId="Normal"><w:name w:val="Normal"/></w:style>
<w:style w:type="paragraph" w:styleId="Quote"><w:name w:val="Quote"/><w:pPr><w:ind w:left="360"/></w:pPr></w:style>
<w:style w:type="paragraph" w:styleId="Code"><w:name w:val="Code"/><w:rPr><w:rFonts w:ascii="Consolas" w:hAnsi="Consolas"/></w:rPr></w:style>
<w:style w:type="paragraph" w:styleId="Heading1"><w:name w:val="heading 1"/><w:basedOn w:val="Normal"/><w:qFormat/></w:style>
<w:style w:type="paragraph" w:styleId="Heading2"><w:name w:val="heading 2"/><w:basedOn w:val="Normal"/><w:qFormat/></w:style>
<w:style w:type="paragraph" w:styleId="Heading3"><w:name w:val="heading 3"/><w:basedOn w:val="Normal"/><w:qFormat/></w:style>
<w:style w:type="paragraph" w:styleId="Heading4"><w:name w:val="heading 4"/><w:basedOn w:val="Normal"/><w:qFormat/></w:style>
<w:style w:type="paragraph" w:styleId="Heading5"><w:name w:val="heading 5"/><w:basedOn w:val="Normal"/><w:qFormat/></w:style>
<w:style w:type="paragraph" w:styleId="Heading6"><w:name w:val="heading 6"/><w:basedOn w:val="Normal"/><w:qFormat/></w:style>
</w:styles>"#,
    )
}

fn build_numbering_xml() -> String {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:numbering xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">"#,
    );
    for (abstract_id, format, text) in [(0, "bullet", "•"), (1, "decimal", "%1.")] {
        let _ = write!(
            xml,
            "<w:abstractNum w:abstractNumId=\"{abstract_id}\"><w:multiLevelType w:val=\"hybridMultilevel\"/>"
        );
        for level in 0..9 {
            let indent = 360 * (level + 1);
            let _ = write!(
                xml,
                "<w:lvl w:ilvl=\"{level}\"><w:start w:val=\"1\"/><w:numFmt w:val=\"{format}\"/><w:lvlText w:val=\"{text}\"/><w:lvlJc w:val=\"left\"/><w:pPr><w:ind w:left=\"{indent}\" w:hanging=\"360\"/></w:pPr></w:lvl>"
            );
        }
        xml.push_str("</w:abstractNum>");
    }
    xml.push_str("<w:num w:numId=\"1\"><w:abstractNumId w:val=\"0\"/></w:num><w:num w:numId=\"2\"><w:abstractNumId w:val=\"1\"/></w:num></w:numbering>");
    xml
}

fn build_content_types_xml<'a>(extensions: impl Iterator<Item = &'a str>) -> String {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/word/document.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml"/><Override PartName="/word/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.styles+xml"/><Override PartName="/word/numbering.xml" ContentType="application/vnd.openxmlformats-officedocument.wordprocessingml.numbering+xml"/>"#,
    );
    let mut seen = HashSet::new();
    for extension in extensions {
        if !seen.insert(extension) {
            continue;
        }
        let content_type = match extension {
            "png" => "image/png",
            "jpg" | "jpeg" => "image/jpeg",
            "gif" => "image/gif",
            "bmp" => "image/bmp",
            _ => continue,
        };
        let _ = write!(
            xml,
            "<Default Extension=\"{extension}\" ContentType=\"{content_type}\"/>"
        );
    }
    xml.push_str("</Types>");
    xml
}

fn build_root_relationships_xml() -> String {
    String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="word/document.xml"/></Relationships>"#,
    )
}

fn build_document_relationships_xml<'a>(
    relationships: impl Iterator<Item = (&'a str, &'a str)>,
) -> String {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles" Target="styles.xml"/><Relationship Id="rId2" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/numbering" Target="numbering.xml"/>"#,
    );
    for (id, target) in relationships {
        let _ = write!(xml, "<Relationship Id=\"{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/image\" Target=\"{}\"/>", xml_escape(id), xml_escape(target));
    }
    xml.push_str("</Relationships>");
    xml
}

fn media_extension(content_type: &str) -> Option<&'static str> {
    match content_type.to_ascii_lowercase().as_str() {
        "image/png" => Some("png"),
        "image/jpeg" => Some("jpg"),
        "image/gif" => Some("gif"),
        "image/bmp" => Some("bmp"),
        _ => None,
    }
}

fn read_part<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &'static str,
) -> Result<Option<String>, DocxError> {
    let Some(buf) = read_part_bytes(archive, name)? else {
        return Ok(None);
    };
    String::from_utf8(buf)
        .map(Some)
        .map_err(|_| DocxError::NotUtf8(name))
}

fn read_part_bytes<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<Option<Vec<u8>>, DocxError> {
    let mut file = match archive.by_name(name) {
        Ok(file) => file,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut buf = Vec::with_capacity(file.size() as usize);
    file.read_to_end(&mut buf)?;
    Ok(Some(buf))
}
