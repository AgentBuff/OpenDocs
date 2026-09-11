//! Range formatting merges only explicitly selected properties in the canonical engine.
use oo_schema::{CellStyle, GridRange};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CellStyleField {
    NumberFormat,
    FontFamily,
    FontSize,
    FontBold,
    FontItalic,
    FontUnderline,
    FontStrikethrough,
    FontColor,
    FillBackground,
    HorizontalAlignment,
    VerticalAlignment,
    Wrap,
    Borders,
    BorderTop,
    BorderBottom,
    BorderLeft,
    BorderRight,
    OuterBorders,
}

/// Non-persistent row selection for a single range formatting transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RangeRowPattern {
    FirstRow,
    AlternatingRows,
}

impl RangeRowPattern {
    pub(crate) fn matches(self, range: GridRange, row: u32) -> bool {
        match self {
            Self::FirstRow => row == range.start_row,
            Self::AlternatingRows => (row - range.start_row) % 2 == 1,
        }
    }
}

pub(crate) fn apply_style_fields(
    base: &CellStyle,
    target: &CellStyle,
    fields: Option<&[CellStyleField]>,
    range: GridRange,
    row: u32,
    column: u32,
) -> CellStyle {
    let Some(fields) = fields else {
        return target.clone();
    };
    let mut result = base.clone();
    let font = target.font.clone().unwrap_or_default();
    let alignment = target.alignment.clone().unwrap_or_default();
    let edges = target.borders.clone().unwrap_or_default();
    for field in fields {
        match field {
            CellStyleField::NumberFormat => result.number_format = target.number_format.clone(),
            CellStyleField::FontFamily => {
                result.font.get_or_insert_default().family = font.family.clone()
            }
            CellStyleField::FontSize => result.font.get_or_insert_default().size = font.size,
            CellStyleField::FontBold => result.font.get_or_insert_default().bold = font.bold,
            CellStyleField::FontItalic => result.font.get_or_insert_default().italic = font.italic,
            CellStyleField::FontUnderline => {
                result.font.get_or_insert_default().underline = font.underline
            }
            CellStyleField::FontStrikethrough => {
                result.font.get_or_insert_default().strikethrough = font.strikethrough
            }
            CellStyleField::FontColor => {
                result.font.get_or_insert_default().color = font.color.clone()
            }
            CellStyleField::FillBackground => {
                result.fill.get_or_insert_default().background = target
                    .fill
                    .as_ref()
                    .and_then(|fill| fill.background.clone())
            }
            CellStyleField::HorizontalAlignment => {
                result.alignment.get_or_insert_default().horizontal = alignment.horizontal.clone()
            }
            CellStyleField::VerticalAlignment => {
                result.alignment.get_or_insert_default().vertical = alignment.vertical.clone()
            }
            CellStyleField::Wrap => result.alignment.get_or_insert_default().wrap = alignment.wrap,
            CellStyleField::Borders => result.borders = target.borders.clone(),
            CellStyleField::BorderTop => {
                result.borders.get_or_insert_default().top = edges.top.clone()
            }
            CellStyleField::BorderBottom => {
                result.borders.get_or_insert_default().bottom = edges.bottom.clone()
            }
            CellStyleField::BorderLeft => {
                result.borders.get_or_insert_default().left = edges.left.clone()
            }
            CellStyleField::BorderRight => {
                result.borders.get_or_insert_default().right = edges.right.clone()
            }
            CellStyleField::OuterBorders => {
                if row == range.start_row {
                    result.borders.get_or_insert_default().top = edges.top.clone();
                }
                if row == range.end_row {
                    result.borders.get_or_insert_default().bottom = edges.bottom.clone();
                }
                if column == range.start_column {
                    result.borders.get_or_insert_default().left = edges.left.clone();
                }
                if column == range.end_column {
                    result.borders.get_or_insert_default().right = edges.right.clone();
                }
            }
        }
    }
    if result.borders.as_ref().is_some_and(|b| {
        b.top.is_none() && b.bottom.is_none() && b.left.is_none() && b.right.is_none()
    }) {
        result.borders = None;
    }
    result
}
