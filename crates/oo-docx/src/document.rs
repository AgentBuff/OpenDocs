//! 解析 `word/document.xml`：段落、run、页面设置。

use std::collections::HashMap;

use oo_schema::{
    BlockAlignment, BlockData, BlockPresentation, DocumentBlock, DocumentBlockKind, DocumentModel,
    DocumentPageNumbering, DocumentSection, ImageBlock, InlineRun, InlineStyle, PageNumberFormat,
    PageSetup, ParagraphStyleRef, RichText,
};
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::props::{twips_to_pt, ParaProps, TextProps};
use crate::styles::StyleSheet;
use crate::xml::{
    attr, attr_f32, end_local_name, local_name, read_para_props, read_text_props, skip_subtree,
};
use crate::DocxError;

/// 导入时被压平或降级的结构计数。
///
/// 这些都是「已知但尚未建模」的内容：文字会保留，但结构语义只能降级表达。计数交给
/// [`crate::collect_docx_import_losses`] 汇总进损失报告，让调用方明确知道发生了什么，
/// 而不是静默丢失。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ImportLossCounters {
    /// `w:tbl` 被压平成段落的表格数。
    pub flattened_tables: usize,
    /// 含 `w:numPr` 但未保留编号语义的段落数。
    pub dropped_list_numbering: usize,
    /// 指向外部目标（`r:id` 或 `w:anchor`）但目标未被捕获的超链接数。
    pub dropped_hyperlink_targets: usize,
}

pub fn parse_document(
    xml: &str,
    sheet: &StyleSheet,
    doc_id: impl Into<String>,
    media_relations: &HashMap<String, String>,
) -> Result<(DocumentModel, ImportLossCounters), DocxError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    // ArtifactEnvelope 单独持有 artifactId；DocumentModel 只负责 block 树和页面设置。
    let _ = doc_id.into();
    let mut doc = DocumentModel::default();
    let mut losses = ImportLossCounters::default();
    let mut paragraph_seq = 0usize;
    let mut page_numbering = None;

    loop {
        match reader.read_event()? {
            Event::Start(e) => match local_name(&e).as_slice() {
                // 表格尚未建模。这里不跳过它的子树，而是让内部的 w:p
                // 落到下面的段落分支里——宁可把表格拍平成段落，也不要静默丢文字。
                // 代价是结构丢失，因此必须计数并进入损失报告。
                b"tbl" => losses.flattened_tables += 1,
                b"document" | b"body" | b"tr" | b"tc" => {}
                b"p" => {
                    paragraph_seq += 1;
                    let (para, media_ids) =
                        read_paragraph(&mut reader, sheet, paragraph_seq, &mut losses)?;
                    doc.root.push(para.id.clone());
                    doc.blocks.push(para);
                    append_image_blocks(&mut doc, paragraph_seq, media_ids, media_relations)?;
                }
                b"sectPr" => {
                    let section = read_section_properties(&mut reader)?;
                    doc.page_setup = section.0;
                    page_numbering = section.1;
                }
                _ => skip_subtree(&mut reader, &e)?,
            },
            // 空段落 <w:p/> 也要占一行。
            Event::Empty(e) if local_name(&e) == b"p" => {
                paragraph_seq += 1;
                let para = new_paragraph(
                    paragraph_seq,
                    String::new(),
                    Vec::new(),
                    &ParaProps::default().merge(&sheet.default_para),
                );
                doc.root.push(para.id.clone());
                doc.blocks.push(para);
            }
            Event::Eof => break,
            _ => {}
        }
    }

    if let Some(start_block_id) = doc.root.first().cloned() {
        if doc.page_setup.is_some() || page_numbering.is_some() {
            doc.page_semantics.sections.push(DocumentSection {
                id: "section-1".into(),
                start_block_id,
                page_setup: doc.page_setup.clone(),
                header: None,
                footer: None,
                page_numbering,
            });
        }
    }
    Ok((doc, losses))
}

