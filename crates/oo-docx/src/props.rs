//! docx 的「部分样式」及其合并规则。
//!
//! ECMA-376 里的样式是层层覆盖的：文档默认值（`docDefaults`）→ 具名段落样式
//! （`w:pStyle` 指向的 `w:style`）→ 直接格式（写在段落/run 上的 `w:pPr`/`w:rPr`）。
//! 每一层只声明它关心的属性，因此这里的字段全是 `Option`，由 [`TextProps::merge`]
//! 和 [`ParaProps::merge`] 按「后者覆盖前者」的方向合并，最后再落成模型层的完整样式。

use serde::Serialize;

/// DOCX importer 内部的样式值。它们只负责把 Word 属性解析成 typed
/// `BlockPresentation`，不属于文档存储模型；解析完成后由 adapter 显式映射到 schema。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum VerticalAlign {
    #[default]
    Baseline,
    Superscript,
    Subscript,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Alignment {
    #[default]
    Left,
    Center,
    Right,
    Justify,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TextStyle {
    pub font_family: String,
    pub font_size: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub color: String,
    pub highlight: Option<String>,
    pub vertical_align: VerticalAlign,
}

impl Default for TextStyle {
    fn default() -> Self {
        Self {
            font_family: "Calibri".into(),
            font_size: 11.0,
            bold: false,
            italic: false,
            underline: false,
            strikethrough: false,
            color: "#000000".into(),
            highlight: None,
            vertical_align: VerticalAlign::Baseline,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ListKind {
    Bullet,
    Ordered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ListStyle {
    pub kind: ListKind,
    pub level: u8,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ParagraphStyle {
    pub alignment: Alignment,
    pub list: Option<ListStyle>,
    pub line_height: f32,
    pub space_before: f32,
    pub space_after: f32,
    pub indent_left: f32,
    pub indent_right: f32,
    pub indent_first_line: f32,
}

impl Default for ParagraphStyle {
    fn default() -> Self {
        Self {
            alignment: Alignment::Left,
            list: None,
            line_height: 1.15,
            space_before: 0.0,
            space_after: 8.0,
            indent_left: 0.0,
            indent_right: 0.0,
            indent_first_line: 0.0,
        }
    }
}

/// 一个 twip（缇）等于 1/20 磅，是 docx 里长度的通用单位。
pub const TWIPS_PER_PT: f32 = 20.0;
/// `w:spacing/@w:line` 在 `lineRule="auto"` 下以 1/240 行为单位。
const LINE_UNITS_PER_LINE: f32 = 240.0;

pub fn twips_to_pt(twips: f32) -> f32 {
    twips / TWIPS_PER_PT
}

/// 行内样式的部分声明（对应 `w:rPr`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TextProps {
    pub font_family: Option<String>,
    /// 已换算为磅（`w:sz` 的值是半磅）。
    pub font_size: Option<f32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub underline: Option<bool>,
    pub strikethrough: Option<bool>,
    /// 已补上 `#` 前缀的 CSS 颜色。
    pub color: Option<String>,
    /// 文字高亮底色。`Some(None)` 表示显式取消高亮。
    pub highlight: Option<Option<String>>,
    pub vertical_align: Option<VerticalAlign>,
}

impl TextProps {
    /// 用 `over` 覆盖 `self`，`over` 中为 `None` 的字段保持不变。
    pub fn merge(&self, over: &TextProps) -> TextProps {
        TextProps {
            font_family: over
                .font_family
                .clone()
                .or_else(|| self.font_family.clone()),
            font_size: over.font_size.or(self.font_size),
            bold: over.bold.or(self.bold),
            italic: over.italic.or(self.italic),
            underline: over.underline.or(self.underline),
            strikethrough: over.strikethrough.or(self.strikethrough),
            color: over.color.clone().or_else(|| self.color.clone()),
            highlight: over.highlight.clone().or_else(|| self.highlight.clone()),
            vertical_align: over.vertical_align.or(self.vertical_align),
        }
    }

    /// 落成完整样式，未声明的属性取模型层默认值。
    pub fn resolve(&self) -> TextStyle {
        let base = TextStyle::default();
        TextStyle {
            font_family: self.font_family.clone().unwrap_or(base.font_family),
            font_size: self.font_size.unwrap_or(base.font_size),
            bold: self.bold.unwrap_or(base.bold),
            italic: self.italic.unwrap_or(base.italic),
            underline: self.underline.unwrap_or(base.underline),
            strikethrough: self.strikethrough.unwrap_or(base.strikethrough),
            color: self.color.clone().unwrap_or(base.color),
            highlight: self.highlight.clone().unwrap_or(base.highlight),
            vertical_align: self.vertical_align.unwrap_or(base.vertical_align),
        }
    }
}

/// 段落样式的部分声明（对应 `w:pPr`）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ParaProps {
    /// 指向具名样式的 `w:pStyle w:val`。
    pub style_id: Option<String>,
    /// docx 的列表信息在 `w:numPr` 里，Phase 3 才做完整的编号定义解析；
    /// 目前只记住层级，种类由前端设置。
    pub list_level: Option<u8>,
    pub alignment: Option<Alignment>,
    /// 行高倍数。
    pub line_height: Option<f32>,
    /// 以下长度单位均为磅。
    pub space_before: Option<f32>,
    pub space_after: Option<f32>,
    pub indent_left: Option<f32>,
    pub indent_right: Option<f32>,
    pub indent_first_line: Option<f32>,
}

impl ParaProps {
    pub fn merge(&self, over: &ParaProps) -> ParaProps {
        ParaProps {
            // style_id 只应来自直接格式，合并时同样遵循「后者优先」。
            style_id: over.style_id.clone().or_else(|| self.style_id.clone()),
            list_level: over.list_level.or(self.list_level),
            alignment: over.alignment.or(self.alignment),
            line_height: over.line_height.or(self.line_height),
            space_before: over.space_before.or(self.space_before),
            space_after: over.space_after.or(self.space_after),
            indent_left: over.indent_left.or(self.indent_left),
            indent_right: over.indent_right.or(self.indent_right),
            indent_first_line: over.indent_first_line.or(self.indent_first_line),
        }
    }

    pub fn resolve(&self) -> ParagraphStyle {
        let base = ParagraphStyle::default();
        ParagraphStyle {
            alignment: self.alignment.unwrap_or(base.alignment),
            // 编号定义（numbering.xml）尚未解析，因此这里不产出列表。
            list: base.list,
            line_height: self.line_height.unwrap_or(base.line_height),
            space_before: self.space_before.unwrap_or(base.space_before),
            space_after: self.space_after.unwrap_or(base.space_after),
            indent_left: self.indent_left.unwrap_or(base.indent_left),
            indent_right: self.indent_right.unwrap_or(base.indent_right),
            indent_first_line: self.indent_first_line.unwrap_or(base.indent_first_line),
        }
    }

    /// 解析 `w:spacing/@w:line` 与 `@w:lineRule`。
    ///
    /// `auto` 表示倍数（240 = 单倍行距）；`exact`/`atLeast` 给的是绝对磅值，这里
    /// 折算不出倍数，交给调用方按字号换算，因此返回 `None` 由默认值兜底。
    pub fn line_height_from(line: f32, rule: Option<&str>) -> Option<f32> {
        match rule.unwrap_or("auto") {
            "auto" => Some(line / LINE_UNITS_PER_LINE),
            _ => None,
        }
    }
}

/// 解析 `w:b`、`w:i` 这类布尔开关。缺省 `w:val` 时表示 true。
pub fn parse_on_off(val: Option<&str>) -> bool {
    !matches!(val, Some("0") | Some("false") | Some("off"))
}

/// 解析 `w:jc w:val`。
pub fn parse_alignment(val: &str) -> Option<Alignment> {
    match val {
        "left" | "start" => Some(Alignment::Left),
        "center" => Some(Alignment::Center),
        "right" | "end" => Some(Alignment::Right),
        "both" | "justify" | "distribute" => Some(Alignment::Justify),
        _ => None,
    }
}

/// 解析 `w:highlight w:val` 的具名颜色。
///
/// docx 用的是一组固定的名字而不是十六进制值，这里映射成接近 Word 呈现的颜色。
pub fn parse_highlight(val: &str) -> Option<String> {
    let color = match val {
        "yellow" => "#ffff00",
        "green" => "#00ff00",
        "cyan" => "#00ffff",
        "magenta" => "#ff00ff",
        "blue" => "#0000ff",
        "red" => "#ff0000",
        "darkBlue" => "#000080",
        "darkCyan" => "#008080",
        "darkGreen" => "#008000",
        "darkMagenta" => "#800080",
        "darkRed" => "#800000",
        "darkYellow" => "#808000",
        "darkGray" => "#808080",
        "lightGray" => "#c0c0c0",
        "black" => "#000000",
        "white" => "#ffffff",
        "none" => return None,
        _ => return None,
    };
    Some(color.to_string())
}

/// 解析 `w:color w:val`。`auto` 表示由阅读器决定，按黑色处理。
pub fn parse_color(val: &str) -> Option<String> {
    if val.eq_ignore_ascii_case("auto") {
        return Some("#000000".to_string());
    }
    let hex = val.trim_start_matches('#');
    if hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        Some(format!("#{hex}"))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_prefers_the_overriding_layer() {
        let base = TextProps {
            font_size: Some(11.0),
            bold: Some(true),
            ..Default::default()
        };
        let over = TextProps {
            bold: Some(false),
            italic: Some(true),
            ..Default::default()
        };
        let merged = base.merge(&over);
        assert_eq!(merged.font_size, Some(11.0), "未被覆盖的字段应保留");
        assert_eq!(merged.bold, Some(false), "被覆盖的字段应取新值");
        assert_eq!(merged.italic, Some(true), "新增字段应生效");
    }

    #[test]
    fn resolve_falls_back_to_model_defaults() {
        let style = TextProps::default().resolve();
        assert_eq!(style, TextStyle::default());
    }

    #[test]
    fn on_off_defaults_to_true_when_val_absent() {
        assert!(parse_on_off(None));
        assert!(parse_on_off(Some("1")));
        assert!(!parse_on_off(Some("0")));
        assert!(!parse_on_off(Some("false")));
    }

    #[test]
    fn line_rule_auto_yields_multiplier() {
        assert_eq!(ParaProps::line_height_from(276.0, Some("auto")), Some(1.15));
        assert_eq!(ParaProps::line_height_from(240.0, None), Some(1.0));
        assert_eq!(ParaProps::line_height_from(240.0, Some("exact")), None);
    }

    #[test]
    fn highlight_names_map_to_colors() {
        assert_eq!(parse_highlight("yellow").as_deref(), Some("#ffff00"));
        assert_eq!(parse_highlight("none"), None, "none 表示不高亮");
        assert_eq!(parse_highlight("不存在的名字"), None);
    }

    #[test]
    fn color_parsing_handles_auto_and_invalid() {
        assert_eq!(parse_color("C00000").as_deref(), Some("#C00000"));
        assert_eq!(parse_color("auto").as_deref(), Some("#000000"));
        assert_eq!(parse_color("zzz"), None);
    }

    #[test]
    fn twips_convert_to_points() {
        assert_eq!(twips_to_pt(1440.0), 72.0, "1440 twip 应等于一英寸");
    }
}
