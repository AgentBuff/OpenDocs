//! 解析 `word/styles.xml`：文档默认值 + 具名段落样式。

use std::collections::HashMap;

use quick_xml::events::Event;
use quick_xml::Reader;

use crate::props::{ParaProps, TextProps};
use crate::xml::{attr, local_name, read_para_props, read_text_props, skip_subtree};
use crate::DocxError;

/// 一条具名样式。
#[derive(Debug, Clone, Default)]
pub struct NamedStyle {
    pub based_on: Option<String>,
    pub para: ParaProps,
    pub text: TextProps,
}

/// 整份样式表。
#[derive(Debug, Clone, Default)]
pub struct StyleSheet {
    pub default_para: ParaProps,
    pub default_text: TextProps,
    pub styles: HashMap<String, NamedStyle>,
}

impl StyleSheet {
    /// 解析出某个具名样式在继承链展开后的完整声明。
    ///
    /// `w:basedOn` 可以串成一条链，这里自底向上展开。为防御构造错误的文档里出现
    /// 环状继承，最多向上追溯 [`MAX_INHERITANCE_DEPTH`] 层。
    pub fn resolve_named(&self, style_id: &str) -> (ParaProps, TextProps) {
        let mut chain: Vec<&NamedStyle> = Vec::new();
        let mut cursor = Some(style_id.to_string());
        let mut seen = Vec::new();

        while let Some(id) = cursor {
            if seen.contains(&id) || chain.len() >= MAX_INHERITANCE_DEPTH {
                break;
            }
            seen.push(id.clone());
            match self.styles.get(&id) {
                Some(style) => {
                    chain.push(style);
                    cursor = style.based_on.clone();
                }
                None => break,
            }
        }

        // chain 是「子 → 父」顺序，合并要从父开始，故反向遍历。
        let mut para = ParaProps::default();
        let mut text = TextProps::default();
        for style in chain.iter().rev() {
            para = para.merge(&style.para);
            text = text.merge(&style.text);
        }
        (para, text)
    }
}

const MAX_INHERITANCE_DEPTH: usize = 16;

pub fn parse_styles(xml: &str) -> Result<StyleSheet, DocxError> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(false);

    let mut sheet = StyleSheet::default();

    loop {
        match reader.read_event()? {
            Event::Start(e) => match local_name(&e).as_slice() {
                b"docDefaults" | b"styles" => {} // 继续下探
                b"rPrDefault" => {
                    // 内层可能直接就是 w:rPr，也可能是空的 rPrDefault。
                    if let Some(props) = read_nested_rpr(&mut reader)? {
                        sheet.default_text = props;
                    }
                }
                b"pPrDefault" => {
                    if let Some((para, text)) = read_nested_ppr(&mut reader)? {
                        sheet.default_para = para;
                        sheet.default_text = sheet.default_text.merge(&text);
                    }
                }
                b"style" => {
                    let style_type = attr(&e, "type");
                    let style_id = attr(&e, "styleId");
                    let parsed = read_style_body(&mut reader)?;
                    // 当前只用段落样式；字符样式（w:type="character"）留待后续。
                    if style_type.as_deref() == Some("paragraph") {
                        if let Some(id) = style_id {
                            sheet.styles.insert(id, parsed);
                        }
                    }
                }
                _ => skip_subtree(&mut reader, &e)?,
            },
            Event::Eof => break,
            _ => {}
        }
    }

    Ok(sheet)
}

/// 读取 `w:rPrDefault` 内部包裹的 `w:rPr`。
fn read_nested_rpr(reader: &mut Reader<&[u8]>) -> Result<Option<TextProps>, DocxError> {
    let mut found = None;
    loop {
        match reader.read_event()? {
            Event::Start(e) if local_name(&e) == b"rPr" => found = Some(read_text_props(reader)?),
            Event::Start(e) => skip_subtree(reader, &e)?,
            Event::End(_) => break,
            Event::Eof => return Err(DocxError::UnexpectedEof("w:rPrDefault")),
            _ => {}
        }
    }
    Ok(found)
}