/// 读取一个 `w:p`，直到它的结束标签。
fn read_paragraph(
    reader: &mut Reader<&[u8]>,
    sheet: &StyleSheet,
    seq: usize,
    losses: &mut ImportLossCounters,
) -> Result<(DocumentBlock, Vec<String>), DocxError> {
    let mut direct_para = ParaProps::default();
    // 段落标记自带的字符格式，作为本段所有 run 的基线之一。
    let mut mark_text = TextProps::default();
    let mut text = String::new();
    let mut runs: Vec<InlineRun> = Vec::new();
    let mut media_ids = Vec::new();

    loop {
        match reader.read_event()? {
            Event::Start(e) => match local_name(&e).as_slice() {
                b"pPr" => {
                    let (para, mark) = read_para_props(reader)?;
                    direct_para = direct_para.merge(&para);
                    mark_text = mark_text.merge(&mark);
                }
                b"r" => {
                    // 规范要求 w:pPr 出现在所有 run 之前，因此此刻 direct_para 与
                    // mark_text 已经就绪，可以直接算出这个 run 的样式基线。
                    let base_text = resolve_base_text(sheet, &direct_para, &mark_text);
                    read_run(reader, &base_text, &mut text, &mut runs, &mut media_ids)?;
                }
                // 超链接、书签等容器：不跳过子树，让内部的 w:r 照常被处理。
                // 链接文字因此保留，但目标需要解析 rels 或 w:anchor，目前不捕获——
                // 有目标就要计数，否则「文字还在、链接没了」会被误认为导入无损。
                b"hyperlink" => {
                    if attr(&e, "id").is_some() || attr(&e, "anchor").is_some() {
                        losses.dropped_hyperlink_targets += 1;
                    }
                }
                b"smartTag" | b"sdtContent" | b"sdt" => {}
                _ => skip_subtree(reader, &e)?,
            },
            Event::End(e) if end_local_name(&e) == b"p" => break,
            Event::Eof => return Err(DocxError::UnexpectedEof("w:p")),
            _ => {}
        }
    }

    let style = resolve_para_props(sheet, &direct_para);
    // 编号定义（numbering.xml）尚未解析，带 numPr 的段落只能保留层级。
    if style.list_level.is_some() {
        losses.dropped_list_numbering += 1;
    }
    Ok((new_paragraph(seq, text, runs, &style), media_ids))
}

fn append_image_blocks(
    doc: &mut DocumentModel,
    paragraph_seq: usize,
    media_ids: Vec<String>,
    media_relations: &HashMap<String, String>,
) -> Result<(), DocxError> {
    for (image_index, relation_id) in media_ids.into_iter().enumerate() {
        let asset_id = media_relations
            .get(&relation_id)
            .ok_or_else(|| DocxError::InvalidRelationship(relation_id.clone()))?;
        let image = DocumentBlock {
            id: format!("image-{paragraph_seq}-{image_index}"),
            kind: DocumentBlockKind::Image,
            presentation: BlockPresentation::default(),
            content: None,
            children: Vec::new(),
            data: BlockData::Image(ImageBlock {
                asset_id: asset_id.clone(),
                alt: String::new(),
                original_asset_id: None,
                transform: Default::default(),
                size: Default::default(),
                placement: Default::default(),
                caption: String::new(),
            }),
        };
        doc.root.push(image.id.clone());
        doc.blocks.push(image);
    }
    Ok(())
}

/// 段落样式的三层合并：文档默认 → 具名样式 → 直接格式。
fn resolve_para_props(sheet: &StyleSheet, direct: &ParaProps) -> ParaProps {
    let mut merged = sheet.default_para.clone();
    if let Some(style_id) = &direct.style_id {
        let (named_para, _) = sheet.resolve_named(style_id);
        merged = merged.merge(&named_para);
    }
    merged.merge(direct)
}

/// run 样式的基线：文档默认 → 具名段落样式里的字符格式 → 段落标记格式。
/// run 自己的 `w:rPr` 在此之上再覆盖一层。
fn resolve_base_text(sheet: &StyleSheet, direct: &ParaProps, mark: &TextProps) -> TextProps {
    let mut merged = sheet.default_text.clone();
    if let Some(style_id) = &direct.style_id {
        let (_, named_text) = sheet.resolve_named(style_id);
        merged = merged.merge(&named_text);
    }
    merged.merge(mark)
}

/// 读取一个 `w:r`，把文字追加进段落，并登记对应的样式区间。
fn read_run(
    reader: &mut Reader<&[u8]>,
    base: &TextProps,
    text: &mut String,
    runs: &mut Vec<InlineRun>,
    media_ids: &mut Vec<String>,
) -> Result<(), DocxError> {
    let mut props = base.clone();
    let start = text.chars().count();

    loop {
        match reader.read_event()? {
            Event::Start(e) => match local_name(&e).as_slice() {
                b"rPr" => props = base.merge(&read_text_props(reader)?),
                b"t" => {
                    // w:t 的内容可能被 CDATA 或实体分段，这里逐段拼接。
                    loop {
                        match reader.read_event()? {
                            Event::Text(t) => text.push_str(&t.unescape()?),
                            Event::CData(c) => {
                                text.push_str(&String::from_utf8_lossy(&c.into_inner()))
                            }
                            Event::End(_) => break,
                            Event::Eof => return Err(DocxError::UnexpectedEof("w:t")),
                            _ => {}
                        }
                    }
                }
                b"drawing" => media_ids.push(read_drawing(reader)?),
                _ => skip_subtree(reader, &e)?,
            },
            Event::Empty(e) => match local_name(&e).as_slice() {
                // 软换行：段落内的强制断行，排版引擎按 '\n' 处理。
                b"br" => text.push('\n'),
                b"tab" => text.push('\t'),
                b"noBreakHyphen" => text.push('-'),
                _ => {}
            },
            Event::End(e) if end_local_name(&e) == b"r" => break,
            Event::Eof => return Err(DocxError::UnexpectedEof("w:r")),
            _ => {}
        }
    }

    let end = text.chars().count();
    if end > start {
        push_run(
            runs,
            InlineRun {
                start,
                end,
                style: text_style(&props),
            },
        );
    }
    Ok(())
}

