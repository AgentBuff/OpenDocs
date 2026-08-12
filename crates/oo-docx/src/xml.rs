//! quick-xml 之上的一层薄封装，处理 docx XML 的两个共性问题：
//! 命名空间前缀（`w:p` / `p` 都要能匹配）和 `w:rPr`/`w:pPr` 这类属性块的重复解析。

use quick_xml::events::{BytesEnd, BytesStart, Event};
use quick_xml::Reader;

use crate::props::{
    parse_alignment, parse_color, parse_highlight, parse_on_off, twips_to_pt, ParaProps, TextProps,
    VerticalAlign,
};
use crate::DocxError;

/// 取开始标签的本地名（去掉命名空间前缀）。
pub fn local_name(e: &BytesStart<'_>) -> Vec<u8> {
    e.local_name().as_ref().to_vec()
}

/// 取结束标签的本地名。
pub fn end_local_name(e: &BytesEnd<'_>) -> Vec<u8> {
    e.local_name().as_ref().to_vec()
}

/// 按本地名读取属性值。docx 的属性同样带 `w:` 前缀。
pub fn attr(e: &BytesStart<'_>, name: &str) -> Option<String> {
    e.attributes().flatten().find_map(|a| {
        if a.key.local_name().as_ref() == name.as_bytes() {
            a.unescape_value().ok().map(|v| v.into_owned())
        } else {
            None
        }
    })
}

/// 读取属性并解析为浮点数。
pub fn attr_f32(e: &BytesStart<'_>, name: &str) -> Option<f32> {
    attr(e, name)?.trim().parse::<f32>().ok()
}

/// 消费掉当前元素的整个子树。
pub fn skip_subtree(reader: &mut Reader<&[u8]>, e: &BytesStart<'_>) -> Result<(), DocxError> {
    reader.read_to_end(e.name())?;
    Ok(())
}

/// 解析一个 `w:rPr` 块，读到它的结束标签为止。调用时 `w:rPr` 的开始标签已被消费。
pub fn read_text_props(reader: &mut Reader<&[u8]>) -> Result<TextProps, DocxError> {
    let mut props = TextProps::default();
    loop {
        match reader.read_event()? {
            Event::Empty(e) => apply_text_prop(&mut props, &e),
            Event::Start(e) => {
                apply_text_prop(&mut props, &e);
                // 属性元素一般自闭合；带子元素时整体跳过，避免把子元素的结束标签
                // 误当成 w:rPr 的结束而提前退出。
                skip_subtree(reader, &e)?;
            }
            Event::End(_) => break,
            Event::Eof => return Err(DocxError::UnexpectedEof("w:rPr")),
            _ => {}
        }
    }
    Ok(props)
}

/// 解析一个 `w:pPr` 块，读到它的结束标签为止。
///
/// `w:pPr` 里可以嵌一个 `w:rPr`（段落标记本身的字符格式），它同样是这个段落
/// 默认字符格式的一部分，因此一并返回。
pub fn read_para_props(reader: &mut Reader<&[u8]>) -> Result<(ParaProps, TextProps), DocxError> {
    let mut para = ParaProps::default();
    let mut text = TextProps::default();
    loop {
        match reader.read_event()? {
            Event::Empty(e) => apply_para_prop(&mut para, &e),
            Event::Start(e) => {
                if local_name(&e) == b"rPr" {
                    text = read_text_props(reader)?;
                } else {
                    apply_para_prop(&mut para, &e);
                    skip_subtree(reader, &e)?;
                }
            }
            Event::End(_) => break,
            Event::Eof => return Err(DocxError::UnexpectedEof("w:pPr")),
            _ => {}
        }
    }
    Ok((para, text))
}

/// 把 `w:rPr` 的一个子元素应用到 [`TextProps`] 上。
pub fn apply_text_prop(props: &mut TextProps, e: &BytesStart<'_>) {
    match local_name(e).as_slice() {
        b"rFonts" => {
            // eastAsia 优先：中文文档里正文字体通常写在这个属性上。
            props.font_family = attr(e, "eastAsia")
                .or_else(|| attr(e, "ascii"))
                .or_else(|| attr(e, "hAnsi"))
                .filter(|f| !f.is_empty());
        }
        // w:sz 的单位是半磅。
        b"sz" | b"szCs" => {
            if let Some(half_pt) = attr_f32(e, "val") {
                props.font_size = Some(half_pt / 2.0);
            }
        }
        b"b" => props.bold = Some(parse_on_off(attr(e, "val").as_deref())),
        b"i" => props.italic = Some(parse_on_off(attr(e, "val").as_deref())),
        b"u" => {
            // w:u 的 val 是线型，"none" 表示不加下划线。
            let val = attr(e, "val");
            props.underline = Some(!matches!(val.as_deref(), Some("none") | None));
        }
        b"color" => {
            if let Some(v) = attr(e, "val") {
                props.color = parse_color(&v);
            }
        }
        b"strike" => props.strikethrough = Some(parse_on_off(attr(e, "val").as_deref())),
        // w:dstrike 是双删除线，模型里只有一种，按删除线处理。
        b"dstrike" => props.strikethrough = Some(parse_on_off(attr(e, "val").as_deref())),
        b"highlight" => {
            props.highlight = Some(attr(e, "val").and_then(|v| parse_highlight(&v)));
        }
        b"vertAlign" => {
            props.vertical_align = Some(match attr(e, "val").as_deref() {
                Some("superscript") => VerticalAlign::Superscript,
                Some("subscript") => VerticalAlign::Subscript,
                _ => VerticalAlign::Baseline,
            });
        }
        _ => {}
    }
}

/// 把 `w:pPr` 的一个子元素应用到 [`ParaProps`] 上。
pub fn apply_para_prop(props: &mut ParaProps, e: &BytesStart<'_>) {
    match local_name(e).as_slice() {
        b"pStyle" => props.style_id = attr(e, "val"),
        b"ilvl" => {
            props.list_level = attr(e, "val").and_then(|v| v.parse::<u8>().ok());
        }
        b"jc" => {
            if let Some(v) = attr(e, "val") {
                props.alignment = parse_alignment(&v);
            }
        }
        b"spacing" => {
            if let Some(before) = attr_f32(e, "before") {
                props.space_before = Some(twips_to_pt(before));
            }
            if let Some(after) = attr_f32(e, "after") {
                props.space_after = Some(twips_to_pt(after));
            }
            if let Some(line) = attr_f32(e, "line") {
                props.line_height =
                    ParaProps::line_height_from(line, attr(e, "lineRule").as_deref());
            }
        }
        b"ind" => {
            if let Some(left) = attr_f32(e, "left").or_else(|| attr_f32(e, "start")) {
                props.indent_left = Some(twips_to_pt(left));
            }
            if let Some(right) = attr_f32(e, "right").or_else(|| attr_f32(e, "end")) {
                props.indent_right = Some(twips_to_pt(right));
            }
            if let Some(first) = attr_f32(e, "firstLine") {
                props.indent_first_line = Some(twips_to_pt(first));
            }
            // 悬挂缩进是负的首行缩进。
            if let Some(hanging) = attr_f32(e, "hanging") {
                props.indent_first_line = Some(-twips_to_pt(hanging));
            }
        }
        _ => {}
    }
}