/// 读取 `w:pPrDefault` 内部包裹的 `w:pPr`。
fn read_nested_ppr(
    reader: &mut Reader<&[u8]>,
) -> Result<Option<(ParaProps, TextProps)>, DocxError> {
    let mut found = None;
    loop {
        match reader.read_event()? {
            Event::Start(e) if local_name(&e) == b"pPr" => found = Some(read_para_props(reader)?),
            Event::Start(e) => skip_subtree(reader, &e)?,
            Event::End(_) => break,
            Event::Eof => return Err(DocxError::UnexpectedEof("w:pPrDefault")),
            _ => {}
        }
    }
    Ok(found)
}

/// 读取一条 `w:style` 的内容。
fn read_style_body(reader: &mut Reader<&[u8]>) -> Result<NamedStyle, DocxError> {
    let mut style = NamedStyle::default();
    loop {
        match reader.read_event()? {
            Event::Empty(e) => {
                if local_name(&e) == b"basedOn" {
                    style.based_on = attr(&e, "val");
                }
            }
            Event::Start(e) => match local_name(&e).as_slice() {
                b"pPr" => {
                    let (para, mark_text) = read_para_props(reader)?;
                    style.para = style.para.merge(&para);
                    style.text = style.text.merge(&mark_text);
                }
                b"rPr" => {
                    let text = read_text_props(reader)?;
                    style.text = style.text.merge(&text);
                }
                b"basedOn" => {
                    style.based_on = attr(&e, "val");
                    skip_subtree(reader, &e)?;
                }
                _ => skip_subtree(reader, &e)?,
            },
            Event::End(_) => break,
            Event::Eof => return Err(DocxError::UnexpectedEof("w:style")),
            _ => {}
        }
    }
    Ok(style)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::props::Alignment;

    const XML: &str = r#"<?xml version="1.0"?>
<w:styles xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:docDefaults>
    <w:rPrDefault><w:rPr><w:rFonts w:ascii="Calibri"/><w:sz w:val="22"/></w:rPr></w:rPrDefault>
    <w:pPrDefault><w:pPr><w:spacing w:after="160" w:line="276" w:lineRule="auto"/></w:pPr></w:pPrDefault>
  </w:docDefaults>
  <w:style w:type="paragraph" w:styleId="Base">
    <w:pPr><w:jc w:val="center"/></w:pPr>
    <w:rPr><w:b/></w:rPr>
  </w:style>
  <w:style w:type="paragraph" w:styleId="Derived">
    <w:basedOn w:val="Base"/>
    <w:rPr><w:sz w:val="32"/></w:rPr>
  </w:style>
  <w:style w:type="character" w:styleId="Ignored">
    <w:rPr><w:i/></w:rPr>
  </w:style>
</w:styles>"#;

    #[test]
    fn parses_document_defaults() {
        let sheet = parse_styles(XML).unwrap();
        assert_eq!(sheet.default_text.font_size, Some(11.0), "22 半磅 = 11 磅");
        assert_eq!(sheet.default_text.font_family.as_deref(), Some("Calibri"));
        assert_eq!(sheet.default_para.space_after, Some(8.0), "160 twip = 8 磅");
        assert_eq!(sheet.default_para.line_height, Some(1.15));
    }

    #[test]
    fn only_paragraph_styles_are_collected() {
        let sheet = parse_styles(XML).unwrap();
        assert!(sheet.styles.contains_key("Base"));
        assert!(sheet.styles.contains_key("Derived"));
        assert!(
            !sheet.styles.contains_key("Ignored"),
            "字符样式不应进入段落样式表"
        );
    }

    #[test]
    fn based_on_chain_is_flattened_child_wins() {
        let sheet = parse_styles(XML).unwrap();
        let (para, text) = sheet.resolve_named("Derived");
        assert_eq!(
            para.alignment,
            Some(Alignment::Center),
            "应继承父样式的对齐"
        );
        assert_eq!(text.bold, Some(true), "应继承父样式的加粗");
        assert_eq!(text.font_size, Some(16.0), "自身声明的字号应生效");
    }

    #[test]
    fn unknown_style_id_resolves_to_empty() {
        let sheet = parse_styles(XML).unwrap();
        let (para, text) = sheet.resolve_named("NoSuchStyle");
        assert_eq!(para, ParaProps::default());
        assert_eq!(text, TextProps::default());
    }
}