/// 读取一个 `w:drawing`，提取 DrawingML 图片关系的 `r:embed` 值。
fn read_drawing(reader: &mut Reader<&[u8]>) -> Result<String, DocxError> {
    let mut relation_id = None;
    loop {
        match reader.read_event()? {
            Event::Start(e) if local_name(&e).as_slice() == b"blip" => {
                relation_id = attr(&e, "embed");
                skip_subtree(reader, &e)?;
            }
            Event::Empty(e) if local_name(&e).as_slice() == b"blip" => {
                relation_id = attr(&e, "embed");
            }
            Event::End(e) if end_local_name(&e) == b"drawing" => {
                return relation_id
                    .ok_or_else(|| DocxError::InvalidRelationship("drawing 缺少图片关系".into()));
            }
            Event::Eof => return Err(DocxError::UnexpectedEof("w:drawing")),
            _ => {}
        }
    }
}

/// 追加样式区间，与前一个区间样式相同时直接延长，避免产生大量碎片区间。
fn push_run(runs: &mut Vec<InlineRun>, run: InlineRun) {
    match runs.last_mut() {
        Some(last) if last.end == run.start && last.style == run.style => last.end = run.end,
        _ => runs.push(run),
    }
}

fn new_paragraph(
    seq: usize,
    text: String,
    runs: Vec<InlineRun>,
    props: &ParaProps,
) -> DocumentBlock {
    DocumentBlock {
        id: format!("p-{seq}"),
        kind: DocumentBlockKind::Paragraph,
        presentation: paragraph_presentation(props),
        content: Some(RichText { text, runs }),
        children: Vec::new(),
        data: BlockData::None,
    }
}

fn paragraph_presentation(props: &ParaProps) -> BlockPresentation {
    let style = props.resolve();
    BlockPresentation {
        align: match style.alignment {
            crate::props::Alignment::Left => BlockAlignment::Left,
            crate::props::Alignment::Center => BlockAlignment::Center,
            crate::props::Alignment::Right => BlockAlignment::Right,
            crate::props::Alignment::Justify => BlockAlignment::Justify,
        },
        list: None,
        // Document presentation stores a bounded indentation level rather than a physical
        // OOXML measurement. Keep the adapter conversion here and never persist OOXML attrs.
        indent_start: style.indent_left.round().clamp(0.0, 20.0) as u8,
        indent_end: style.indent_right,
        spacing_before: style.space_before,
        spacing_after: style.space_after,
        line_height: style.line_height,
        named_style: props
            .style_id
            .as_ref()
            .map(|name| ParagraphStyleRef { name: name.clone() }),
    }
}

fn text_style(props: &TextProps) -> InlineStyle {
    serde_json::from_value(
        serde_json::to_value(props.resolve()).expect("text style must serialize"),
    )
    .expect("text style must match InlineStyle schema")
}

/// 读取 `w:sectPr` 中的纸张尺寸与页边距。
fn read_section_properties(
    reader: &mut Reader<&[u8]>,
) -> Result<(Option<PageSetup>, Option<DocumentPageNumbering>), DocxError> {
    let mut page = PageSetup {
        width: 595.28,
        height: 841.89,
        margin_top: 72.0,
        margin_right: 72.0,
        margin_bottom: 72.0,
        margin_left: 72.0,
    };
    let mut page_numbering = None;
    loop {
        let event = reader.read_event()?;
        // sectPr 的子元素几乎都是自闭合的；带子树的（如 w:footnotePr）整体跳过，
        // 否则它们的结束标签会被误判成 sectPr 结束。
        if let Event::Start(e) = &event {
            if !matches!(local_name(e).as_slice(), b"pgSz" | b"pgMar" | b"pgNumType") {
                skip_subtree(reader, e)?;
                continue;
            }
        }
        match event {
            Event::Empty(e) | Event::Start(e) => {
                match local_name(&e).as_slice() {
                    b"pgSz" => {
                        if let Some(w) = attr_f32(&e, "w") {
                            page.width = twips_to_pt(w);
                        }
                        if let Some(h) = attr_f32(&e, "h") {
                            page.height = twips_to_pt(h);
                        }
                        // 横向纸张：w:orient="landscape" 时 pgSz 已给出交换后的尺寸，
                        // 无需额外处理，这里仅确保方向属性不被误当作长度。
                        let _ = attr(&e, "orient");
                    }
                    b"pgMar" => {
                        if let Some(v) = attr_f32(&e, "top") {
                            page.margin_top = twips_to_pt(v);
                        }
                        if let Some(v) = attr_f32(&e, "right") {
                            page.margin_right = twips_to_pt(v);
                        }
                        if let Some(v) = attr_f32(&e, "bottom") {
                            page.margin_bottom = twips_to_pt(v);
                        }
                        if let Some(v) = attr_f32(&e, "left") {
                            page.margin_left = twips_to_pt(v);
                        }
                    }
                    b"pgNumType" => {
                        let start_at = attr(&e, "start")
                            .and_then(|value| value.parse::<u32>().ok())
                            .filter(|value| *value > 0)
                            .unwrap_or(1);
                        let format = match attr(&e, "fmt").as_deref() {
                            Some("upperRoman") => PageNumberFormat::UpperRoman,
                            Some("lowerRoman") => PageNumberFormat::LowerRoman,
                            Some("upperLetter") => PageNumberFormat::UpperLetter,
                            Some("lowerLetter") => PageNumberFormat::LowerLetter,
                            _ => PageNumberFormat::Decimal,
                        };
                        page_numbering = Some(DocumentPageNumbering { start_at, format });
                    }
                    _ => {}
                }
            }
            Event::End(e) if end_local_name(&e) == b"sectPr" => break,
            Event::Eof => return Err(DocxError::UnexpectedEof("w:sectPr")),
            _ => {}
        }
    }
    Ok((Some(page), page_numbering))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(xml: &str) -> (DocumentModel, ImportLossCounters) {
        parse_document(xml, &StyleSheet::default(), "doc", &HashMap::new()).unwrap()
    }

    /// 表格被压平、列表编号被丢弃、超链接目标被丢弃——这三种降级以前完全不进损失
    /// 报告，导入方只能靠肉眼发现「表格变成了一堆段落」。它们必须被计数。
    #[test]
    fn structural_downgrades_are_counted() {
        let xml = r#"<w:document><w:body>
            <w:tbl><w:tr><w:tc><w:p><w:r><w:t>单元格</w:t></w:r></w:p></w:tc></w:tr></w:tbl>
            <w:p><w:pPr><w:numPr><w:ilvl w:val="2"/><w:numId w:val="1"/></w:numPr></w:pPr><w:r><w:t>列表项</w:t></w:r></w:p>
            <w:p><w:hyperlink r:id="rId7"><w:r><w:t>外链文字</w:t></w:r></w:hyperlink></w:p>
            <w:p><w:hyperlink w:anchor="section-2"><w:r><w:t>书签文字</w:t></w:r></w:hyperlink></w:p>
        </w:body></w:document>"#;

        let (model, losses) = parse(xml);

        assert_eq!(losses.flattened_tables, 1);
        assert_eq!(losses.dropped_list_numbering, 1);
        assert_eq!(losses.dropped_hyperlink_targets, 2);

        // 降级的是结构，不是内容：文字必须一个不少。
        let text = model.plain_text();
        for expected in ["单元格", "列表项", "外链文字", "书签文字"] {
            assert!(text.contains(expected), "丢失了 {expected}：{text:?}");
        }
    }

    /// 没有这些结构时不能凭空产生损失计数。
    #[test]
    fn plain_paragraphs_report_no_structural_losses() {
        let xml = r#"<w:document><w:body><w:p><w:r><w:t>普通段落</w:t></w:r></w:p></w:body></w:document>"#;

        let (model, losses) = parse(xml);

        assert_eq!(losses, ImportLossCounters::default());
        assert_eq!(model.plain_text().trim(), "普通段落");
    }

    /// 没有目标的超链接容器（例如仅作书签锚点占位）不该被算作链接丢失。
    #[test]
    fn hyperlink_without_a_target_is_not_reported() {
        let xml = r#"<w:document><w:body><w:p><w:hyperlink><w:r><w:t>纯文字</w:t></w:r></w:hyperlink></w:p></w:body></w:document>"#;

        let (_, losses) = parse(xml);

        assert_eq!(losses.dropped_hyperlink_targets, 0);
    }
}
