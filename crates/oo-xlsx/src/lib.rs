//! Minimal, explicit XLSX adapter for the canonical [`oo_schema::SpreadsheetModel`].
//!
//! This crate owns ZIP/XML concerns only. It does not execute spreadsheet commands, calculate
//! formulas, or enter the `oo-spreadsheet` engine. Import reads workbook relationships, sheets,
//! shared strings, inline strings, numbers and formulas into the sparse model. Export emits the
//! same supported range as a valid XLSX package. Unsupported package parts are reported in the
//! import report; unsupported model attributes fail export instead of being silently discarded.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Write as _;
use std::io::{Cursor, Read, Write};

use oo_schema::{
    AlignmentStyle, ArtifactEnvelope, ArtifactPayload, CellBorderEdge, CellBorders, CellModel,
    CellStyle, DateSystem, FillStyle, FontStyle, FreezePane, GridRange, SheetMetadata, SheetModel,
    SheetVisibility, SpreadsheetMetadata, SpreadsheetModel,
};
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde_json::{Map, Number, Value};
use zip::write::SimpleFileOptions;

const CONTENT_TYPES: &str = "[Content_Types].xml";
const ROOT_RELS: &str = "_rels/.rels";
const WORKBOOK: &str = "xl/workbook.xml";
const WORKBOOK_RELS: &str = "xl/_rels/workbook.xml.rels";
const SHARED_STRINGS: &str = "xl/sharedStrings.xml";
const STYLES: &str = "xl/styles.xml";

#[derive(Debug, Clone, PartialEq)]
pub struct XlsxImportResult {
    pub model: SpreadsheetModel,
    /// ZIP parts intentionally not consumed by this minimal adapter (comments, drawings, styles,
    /// charts, external links, and arbitrary vendor extensions). They are never silently claimed
    /// to be represented in `SpreadsheetModel`.
    pub ignored_parts: Vec<String>,
    /// Parts that were present but whose semantics are not represented by the canonical model.
    /// This is intentionally structured instead of a lossy boolean so callers can show a loss
    /// report or decide whether the import is acceptable.
    pub unsupported_parts: Vec<XlsxUnsupportedPart>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XlsxUnsupportedPart {
    pub part: String,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XlsxLossReport {
    pub unsupported: Vec<XlsxUnsupportedPart>,
}

impl XlsxLossReport {
    pub fn is_lossless(&self) -> bool {
        self.unsupported.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct XlsxSemanticDiff {
    pub differences: Vec<String>,
}

impl XlsxSemanticDiff {
    pub fn is_equivalent(&self) -> bool {
        self.differences.is_empty()
    }
}

#[derive(Debug, thiserror::Error)]
pub enum XlsxError {
    #[error("不是有效的 XLSX zip 包：{0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("XLSX XML 解析失败：{0}")]
    Xml(#[from] quick_xml::Error),
    #[error("读取 XLSX 部件失败：{0}")]
    Io(#[from] std::io::Error),
    #[error("部件 {0} 不是合法的 UTF-8")]
    NotUtf8(String),
    #[error("缺少必需的 XLSX 部件 {0}")]
    MissingPart(&'static str),
    #[error("XLSX 部件格式无效：{0}")]
    InvalidPart(String),
    #[error("不支持的 XLSX 单元格类型：{0}")]
    UnsupportedCellType(String),
    #[error("不支持导出的 Spreadsheet 属性：{0}")]
    UnsupportedFeature(String),
    #[error("XLSX cell 引用无效：{0}")]
    InvalidCellReference(String),
    #[error("Spreadsheet schema 校验失败：{0}")]
    Schema(#[from] oo_schema::SchemaValidationError),
    #[error("XLSX XML 提前结束：{0}")]
    UnexpectedEof(&'static str),
}

/// Parse an XLSX package and return its canonical sparse spreadsheet model.
pub fn read_xlsx(bytes: &[u8]) -> Result<SpreadsheetModel, XlsxError> {
    let imported = read_xlsx_with_report(bytes)?;
    if let Some(first) = imported.unsupported_parts.first() {
        return Err(XlsxError::UnsupportedFeature(format!(
            "{}: {}",
            first.part, first.reason
        )));
    }
    Ok(imported.model)
}

/// Parse an XLSX package while making unsupported ZIP parts explicit to the caller.
pub fn read_xlsx_with_report(bytes: &[u8]) -> Result<XlsxImportResult, XlsxError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes))?;
    let all_parts = archive
        .file_names()
        .filter(|name| !name.ends_with('/'))
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut unique_parts = BTreeSet::new();
    for part in &all_parts {
        if !unique_parts.insert(part) {
            return Err(XlsxError::InvalidPart(format!("ZIP 部件重复：{part}")));
        }
    }
    require_part(&all_parts, CONTENT_TYPES)?;
    require_part(&all_parts, ROOT_RELS)?;
    let workbook_xml = read_part(&mut archive, WORKBOOK)?;
    let workbook_rels_xml = read_part(&mut archive, WORKBOOK_RELS)?;
    let workbook = parse_workbook(&workbook_xml)?;
    let (date_system, calculation_mode, active_sheet_index) =
        parse_workbook_settings(&workbook_xml)?;
    let relationships = parse_relationships(&workbook_rels_xml)?;

    let shared_strings = if all_parts.iter().any(|part| part == SHARED_STRINGS) {
        parse_shared_strings(&read_part(&mut archive, SHARED_STRINGS)?)?
    } else {
        Vec::new()
    };
    let styles_bytes = all_parts
        .iter()
        .any(|part| part == STYLES)
        .then(|| read_part(&mut archive, STYLES))
        .transpose()?;
    let styles = styles_bytes
        .as_deref()
        .map(parse_styles)
        .transpose()?
        .unwrap_or_default();
    let differential_styles = styles_bytes
        .as_deref()
        .map(parse_differential_styles)
        .transpose()?
        .unwrap_or_default();
    let style_unsupported_parts = styles_bytes
        .as_deref()
        .map(style_unsupported_parts)
        .unwrap_or_default();

    let mut sheets = Vec::with_capacity(workbook.len());
    let mut embedded_unsupported = Vec::new();
    let mut consumed = BTreeSet::from([
        CONTENT_TYPES.to_string(),
        ROOT_RELS.to_string(),
        WORKBOOK.to_string(),
        WORKBOOK_RELS.to_string(),
    ]);
    if all_parts.iter().any(|part| part == SHARED_STRINGS) {
        consumed.insert(SHARED_STRINGS.to_string());
    }
    if all_parts.iter().any(|part| part == STYLES) {
        consumed.insert(STYLES.to_string());
    }

    for (position, sheet) in workbook.iter().enumerate() {
        let target = relationships.get(&sheet.relationship_id).ok_or_else(|| {
            XlsxError::InvalidPart(format!(
                "workbook sheet {} 引用未知关系 {}",
                sheet.name, sheet.relationship_id
            ))
        })?;
        if target.external {
            return Err(XlsxError::UnsupportedFeature(format!(
                "外部 worksheet relationship {}",
                sheet.relationship_id
            )));
        }
        let target = normalize_target(&target.target)?;
        let xml = read_part(&mut archive, &target)?;
        consumed.insert(target.clone());
        embedded_unsupported.extend(worksheet_unsupported_parts(&target, &xml));
        let mut id = format!("sheet-{}", sheet.sheet_id);
        if id == "sheet-" {
            id = format!("sheet-{}", position + 1);
        }
        while sheets.iter().any(|existing: &SheetModel| existing.id == id) {
            id.push_str("-copy");
        }
        let mut metadata = parse_worksheet_metadata(&xml, &differential_styles)?;
        metadata.visibility = sheet.visibility;
        let cells = parse_worksheet(&xml, &shared_strings, &styles)?;
        sheets.push(SheetModel {
            id,
            name: sheet.name.clone(),
            cells,
            metadata,
        });
    }
    let (named_ranges, named_range_losses) = parse_named_ranges(&workbook_xml, &sheets)?;
    embedded_unsupported.extend(named_range_losses);

    let model = SpreadsheetModel {
        metadata: SpreadsheetMetadata {
            active_sheet_id: active_sheet_index
                .and_then(|index| sheets.get(index).map(|sheet| sheet.id.clone())),
            calculation_mode,
            date_system,
            named_ranges,
        },
        sheets,
    };
    ArtifactEnvelope::new("xlsx-adapter", ArtifactPayload::Spreadsheet(model.clone()))
        .validate()?;
    let mut ignored_parts = all_parts
        .into_iter()
        .filter(|part| !consumed.contains(part))
        .collect::<Vec<_>>();
    ignored_parts.sort();
    let mut unsupported_parts: Vec<XlsxUnsupportedPart> = ignored_parts
        .iter()
        .map(|part| XlsxUnsupportedPart {
            part: part.clone(),
            reason: unsupported_reason(part).to_string(),
        })
        .collect();
    unsupported_parts.extend(style_unsupported_parts);
    unsupported_parts.extend(workbook_unsupported_parts(&workbook_xml));
    unsupported_parts.extend(embedded_unsupported);
    unsupported_parts.sort_by(|left, right| {
        left.part
            .cmp(&right.part)
            .then(left.reason.cmp(&right.reason))
    });
    unsupported_parts.dedup();
    Ok(XlsxImportResult {
        model,
        ignored_parts,
        unsupported_parts,
    })
}

fn worksheet_unsupported_parts(part: &str, bytes: &[u8]) -> Vec<XlsxUnsupportedPart> {
    let xml = String::from_utf8_lossy(bytes);
    let checks = [
        ("<drawing", "worksheet drawings/images are not imported"),
        (
            "<legacyDrawing",
            "legacy drawings/comments are not imported",
        ),
        ("<tableParts", "structured table semantics are not modeled"),
        ("<hyperlinks", "worksheet hyperlinks are not modeled"),
        ("<sheetProtection", "worksheet protection is not modeled"),
        ("<extLst", "worksheet extension semantics are not modeled"),
        ("<top10", "top-10 filter predicates are not modeled"),
        (
            "<dynamicFilter",
            "dynamic filter predicates are not modeled",
        ),
        ("<colorFilter", "color filter predicates are not modeled"),
        ("<iconFilter", "icon filter predicates are not modeled"),
        (
            "t=\"array\"",
            "array-formula spill semantics are not modeled; only the anchor formula is retained",
        ),
    ];
    checks
        .into_iter()
        .filter(|(needle, _)| xml.contains(needle))
        .map(|(_, reason)| XlsxUnsupportedPart {
            part: part.to_string(),
            reason: reason.into(),
        })
        .collect()
}

pub fn inspect_xlsx(bytes: &[u8]) -> Result<XlsxLossReport, XlsxError> {
    let imported = read_xlsx_with_report(bytes)?;
    Ok(XlsxLossReport {
        unsupported: imported.unsupported_parts,
    })
}

/// Preflight canonical content that cannot currently be emitted losslessly.
pub fn inspect_xlsx_model(model: &SpreadsheetModel) -> XlsxLossReport {
    let mut unsupported = Vec::new();
    for sheet in &model.sheets {
        for media in &sheet.metadata.media {
            unsupported.push(XlsxUnsupportedPart {
                part: format!("sheet:{}#media:{}", sheet.id, media.id),
                reason:
                    "worksheet drawing/image relationships require the asset-backed XLSX writer"
                        .into(),
            });
        }
        for cell in &sheet.cells {
            if !cell.attrs.is_empty() {
                unsupported.push(XlsxUnsupportedPart {
                    part: format!("sheet:{}#cell:{}:{}", sheet.id, cell.row, cell.column),
                    reason: "opaque cell attrs have no OOXML mapping".into(),
                });
            }
            if let Some(style) = &cell.style {
                if let Some(reason) = xlsx_style_loss(style) {
                    unsupported.push(XlsxUnsupportedPart {
                        part: format!("sheet:{}#cellStyle:{}:{}", sheet.id, cell.row, cell.column),
                        reason,
                    });
                }
            }
        }
        for rule in &sheet.metadata.conditional_formats {
            if let oo_schema::ConditionalPredicate::CellIs { value, .. } = &rule.predicate {
                if matches!(value, Value::Null | Value::Array(_) | Value::Object(_)) {
                    unsupported.push(XlsxUnsupportedPart {
                        part: format!("sheet:{}#conditionalFormat:{}", sheet.id, rule.id),
                        reason: "cellIs requires a string, number, or boolean OOXML scalar".into(),
                    });
                }
            }
            if let oo_schema::ConditionalPredicate::ColorScale { min, max } = &rule.predicate {
                if !is_rgb_color(min) || !is_rgb_color(max) {
                    unsupported.push(XlsxUnsupportedPart {
                        part: format!("sheet:{}#conditionalFormat:{}", sheet.id, rule.id),
                        reason: "colorScale endpoints must be six-digit RGB values".into(),
                    });
                }
            }
            if let Some(reason) = xlsx_style_loss(&rule.style) {
                unsupported.push(XlsxUnsupportedPart {
                    part: format!("sheet:{}#conditionalStyle:{}", sheet.id, rule.id),
                    reason,
                });
            }
        }
    }
    XlsxLossReport { unsupported }
}

fn xlsx_style_loss(style: &CellStyle) -> Option<String> {
    let colors = style
        .font
        .as_ref()
        .and_then(|font| font.color.as_deref())
        .into_iter()
        .chain(
            style
                .fill
                .as_ref()
                .into_iter()
                .flat_map(|fill| [fill.foreground.as_deref(), fill.background.as_deref()])
                .flatten(),
        )
        .chain(
            style
                .borders
                .as_ref()
                .into_iter()
                .flat_map(|borders| [&borders.top, &borders.bottom, &borders.left, &borders.right])
                .filter_map(|edge| edge.as_ref().and_then(|edge| edge.color.as_deref())),
        );
    if colors.into_iter().any(|color| !is_rgb_color(color)) {
        return Some("typed XLSX colors must be six-digit RGB values".into());
    }
    if style
        .font
        .as_ref()
        .and_then(|font| font.size)
        .is_some_and(|size| !size.is_finite() || size <= 0.0 || size > 409.0)
    {
        return Some("XLSX font size must be finite and between 0 and 409 points".into());
    }
    if style
        .number_format
        .as_ref()
        .is_some_and(|format| format.is_empty())
    {
        return Some("XLSX number format cannot be empty".into());
    }
    if let Some(alignment) = &style.alignment {
        if alignment.horizontal.as_deref().is_some_and(|value| {
            !matches!(
                value,
                "general"
                    | "left"
                    | "center"
                    | "right"
                    | "fill"
                    | "justify"
                    | "centerContinuous"
                    | "distributed"
            )
        }) || alignment.vertical.as_deref().is_some_and(|value| {
            !matches!(
                value,
                "top" | "middle" | "center" | "bottom" | "justify" | "distributed"
            )
        }) {
            return Some(
                "typed XLSX alignment uses an unsupported horizontal/vertical value".into(),
            );
        }
    }
    None
}

fn is_rgb_color(color: &str) -> bool {
    let rgb = color.trim_start_matches('#');
    rgb.len() == 6 && rgb.chars().all(|character| character.is_ascii_hexdigit())
}

fn workbook_unsupported_parts(bytes: &[u8]) -> Vec<XlsxUnsupportedPart> {
    let xml = String::from_utf8_lossy(bytes);
    let mut unsupported = Vec::new();
    if xml.contains("externalReferences") {
        unsupported.push(XlsxUnsupportedPart {
            part: format!("{WORKBOOK}#externalReferences"),
            reason: "external workbook references are not modeled".into(),
        });
    }
    unsupported
}

fn parse_named_ranges(
    bytes: &[u8],
    sheets: &[SheetModel],
) -> Result<
    (
        Vec<oo_schema::SpreadsheetNamedRange>,
        Vec<XlsxUnsupportedPart>,
    ),
    XlsxError,
> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut current: Option<(String, Option<usize>, String)> = None;
    let mut ranges = Vec::new();
    let mut losses = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) if event.local_name().as_ref() == b"definedName" => {
                if let Some(name) = attr(&event, "name") {
                    current = Some((
                        name,
                        attr(&event, "localSheetId").and_then(|value| value.parse().ok()),
                        String::new(),
                    ));
                }
            }
            Event::Text(text) if current.is_some() => {
                current.as_mut().unwrap().2.push_str(&text.unescape()?)
            }
            Event::End(event) if event.local_name().as_ref() == b"definedName" => {
                let Some((name, local_index, target)) = current.take() else {
                    continue;
                };
                let parsed = target
                    .trim()
                    .trim_start_matches('=')
                    .rsplit_once('!')
                    .and_then(|(sheet_name, reference)| {
                        let sheet_name = sheet_name.trim_matches('\'').replace("''", "'");
                        let sheet = sheets.iter().find(|sheet| sheet.name == sheet_name)?;
                        let reference = reference.replace('$', "");
                        parse_range_ref(&reference)
                            .ok()
                            .flatten()
                            .map(|range| (sheet.id.clone(), range))
                    });
                let scope_sheet_id =
                    local_index.and_then(|index| sheets.get(index).map(|sheet| sheet.id.clone()));
                if let Some((sheet_id, range)) =
                    parsed.filter(|_| local_index.is_none() || scope_sheet_id.is_some())
                {
                    ranges.push(oo_schema::SpreadsheetNamedRange {
                        name,
                        scope_sheet_id,
                        sheet_id,
                        range,
                    });
                } else {
                    losses.push(XlsxUnsupportedPart {
                        part: format!("{WORKBOOK}#definedName:{name}"),
                        reason: format!(
                            "named formula or non-rectangular target is not modeled: {target}"
                        ),
                    });
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok((ranges, losses))
}

/// Export the supported SpreadsheetModel range as a valid minimal XLSX package.
pub fn write_xlsx(model: &SpreadsheetModel) -> Result<Vec<u8>, XlsxError> {
    ArtifactEnvelope::new("xlsx-adapter", ArtifactPayload::Spreadsheet(model.clone()))
        .validate()?;
    if let Some(first) = inspect_xlsx_model(model).unsupported.first() {
        return Err(XlsxError::UnsupportedFeature(format!(
            "{}: {}",
            first.part, first.reason
        )));
    }
    if model.sheets.is_empty() {
        return Err(XlsxError::UnsupportedFeature(
            "XLSX 至少需要一个 worksheet".into(),
        ));
    }
    for sheet in &model.sheets {
        if !sheet.metadata.media.is_empty() {
            return Err(XlsxError::UnsupportedFeature(format!(
                "sheet {} 包含当前 XLSX adapter 尚未可逆导出的媒体",
                sheet.id
            )));
        }
    }
    let mut shared = BTreeMap::<String, usize>::new();
    for sheet in &model.sheets {
        for cell in &sheet.cells {
            if cell.attrs.is_empty() {
                if let Some(Value::String(value)) = &cell.value {
                    if cell.formula.is_none() {
                        let next = shared.len();
                        shared.entry(value.clone()).or_insert(next);
                    }
                }
            } else {
                return Err(XlsxError::UnsupportedFeature(format!(
                    "sheet {} cell ({},{}) attrs",
                    sheet.id, cell.row, cell.column
                )));
            }
        }
    }
    let shared_lookup: HashMap<_, _> = shared
        .iter()
        .map(|(value, index)| (value.as_str(), *index))
        .collect();
    // Style zero must remain neutral for cells without an explicit style.
    let mut styles = vec![CellStyle::default()];
    for sheet in &model.sheets {
        for cell in &sheet.cells {
            if let Some(style) = &cell.style {
                if !styles.iter().any(|known| known == style) {
                    styles.push(style.clone());
                }
            }
        }
    }
    let style_lookup: HashMap<String, usize> = styles
        .iter()
        .enumerate()
        .map(|(index, style)| (serde_json::to_string(style).unwrap_or_default(), index))
        .collect();
    let mut differential_styles = Vec::<CellStyle>::new();
    for sheet in &model.sheets {
        for rule in &sheet.metadata.conditional_formats {
            if rule.style != CellStyle::default()
                && !differential_styles.iter().any(|known| known == &rule.style)
            {
                differential_styles.push(rule.style.clone());
            }
        }
    }
    let differential_style_lookup: HashMap<String, usize> = differential_styles
        .iter()
        .enumerate()
        .map(|(index, style)| (serde_json::to_string(style).unwrap_or_default(), index))
        .collect();
    let has_styles = !differential_styles.is_empty()
        || model
            .sheets
            .iter()
            .any(|sheet| sheet.cells.iter().any(|cell| cell.style.is_some()));

    let mut parts = vec![
        (
            CONTENT_TYPES.to_string(),
            content_types_xml(model.sheets.len(), !shared.is_empty(), has_styles),
        ),
        (ROOT_RELS.to_string(), root_relationships_xml()),
        (WORKBOOK.to_string(), workbook_xml(model)),
        (
            WORKBOOK_RELS.to_string(),
            workbook_relationships_xml(model.sheets.len(), !shared.is_empty(), has_styles),
        ),
    ];
    if !shared.is_empty() {
        parts.push((SHARED_STRINGS.to_string(), shared_strings_xml(&shared)));
    }
    if has_styles {
        parts.push((
            STYLES.to_string(),
            styles_xml(&styles, &differential_styles),
        ));
    }
    for (index, sheet) in model.sheets.iter().enumerate() {
        parts.push((
            format!("xl/worksheets/sheet{}.xml", index + 1),
            worksheet_xml(
                sheet,
                &shared_lookup,
                &style_lookup,
                &differential_style_lookup,
            )?,
        ));
    }

    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in parts {
        archive.start_file(name, SimpleFileOptions::default())?;
        archive.write_all(content.as_bytes())?;
    }
    Ok(archive.finish()?.into_inner())
}

/// Compare canonical spreadsheet interchange semantics rather than ZIP part
/// names, relationship ids, generated sheet ids, or generated rule ids.
pub fn semantic_diff(expected: &SpreadsheetModel, actual: &SpreadsheetModel) -> XlsxSemanticDiff {
    let expected = normalize_interchange_model(expected);
    let actual = normalize_interchange_model(actual);
    let left = serde_json::to_value(expected).unwrap_or(Value::Null);
    let right = serde_json::to_value(actual).unwrap_or(Value::Null);
    let mut differences = Vec::new();
    collect_json_differences("spreadsheet", &left, &right, &mut differences);
    XlsxSemanticDiff { differences }
}

fn normalize_interchange_model(model: &SpreadsheetModel) -> SpreadsheetModel {
    let mut normalized = model.clone();
    let sheet_ids: HashMap<String, String> = normalized
        .sheets
        .iter()
        .enumerate()
        .map(|(index, sheet)| (sheet.id.clone(), format!("sheet-{}", index + 1)))
        .collect();
    for (index, sheet) in normalized.sheets.iter_mut().enumerate() {
        sheet.id = format!("sheet-{}", index + 1);
        sheet.cells.sort_by_key(|cell| (cell.row, cell.column));
        for cell in &mut sheet.cells {
            if let Some(family) = cell
                .style
                .as_mut()
                .and_then(|style| style.font.as_mut())
                .and_then(|font| font.family.as_mut())
            {
                *family = oo_schema::font_family::primary_font_family(family);
            }
        }
        for (rule_index, rule) in sheet.metadata.conditional_formats.iter_mut().enumerate() {
            rule.id = format!("conditional-{rule_index}");
        }
        for (rule_index, rule) in sheet.metadata.data_validations.iter_mut().enumerate() {
            rule.id = format!("validation-{rule_index}");
        }
    }
    normalized.metadata.active_sheet_id = normalized
        .metadata
        .active_sheet_id
        .as_ref()
        .and_then(|id| sheet_ids.get(id).cloned());
    for named in &mut normalized.metadata.named_ranges {
        if let Some(id) = sheet_ids.get(&named.sheet_id) {
            named.sheet_id.clone_from(id);
        }
        if let Some(scope) = named.scope_sheet_id.as_mut() {
            if let Some(id) = sheet_ids.get(scope) {
                scope.clone_from(id);
            }
        }
    }
    normalized
}

fn collect_json_differences(path: &str, left: &Value, right: &Value, output: &mut Vec<String>) {
    match (left, right) {
        (Value::Object(left), Value::Object(right)) => {
            let keys = left.keys().chain(right.keys()).collect::<BTreeSet<_>>();
            for key in keys {
                collect_json_differences(
                    &format!("{path}.{key}"),
                    left.get(key).unwrap_or(&Value::Null),
                    right.get(key).unwrap_or(&Value::Null),
                    output,
                );
            }
        }
        (Value::Array(left), Value::Array(right)) => {
            for index in 0..left.len().max(right.len()) {
                collect_json_differences(
                    &format!("{path}[{index}]"),
                    left.get(index).unwrap_or(&Value::Null),
                    right.get(index).unwrap_or(&Value::Null),
                    output,
                );
            }
        }
        _ if left != right => output.push(format!("{path}: expected {left}, actual {right}")),
        _ => {}
    }
}

#[derive(Debug, Clone)]
struct WorkbookSheet {
    name: String,
    sheet_id: String,
    relationship_id: String,
    visibility: SheetVisibility,
}

#[derive(Debug, Clone)]
struct Relationship {
    target: String,
    external: bool,
}

fn require_part(parts: &[String], part: &'static str) -> Result<(), XlsxError> {
    parts
        .iter()
        .any(|candidate| candidate == part)
        .then_some(())
        .ok_or(XlsxError::MissingPart(part))
}

fn read_part<R: Read + std::io::Seek>(
    archive: &mut zip::ZipArchive<R>,
    name: &str,
) -> Result<Vec<u8>, XlsxError> {
    let mut file = archive.by_name(name)?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    Ok(bytes)
}

fn normalize_target(target: &str) -> Result<String, XlsxError> {
    let target = target.replace('\\', "/");
    let target = target.trim_start_matches('/');
    let target = if target.starts_with("xl/") {
        target.to_string()
    } else {
        format!("xl/{target}")
    };
    if target
        .split('/')
        .any(|segment| segment == ".." || segment.is_empty())
    {
        return Err(XlsxError::InvalidPart(format!(
            "关系目标路径 {target} 不安全"
        )));
    }
    Ok(target)
}

fn unsupported_reason(part: &str) -> &'static str {
    if part.starts_with("xl/media/") || part.starts_with("xl/drawings/") {
        "media/drawing semantics are not modeled"
    } else if part.starts_with("xl/charts/") {
        "chart semantics are not modeled"
    } else if part.starts_with("xl/externalLinks/") {
        "external workbook links are not modeled"
    } else if part.ends_with("vbaProject.bin") || part.contains("macrosheets/") {
        "VBA macros and macro sheets are not modeled and will never be executed"
    } else if part.starts_with("xl/pivot") || part.contains("pivotCache") {
        "pivot table semantics are not modeled"
    } else if part.contains("comments") || part.contains("threadedComment") {
        "comment semantics are not modeled"
    } else {
        "package part is not modeled"
    }
}

fn parse_workbook(bytes: &[u8]) -> Result<Vec<WorkbookSheet>, XlsxError> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut sheets = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Empty(event) if event.local_name().as_ref() == b"sheet" => {
                sheets.push(parse_workbook_sheet(&event, sheets.len())?);
            }
            Event::Start(event) if event.local_name().as_ref() == b"sheet" => {
                sheets.push(parse_workbook_sheet(&event, sheets.len())?);
                reader.read_to_end_into(event.name(), &mut Vec::new())?;
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    if sheets.is_empty() {
        return Err(XlsxError::InvalidPart("workbook 没有 worksheet".into()));
    }
    Ok(sheets)
}

fn parse_workbook_sheet(
    event: &BytesStart<'_>,
    position: usize,
) -> Result<WorkbookSheet, XlsxError> {
    let name =
        attr(event, "name").ok_or_else(|| XlsxError::InvalidPart("sheet 缺少 name".into()))?;
    let relationship_id = attr(event, "id")
        .ok_or_else(|| XlsxError::InvalidPart(format!("sheet {name} 缺少 r:id")))?;
    let sheet_id = attr(event, "sheetId").unwrap_or_else(|| (position + 1).to_string());
    let visibility = match attr(event, "state").as_deref() {
        Some("hidden") => SheetVisibility::Hidden,
        Some("veryHidden") => SheetVisibility::VeryHidden,
        _ => SheetVisibility::Visible,
    };
    Ok(WorkbookSheet {
        name,
        sheet_id,
        relationship_id,
        visibility,
    })
}

fn parse_workbook_settings(
    bytes: &[u8],
) -> Result<(DateSystem, oo_schema::CalculationMode, Option<usize>), XlsxError> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut date_system = DateSystem::Excel1900;
    let mut calculation_mode = oo_schema::CalculationMode::Automatic;
    let mut active_sheet_index = None;
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Empty(event) | Event::Start(event)
                if event.local_name().as_ref() == b"workbookPr" =>
            {
                if attr(&event, "date1904")
                    .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
                {
                    date_system = DateSystem::Excel1904;
                }
            }
            Event::Empty(event) | Event::Start(event)
                if event.local_name().as_ref() == b"calcPr" =>
            {
                if attr(&event, "calcMode").as_deref() == Some("manual") {
                    calculation_mode = oo_schema::CalculationMode::Manual;
                }
            }
            Event::Empty(event) | Event::Start(event)
                if event.local_name().as_ref() == b"workbookView" =>
            {
                active_sheet_index = attr(&event, "activeTab").and_then(|value| value.parse().ok());
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok((date_system, calculation_mode, active_sheet_index))
}

fn parse_relationships(bytes: &[u8]) -> Result<HashMap<String, Relationship>, XlsxError> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut relationships = HashMap::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Empty(event) | Event::Start(event)
                if event.local_name().as_ref() == b"Relationship" =>
            {
                let id = attr(&event, "Id")
                    .ok_or_else(|| XlsxError::InvalidPart("Relationship 缺少 Id".into()))?;
                let target = attr(&event, "Target")
                    .ok_or_else(|| XlsxError::InvalidPart(format!("关系 {id} 缺少 Target")))?;
                let external = attr(&event, "TargetMode")
                    .is_some_and(|mode| mode.eq_ignore_ascii_case("External"));
                if relationships
                    .insert(id.clone(), Relationship { target, external })
                    .is_some()
                {
                    return Err(XlsxError::InvalidPart(format!("Relationship {id} 重复")));
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(relationships)
}

fn parse_shared_strings(bytes: &[u8]) -> Result<Vec<String>, XlsxError> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut strings = Vec::new();
    let mut current = String::new();
    let mut in_si = false;
    let mut in_text = false;
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) if event.local_name().as_ref() == b"si" => {
                in_si = true;
                current.clear();
            }
            Event::Start(event) if event.local_name().as_ref() == b"t" && in_si => in_text = true,
            Event::Text(text) if in_text => {
                let text = text.unescape()?;
                current.push_str(&text);
            }
            Event::CData(text) if in_text => current.push_str(&String::from_utf8_lossy(&text)),
            Event::End(event) if event.local_name().as_ref() == b"t" => in_text = false,
            Event::End(event) if event.local_name().as_ref() == b"si" => {
                strings.push(current.clone());
                in_si = false;
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(strings)
}

/// Parse number formats and explicit font styles from `styles.xml`. Fills are
/// deliberately left to a later style capability; number formats are lossless and are carried as
/// a typed `CellStyle` rather than an opaque XML index.
fn parse_styles(bytes: &[u8]) -> Result<Vec<CellStyle>, XlsxError> {
    let custom_formats = parse_custom_formats(bytes)?;
    let fonts = parse_fonts(bytes)?;
    let fills = parse_fills(bytes)?;
    let borders = parse_borders(bytes)?;
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut in_cell_xfs = false;
    let mut current: Option<(u32, usize, usize, usize, Option<AlignmentStyle>)> = None;
    let mut styles = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) if event.local_name().as_ref() == b"cellXfs" => in_cell_xfs = true,
            Event::End(event) if event.local_name().as_ref() == b"cellXfs" => in_cell_xfs = false,
            Event::Empty(event) if in_cell_xfs && event.local_name().as_ref() == b"xf" => {
                styles.push(style_from_xf(
                    xf_ids(&event),
                    None,
                    &custom_formats,
                    &fonts,
                    &fills,
                    &borders,
                )?);
            }
            Event::Start(event) if in_cell_xfs && event.local_name().as_ref() == b"xf" => {
                let (num, font, fill, border) = xf_ids(&event);
                current = Some((num, font, fill, border, None));
            }
            Event::Empty(event) | Event::Start(event)
                if current.is_some() && event.local_name().as_ref() == b"alignment" =>
            {
                current.as_mut().unwrap().4 = Some(AlignmentStyle {
                    horizontal: attr(&event, "horizontal"),
                    vertical: attr(&event, "vertical").map(|value| {
                        if value == "center" {
                            "middle".into()
                        } else {
                            value
                        }
                    }),
                    wrap: truthy_attr(&event, "wrapText"),
                });
            }
            Event::End(event) if in_cell_xfs && event.local_name().as_ref() == b"xf" => {
                let (num, font, fill, border, alignment) = current
                    .take()
                    .ok_or_else(|| XlsxError::InvalidPart("cellXfs xf 结束但未开始".into()))?;
                styles.push(style_from_xf(
                    (num, font, fill, border),
                    alignment,
                    &custom_formats,
                    &fonts,
                    &fills,
                    &borders,
                )?);
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(styles)
}

/// Differential formats (`dxfs`) store their style values inline instead of
/// referring to the indexed font/fill/border tables used by ordinary cells.
fn parse_differential_styles(bytes: &[u8]) -> Result<Vec<CellStyle>, XlsxError> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut in_dxfs = false;
    let mut style: Option<CellStyle> = None;
    let mut font: Option<FontStyle> = None;
    let mut fill: Option<FillStyle> = None;
    let mut borders: Option<CellBorders> = None;
    let mut side: Option<(String, CellBorderEdge)> = None;
    let mut styles = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) if event.local_name().as_ref() == b"dxfs" => in_dxfs = true,
            Event::End(event) if event.local_name().as_ref() == b"dxfs" => in_dxfs = false,
            Event::Start(event) if in_dxfs && event.local_name().as_ref() == b"dxf" => {
                style = Some(CellStyle::default());
            }
            Event::Empty(event) if in_dxfs && event.local_name().as_ref() == b"dxf" => {
                styles.push(CellStyle::default());
            }
            Event::End(event) if in_dxfs && event.local_name().as_ref() == b"dxf" => {
                styles.push(
                    style
                        .take()
                        .ok_or_else(|| XlsxError::InvalidPart("dxf 结束但未开始".into()))?,
                );
            }
            Event::Start(event) if style.is_some() && event.local_name().as_ref() == b"font" => {
                font = Some(FontStyle::default());
            }
            Event::End(event) if font.is_some() && event.local_name().as_ref() == b"font" => {
                let value = font.take().unwrap();
                style.as_mut().unwrap().font = (value != FontStyle::default()).then_some(value);
            }
            Event::Start(event) if style.is_some() && event.local_name().as_ref() == b"fill" => {
                fill = Some(FillStyle::default());
            }
            Event::End(event) if fill.is_some() && event.local_name().as_ref() == b"fill" => {
                let value = fill.take().unwrap();
                style.as_mut().unwrap().fill = (value != FillStyle::default()).then_some(value);
            }
            Event::Start(event) if style.is_some() && event.local_name().as_ref() == b"border" => {
                borders = Some(CellBorders::default());
            }
            Event::End(event) if borders.is_some() && event.local_name().as_ref() == b"border" => {
                let value = borders.take().unwrap();
                style.as_mut().unwrap().borders =
                    (value != CellBorders::default()).then_some(value);
            }
            Event::Start(event)
                if borders.is_some()
                    && matches!(
                        event.local_name().as_ref(),
                        b"left" | b"right" | b"top" | b"bottom"
                    ) =>
            {
                side = Some((
                    String::from_utf8_lossy(event.local_name().as_ref()).into_owned(),
                    CellBorderEdge {
                        style: attr(&event, "style"),
                        color: None,
                    },
                ));
            }
            Event::Empty(event)
                if borders.is_some()
                    && matches!(
                        event.local_name().as_ref(),
                        b"left" | b"right" | b"top" | b"bottom"
                    ) =>
            {
                let name = String::from_utf8_lossy(event.local_name().as_ref()).into_owned();
                assign_border_edge(
                    borders.as_mut().unwrap(),
                    &name,
                    CellBorderEdge {
                        style: attr(&event, "style"),
                        color: None,
                    },
                );
            }
            Event::Empty(event) if side.is_some() && event.local_name().as_ref() == b"color" => {
                side.as_mut().unwrap().1.color = rgb_color(&event);
            }
            Event::End(event)
                if side
                    .as_ref()
                    .is_some_and(|(name, _)| name.as_bytes() == event.local_name().as_ref()) =>
            {
                let (name, edge) = side.take().unwrap();
                assign_border_edge(borders.as_mut().unwrap(), &name, edge);
            }
            Event::Empty(event) | Event::Start(event) if font.is_some() => {
                let target = font.as_mut().unwrap();
                let enabled =
                    !matches!(attr(&event, "val").as_deref(), Some("0" | "false" | "none"));
                match event.local_name().as_ref() {
                    b"name" => target.family = attr(&event, "val"),
                    b"sz" => target.size = attr(&event, "val").and_then(|v| v.parse().ok()),
                    b"b" => target.bold = enabled,
                    b"i" => target.italic = enabled,
                    b"u" => target.underline = enabled,
                    b"strike" => target.strikethrough = enabled,
                    b"color" => target.color = rgb_color(&event),
                    _ => {}
                }
            }
            Event::Empty(event) if fill.is_some() && event.local_name().as_ref() == b"fgColor" => {
                fill.as_mut().unwrap().foreground = rgb_color(&event);
            }
            Event::Empty(event) if fill.is_some() && event.local_name().as_ref() == b"bgColor" => {
                fill.as_mut().unwrap().background = rgb_color(&event);
            }
            Event::Empty(event) | Event::Start(event)
                if style.is_some() && event.local_name().as_ref() == b"alignment" =>
            {
                let alignment = AlignmentStyle {
                    horizontal: attr(&event, "horizontal"),
                    vertical: attr(&event, "vertical").map(|value| {
                        if value == "center" {
                            "middle".into()
                        } else {
                            value
                        }
                    }),
                    wrap: truthy_attr(&event, "wrapText"),
                };
                style.as_mut().unwrap().alignment =
                    (alignment != AlignmentStyle::default()).then_some(alignment);
            }
            Event::Empty(event) | Event::Start(event)
                if style.is_some() && event.local_name().as_ref() == b"numFmt" =>
            {
                style.as_mut().unwrap().number_format = attr(&event, "formatCode");
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(styles)
}

fn assign_border_edge(borders: &mut CellBorders, name: &str, edge: CellBorderEdge) {
    if edge.style.is_none() && edge.color.is_none() {
        return;
    }
    match name {
        "left" => borders.left = Some(edge),
        "right" => borders.right = Some(edge),
        "top" => borders.top = Some(edge),
        "bottom" => borders.bottom = Some(edge),
        _ => {}
    }
}

fn parse_custom_formats(bytes: &[u8]) -> Result<BTreeMap<u32, String>, XlsxError> {
    let mut reader = Reader::from_reader(bytes);
    let mut buffer = Vec::new();
    let mut formats = BTreeMap::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Empty(event) | Event::Start(event)
                if event.local_name().as_ref() == b"numFmt" =>
            {
                if let (Some(id), Some(code)) = (
                    attr(&event, "numFmtId").and_then(|v| v.parse().ok()),
                    attr(&event, "formatCode"),
                ) {
                    formats.insert(id, code);
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(formats)
}

fn parse_fonts(bytes: &[u8]) -> Result<Vec<FontStyle>, XlsxError> {
    let mut reader = Reader::from_reader(bytes);
    let mut buffer = Vec::new();
    let mut in_fonts = false;
    let mut current = None;
    let mut fonts = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) if event.local_name().as_ref() == b"fonts" => in_fonts = true,
            Event::End(event) if event.local_name().as_ref() == b"fonts" => in_fonts = false,
            Event::Start(event) if in_fonts && event.local_name().as_ref() == b"font" => {
                current = Some(FontStyle::default())
            }
            Event::Empty(event) if in_fonts && event.local_name().as_ref() == b"font" => {
                fonts.push(FontStyle::default())
            }
            Event::End(event) if in_fonts && event.local_name().as_ref() == b"font" => {
                if let Some(font) = current.take() {
                    fonts.push(font);
                }
            }
            Event::Empty(event) | Event::Start(event) if current.is_some() => {
                let font = current.as_mut().unwrap();
                let enabled =
                    !matches!(attr(&event, "val").as_deref(), Some("0" | "false" | "none"));
                match event.local_name().as_ref() {
                    b"name" => font.family = attr(&event, "val"),
                    b"sz" => font.size = attr(&event, "val").and_then(|v| v.parse().ok()),
                    b"b" => font.bold = enabled,
                    b"i" => font.italic = enabled,
                    b"u" => font.underline = enabled,
                    b"strike" => font.strikethrough = enabled,
                    b"color" => font.color = rgb_color(&event),
                    _ => {}
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(fonts)
}

fn parse_fills(bytes: &[u8]) -> Result<Vec<FillStyle>, XlsxError> {
    let mut reader = Reader::from_reader(bytes);
    let mut buffer = Vec::new();
    let mut in_fills = false;
    let mut current = None;
    let mut fills = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) if event.local_name().as_ref() == b"fills" => in_fills = true,
            Event::End(event) if event.local_name().as_ref() == b"fills" => in_fills = false,
            Event::Start(event) if in_fills && event.local_name().as_ref() == b"fill" => {
                current = Some(FillStyle::default())
            }
            Event::End(event) if in_fills && event.local_name().as_ref() == b"fill" => {
                if let Some(fill) = current.take() {
                    fills.push(fill);
                }
            }
            Event::Empty(event)
                if current.is_some() && event.local_name().as_ref() == b"fgColor" =>
            {
                current.as_mut().unwrap().foreground = rgb_color(&event)
            }
            Event::Empty(event)
                if current.is_some() && event.local_name().as_ref() == b"bgColor" =>
            {
                current.as_mut().unwrap().background = rgb_color(&event)
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(fills)
}

fn parse_borders(bytes: &[u8]) -> Result<Vec<CellBorders>, XlsxError> {
    let mut reader = Reader::from_reader(bytes);
    let mut buffer = Vec::new();
    let mut in_borders = false;
    let mut current = None;
    let mut side: Option<(String, CellBorderEdge)> = None;
    let mut borders = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) if event.local_name().as_ref() == b"borders" => in_borders = true,
            Event::End(event) if event.local_name().as_ref() == b"borders" => in_borders = false,
            Event::Start(event) if in_borders && event.local_name().as_ref() == b"border" => {
                current = Some(CellBorders::default())
            }
            Event::Empty(event) if in_borders && event.local_name().as_ref() == b"border" => {
                borders.push(CellBorders::default())
            }
            Event::End(event) if in_borders && event.local_name().as_ref() == b"border" => {
                if let Some(border) = current.take() {
                    borders.push(border);
                }
            }
            Event::Start(event)
                if current.is_some()
                    && matches!(
                        event.local_name().as_ref(),
                        b"left" | b"right" | b"top" | b"bottom"
                    ) =>
            {
                side = Some((
                    String::from_utf8_lossy(event.local_name().as_ref()).into_owned(),
                    CellBorderEdge {
                        style: attr(&event, "style"),
                        color: None,
                    },
                ))
            }
            Event::Empty(event) if side.is_some() && event.local_name().as_ref() == b"color" => {
                side.as_mut().unwrap().1.color = rgb_color(&event)
            }
            Event::End(event)
                if side
                    .as_ref()
                    .is_some_and(|(name, _)| name.as_bytes() == event.local_name().as_ref()) =>
            {
                let (name, edge) = side.take().unwrap();
                if edge.style.is_some() || edge.color.is_some() {
                    let border = current.as_mut().unwrap();
                    match name.as_str() {
                        "left" => border.left = Some(edge),
                        "right" => border.right = Some(edge),
                        "top" => border.top = Some(edge),
                        "bottom" => border.bottom = Some(edge),
                        _ => {}
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(borders)
}

fn rgb_color(event: &BytesStart<'_>) -> Option<String> {
    attr(event, "rgb")
        .filter(|rgb| {
            (rgb.len() == 6 || rgb.len() == 8) && rgb.chars().all(|c| c.is_ascii_hexdigit())
        })
        .map(|rgb| format!("#{}", &rgb[rgb.len() - 6..]))
}

fn xf_ids(event: &BytesStart<'_>) -> (u32, usize, usize, usize) {
    (
        attr(event, "numFmtId")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        attr(event, "fontId")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        attr(event, "fillId")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
        attr(event, "borderId")
            .and_then(|v| v.parse().ok())
            .unwrap_or(0),
    )
}

fn style_from_xf(
    ids: (u32, usize, usize, usize),
    alignment: Option<AlignmentStyle>,
    custom: &BTreeMap<u32, String>,
    fonts: &[FontStyle],
    fills: &[FillStyle],
    borders: &[CellBorders],
) -> Result<CellStyle, XlsxError> {
    let (num, font, fill, border) = ids;
    let font_value = fonts
        .get(font)
        .cloned()
        .or_else(|| (font == 0).then(FontStyle::default))
        .ok_or_else(|| XlsxError::InvalidPart(format!("cellXf fontId {font} 越界")))?;
    let fill_value = fills
        .get(fill)
        .cloned()
        .or_else(|| (fill == 0).then(FillStyle::default))
        .ok_or_else(|| XlsxError::InvalidPart(format!("cellXf fillId {fill} 越界")))?;
    let border_value = borders
        .get(border)
        .cloned()
        .or_else(|| (border == 0).then(CellBorders::default))
        .ok_or_else(|| XlsxError::InvalidPart(format!("cellXf borderId {border} 越界")))?;
    let font = (font_value != FontStyle::default()).then_some(font_value);
    let fill = (fill_value != FillStyle::default()).then_some(fill_value);
    let borders = (border_value != CellBorders::default()).then_some(border_value);
    Ok(CellStyle {
        number_format: Some(
            custom
                .get(&num)
                .cloned()
                .unwrap_or_else(|| builtin_num_format(num).to_string()),
        )
        .filter(|v| !v.is_empty() && v != "General"),
        font,
        fill,
        alignment: alignment.filter(|v| *v != AlignmentStyle::default()),
        borders,
    })
}

fn style_unsupported_parts(bytes: &[u8]) -> Vec<XlsxUnsupportedPart> {
    let xml = String::from_utf8_lossy(bytes);
    let mut parts = Vec::new();
    if xml.contains("<fgColor theme=")
        || xml.contains("<bgColor theme=")
        || xml.contains("<fgColor indexed=")
        || xml.contains("<bgColor indexed=")
        || xml.contains("patternType=\"dark")
        || xml.contains("patternType=\"light")
    {
        parts.push(XlsxUnsupportedPart {
            part: format!("{STYLES}#fills"),
            reason: "theme/indexed colors and non-solid fill patterns are not modeled".into(),
        });
    }
    if xml.contains("<color theme=") || xml.contains("<color indexed=") {
        parts.push(XlsxUnsupportedPart {
            part: format!("{STYLES}#colors"),
            reason: "theme/indexed style colors are not modeled".into(),
        });
    }
    // Explicit RGB and boolean text decorations are modeled; theme/indexed colors and
    // advanced font effects remain visible in the import loss report.
    let mut reader = Reader::from_reader(bytes);
    let mut buffer = Vec::new();
    let mut in_fonts = false;
    let mut unsupported = false;
    loop {
        match reader.read_event_into(&mut buffer) {
            Ok(Event::Start(event)) if event.local_name().as_ref() == b"fonts" => in_fonts = true,
            Ok(Event::End(event)) if event.local_name().as_ref() == b"fonts" => in_fonts = false,
            Ok(Event::Empty(event) | Event::Start(event)) if in_fonts => {
                unsupported |= match event.local_name().as_ref() {
                    b"color" => attr(&event, "rgb").is_none() || attr(&event, "tint").is_some(),
                    b"u" => attr(&event, "val").is_some_and(|value| {
                        !matches!(
                            value.as_str(),
                            "single" | "none" | "0" | "false" | "1" | "true"
                        )
                    }),
                    b"font" | b"name" | b"sz" | b"b" | b"i" | b"strike" | b"family"
                    | b"charset" => false,
                    _ => true,
                };
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buffer.clear();
    }
    if unsupported {
        parts.push(XlsxUnsupportedPart {
            part: format!("{STYLES}#fonts"),
            reason: "theme/indexed font colors or advanced font effects are not modeled".into(),
        });
    }
    parts
}

fn builtin_num_format(id: u32) -> &'static str {
    match id {
        0 => "General",
        1 => "0",
        2 => "0.00",
        9 => "0%",
        10 => "0.00%",
        14 => "mm-dd-yy",
        20 => "h:mm",
        21 => "h:mm:ss",
        22 => "m/d/yy h:mm",
        _ => "",
    }
}

fn parse_worksheet_metadata(
    bytes: &[u8],
    differential_styles: &[CellStyle],
) -> Result<SheetMetadata, XlsxError> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut metadata = SheetMetadata::default();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Empty(event) | Event::Start(event) if event.local_name().as_ref() == b"row" => {
                let height = attr(&event, "ht").and_then(|value| value.parse::<f64>().ok());
                let hidden =
                    attr(&event, "hidden").is_some_and(|value| value == "1" || value == "true");
                if height.is_some() || hidden {
                    let row = attr(&event, "r")
                        .and_then(|value| value.parse::<u32>().ok())
                        .and_then(|value| value.checked_sub(1))
                        .ok_or_else(|| XlsxError::InvalidPart("行布局缺少有效行号".into()))?;
                    metadata.row_layout.push(oo_schema::SheetRowLayout {
                        row,
                        height,
                        hidden,
                    });
                }
            }
            Event::Empty(event) | Event::Start(event)
                if event.local_name().as_ref() == b"dimension" =>
            {
                if let Some(reference) = attr(&event, "ref") {
                    if let Some(range) = parse_range_ref(&reference)? {
                        metadata.row_count = range.end_row.checked_add(1);
                        metadata.column_count = range.end_column.checked_add(1);
                    }
                }
            }
            Event::Empty(event) | Event::Start(event) if event.local_name().as_ref() == b"pane" => {
                metadata.freeze = FreezePane {
                    rows: attr(&event, "ySplit")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0),
                    columns: attr(&event, "xSplit")
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0),
                };
            }
            Event::Empty(event) | Event::Start(event)
                if event.local_name().as_ref() == b"autoFilter" =>
            {
                if let Some(reference) = attr(&event, "ref") {
                    if let Some(range) = parse_range_ref(&reference)? {
                        metadata.auto_filter = Some(oo_schema::FilterSpec {
                            range,
                            columns: Vec::new(),
                        });
                    }
                }
            }
            Event::Empty(event) if event.local_name().as_ref() == b"mergeCell" => {
                if let Some(reference) = attr(&event, "ref") {
                    if let Some(range) = parse_range_ref(&reference)? {
                        metadata.merged_ranges.push(range);
                    }
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    metadata.auto_filter = parse_filter_spec(bytes)?.or(metadata.auto_filter);
    metadata.sort = parse_sort_spec(bytes)?;
    metadata.data_validations = parse_data_validations(bytes)?;
    metadata.conditional_formats = parse_conditional_formats(bytes, differential_styles)?;
    Ok(metadata)
}

fn parse_filter_spec(bytes: &[u8]) -> Result<Option<oo_schema::FilterSpec>, XlsxError> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut spec: Option<oo_schema::FilterSpec> = None;
    let mut column = None;
    let mut values = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"autoFilter" =>
            {
                if let Some(reference) = attr(&event, "ref") {
                    spec = parse_range_ref(&reference)?.map(|range| oo_schema::FilterSpec {
                        range,
                        columns: Vec::new(),
                    });
                }
            }
            Event::Start(event) if event.local_name().as_ref() == b"filterColumn" => {
                let relative = attr(&event, "colId")
                    .and_then(|value| value.parse::<u32>().ok())
                    .ok_or_else(|| XlsxError::InvalidPart("filterColumn 缺少 colId".into()))?;
                column = spec
                    .as_ref()
                    .map(|filter| filter.range.start_column + relative);
                values.clear();
            }
            Event::Empty(event) if event.local_name().as_ref() == b"filter" && column.is_some() => {
                if let Some(value) = attr(&event, "val") {
                    values.push(parse_scalar(&value));
                }
            }
            Event::Empty(event)
                if event.local_name().as_ref() == b"customFilter" && column.is_some() =>
            {
                let raw = attr(&event, "val").unwrap_or_default();
                let predicate = match attr(&event, "operator").as_deref().unwrap_or("equal") {
                    "greaterThan" => {
                        oo_schema::FilterPredicate::GreaterThan(raw.parse().map_err(|_| {
                            XlsxError::InvalidPart("customFilter greaterThan 不是数字".into())
                        })?)
                    }
                    "lessThan" => {
                        oo_schema::FilterPredicate::LessThan(raw.parse().map_err(|_| {
                            XlsxError::InvalidPart("customFilter lessThan 不是数字".into())
                        })?)
                    }
                    "equal" if raw.starts_with('*') && raw.ends_with('*') && raw.len() >= 2 => {
                        oo_schema::FilterPredicate::Contains(raw[1..raw.len() - 1].to_string())
                    }
                    "equal" => oo_schema::FilterPredicate::Equals(parse_scalar(&raw)),
                    operator => {
                        return Err(XlsxError::UnsupportedFeature(format!(
                            "filter operator {operator}"
                        )))
                    }
                };
                if let (Some(filter), Some(column)) = (spec.as_mut(), column) {
                    filter
                        .columns
                        .push(oo_schema::FilterColumn { column, predicate });
                }
            }
            Event::End(event) if event.local_name().as_ref() == b"filterColumn" => {
                if !values.is_empty() {
                    if let (Some(filter), Some(column)) = (spec.as_mut(), column) {
                        filter.columns.push(oo_schema::FilterColumn {
                            column,
                            predicate: oo_schema::FilterPredicate::Values(std::mem::take(
                                &mut values,
                            )),
                        });
                    }
                }
                column = None;
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(spec)
}

fn parse_sort_spec(bytes: &[u8]) -> Result<Option<oo_schema::SortSpec>, XlsxError> {
    let mut reader = Reader::from_reader(bytes);
    let mut buffer = Vec::new();
    let mut sort = None;
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) | Event::Empty(event)
                if event.local_name().as_ref() == b"sortState" =>
            {
                if let Some(reference) = attr(&event, "ref") {
                    sort = parse_range_ref(&reference)?.map(|range| oo_schema::SortSpec {
                        range,
                        keys: Vec::new(),
                    });
                }
            }
            Event::Empty(event) if event.local_name().as_ref() == b"sortCondition" => {
                if let (Some(sort), Some(reference)) = (sort.as_mut(), attr(&event, "ref")) {
                    let range = parse_range_ref(&reference)?
                        .ok_or_else(|| XlsxError::InvalidPart("sortCondition ref 为空".into()))?;
                    sort.keys.push(oo_schema::SortKey {
                        column: range.start_column,
                        direction: if truthy_attr(&event, "descending") {
                            oo_schema::SortDirection::Descending
                        } else {
                            oo_schema::SortDirection::Ascending
                        },
                    });
                }
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(sort)
}

#[derive(Default)]
struct ValidationDraft {
    kind: String,
    range: Option<GridRange>,
    allow_blank: bool,
    error: Option<String>,
    first: String,
    second: String,
    field: u8,
}

fn parse_data_validations(bytes: &[u8]) -> Result<Vec<oo_schema::DataValidationRule>, XlsxError> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut draft: Option<ValidationDraft> = None;
    let mut rules = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) if event.local_name().as_ref() == b"dataValidation" => {
                let sqref = attr(&event, "sqref")
                    .ok_or_else(|| XlsxError::InvalidPart("dataValidation 缺少 sqref".into()))?;
                if sqref.split_whitespace().count() != 1 {
                    return Err(XlsxError::UnsupportedFeature(
                        "dataValidation 多区域 sqref".into(),
                    ));
                }
                draft = Some(ValidationDraft {
                    kind: attr(&event, "type").unwrap_or_else(|| "none".into()),
                    range: parse_range_ref(&sqref)?,
                    allow_blank: truthy_attr(&event, "allowBlank"),
                    error: attr(&event, "error"),
                    ..ValidationDraft::default()
                });
            }
            Event::Start(event) if event.local_name().as_ref() == b"formula1" => {
                if let Some(draft) = draft.as_mut() {
                    draft.field = 1;
                }
            }
            Event::Start(event) if event.local_name().as_ref() == b"formula2" => {
                if let Some(draft) = draft.as_mut() {
                    draft.field = 2;
                }
            }
            Event::Text(text) if draft.as_ref().is_some_and(|draft| draft.field > 0) => {
                let text = text.unescape()?.into_owned();
                let draft = draft.as_mut().unwrap();
                if draft.field == 1 {
                    draft.first.push_str(&text);
                } else {
                    draft.second.push_str(&text);
                }
            }
            Event::End(event)
                if event.local_name().as_ref() == b"formula1"
                    || event.local_name().as_ref() == b"formula2" =>
            {
                if let Some(draft) = draft.as_mut() {
                    draft.field = 0;
                }
            }
            Event::End(event) if event.local_name().as_ref() == b"dataValidation" => {
                let draft = draft.take().unwrap();
                let range = draft
                    .range
                    .ok_or_else(|| XlsxError::InvalidPart("dataValidation sqref 为空".into()))?;
                let kind = match draft.kind.as_str() {
                    "list" => oo_schema::DataValidationKind::List(
                        draft
                            .first
                            .trim_matches('"')
                            .replace("\"\"", "\"")
                            .split(',')
                            .map(str::to_string)
                            .collect(),
                    ),
                    "whole" => oo_schema::DataValidationKind::WholeNumber {
                        min: draft.first.parse().map_err(|_| {
                            XlsxError::InvalidPart("whole validation formula1".into())
                        })?,
                        max: draft.second.parse().map_err(|_| {
                            XlsxError::InvalidPart("whole validation formula2".into())
                        })?,
                    },
                    "decimal" => oo_schema::DataValidationKind::Decimal {
                        min: draft.first.parse().map_err(|_| {
                            XlsxError::InvalidPart("decimal validation formula1".into())
                        })?,
                        max: draft.second.parse().map_err(|_| {
                            XlsxError::InvalidPart("decimal validation formula2".into())
                        })?,
                    },
                    "date" => oo_schema::DataValidationKind::Date {
                        min_serial: draft.first.parse().map_err(|_| {
                            XlsxError::InvalidPart("date validation formula1".into())
                        })?,
                        max_serial: draft.second.parse().map_err(|_| {
                            XlsxError::InvalidPart("date validation formula2".into())
                        })?,
                    },
                    "custom" => {
                        oo_schema::DataValidationKind::CustomFormula(format!("={}", draft.first))
                    }
                    other => {
                        return Err(XlsxError::UnsupportedFeature(format!(
                            "data validation type {other}"
                        )))
                    }
                };
                rules.push(oo_schema::DataValidationRule {
                    id: format!("xlsx-validation-{}", rules.len() + 1),
                    range,
                    kind,
                    allow_blank: draft.allow_blank,
                    error_message: draft.error,
                });
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(rules)
}

#[derive(Default)]
struct ConditionalDraft {
    range: Option<GridRange>,
    kind: String,
    operator: Option<String>,
    formula: String,
    in_formula: bool,
    colors: Vec<String>,
    differential_style: Option<usize>,
}

fn parse_conditional_formats(
    bytes: &[u8],
    differential_styles: &[CellStyle],
) -> Result<Vec<oo_schema::ConditionalFormatRule>, XlsxError> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut current_range = None;
    let mut draft: Option<ConditionalDraft> = None;
    let mut rules = Vec::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) if event.local_name().as_ref() == b"conditionalFormatting" => {
                current_range =
                    attr(&event, "sqref").and_then(|value| parse_range_ref(&value).ok().flatten());
            }
            Event::Start(event) if event.local_name().as_ref() == b"cfRule" => {
                draft = Some(ConditionalDraft {
                    range: current_range,
                    kind: attr(&event, "type").unwrap_or_default(),
                    operator: attr(&event, "operator"),
                    differential_style: attr(&event, "dxfId").and_then(|value| value.parse().ok()),
                    ..ConditionalDraft::default()
                });
            }
            Event::Start(event) if event.local_name().as_ref() == b"formula" => {
                if let Some(draft) = draft.as_mut() {
                    draft.in_formula = true;
                }
            }
            Event::Text(text) if draft.as_ref().is_some_and(|draft| draft.in_formula) => {
                draft.as_mut().unwrap().formula.push_str(&text.unescape()?);
            }
            Event::End(event) if event.local_name().as_ref() == b"formula" => {
                if let Some(draft) = draft.as_mut() {
                    draft.in_formula = false;
                }
            }
            Event::Empty(event) if event.local_name().as_ref() == b"color" && draft.is_some() => {
                if let Some(rgb) = attr(&event, "rgb") {
                    draft
                        .as_mut()
                        .unwrap()
                        .colors
                        .push(format!("#{}", &rgb[rgb.len().saturating_sub(6)..]));
                }
            }
            Event::End(event) if event.local_name().as_ref() == b"cfRule" => {
                let draft = draft.take().unwrap();
                let range = draft.range.ok_or_else(|| {
                    XlsxError::InvalidPart("conditionalFormatting sqref 无效".into())
                })?;
                let predicate = match draft.kind.as_str() {
                    "cellIs" => oo_schema::ConditionalPredicate::CellIs {
                        operator: parse_comparison(draft.operator.as_deref())?,
                        value: parse_scalar(&draft.formula),
                    },
                    "expression" => {
                        oo_schema::ConditionalPredicate::Formula(format!("={}", draft.formula))
                    }
                    "colorScale" if draft.colors.len() >= 2 => {
                        oo_schema::ConditionalPredicate::ColorScale {
                            min: draft.colors[0].clone(),
                            max: draft.colors[1].clone(),
                        }
                    }
                    other => {
                        return Err(XlsxError::UnsupportedFeature(format!(
                            "conditional format type {other}"
                        )))
                    }
                };
                rules.push(oo_schema::ConditionalFormatRule {
                    id: format!("xlsx-cf-{}", rules.len() + 1),
                    range,
                    predicate,
                    style: draft
                        .differential_style
                        .map(|index| {
                            differential_styles.get(index).cloned().ok_or_else(|| {
                                XlsxError::InvalidPart(format!("cfRule dxfId {index} 越界"))
                            })
                        })
                        .transpose()?
                        .unwrap_or_default(),
                });
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(rules)
}

fn parse_comparison(value: Option<&str>) -> Result<oo_schema::ComparisonOperator, XlsxError> {
    Ok(match value.unwrap_or("equal") {
        "equal" => oo_schema::ComparisonOperator::Equal,
        "notEqual" => oo_schema::ComparisonOperator::NotEqual,
        "greaterThan" => oo_schema::ComparisonOperator::GreaterThan,
        "greaterThanOrEqual" => oo_schema::ComparisonOperator::GreaterThanOrEqual,
        "lessThan" => oo_schema::ComparisonOperator::LessThan,
        "lessThanOrEqual" => oo_schema::ComparisonOperator::LessThanOrEqual,
        other => {
            return Err(XlsxError::UnsupportedFeature(format!(
                "comparison operator {other}"
            )))
        }
    })
}

fn parse_scalar(value: &str) -> Value {
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        Value::String(value[1..value.len() - 1].replace("\"\"", "\""))
    } else if value.eq_ignore_ascii_case("true") {
        Value::Bool(true)
    } else if value.eq_ignore_ascii_case("false") {
        Value::Bool(false)
    } else {
        parse_number(value.to_string()).unwrap_or_else(|| Value::String(value.to_string()))
    }
}

fn truthy_attr(event: &BytesStart<'_>, name: &str) -> bool {
    attr(event, name).is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
}

fn parse_range_ref(reference: &str) -> Result<Option<GridRange>, XlsxError> {
    let mut parts = reference.split(':');
    let start = parts.next().unwrap_or_default();
    let end = parts.next().unwrap_or(start);
    if parts.next().is_some() {
        return Err(XlsxError::InvalidPart(format!("范围引用无效：{reference}")));
    }
    if start.is_empty() || end.is_empty() {
        return Ok(None);
    }
    let (start_row, start_column) = parse_a1(start)?;
    let (end_row, end_column) = parse_a1(end)?;
    Ok(Some(GridRange {
        start_row: start_row.min(end_row),
        start_column: start_column.min(end_column),
        end_row: start_row.max(end_row),
        end_column: start_column.max(end_column),
    }))
}

#[derive(Debug, Default)]
struct RawCell {
    reference: Option<String>,
    cell_type: Option<String>,
    style_index: Option<usize>,
    formula_type: Option<String>,
    formula_index: Option<u32>,
    formula: Option<String>,
    value: Option<String>,
    inline_value: String,
}

#[derive(Clone, Copy)]
enum CellTextField {
    Formula,
    Value,
    Inline,
}

fn parse_worksheet(
    bytes: &[u8],
    shared_strings: &[String],
    styles: &[CellStyle],
) -> Result<Vec<CellModel>, XlsxError> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(false);
    let mut buffer = Vec::new();
    let mut cells = Vec::new();
    let mut current: Option<RawCell> = None;
    let mut text_field: Option<CellTextField> = None;
    let mut in_inline = false;
    let mut shared_formulas = HashMap::<u32, ((u32, u32), String)>::new();
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) if event.local_name().as_ref() == b"c" => {
                if current.is_some() {
                    return Err(XlsxError::InvalidPart("worksheet 嵌套 cell".into()));
                }
                current = Some(RawCell {
                    reference: attr(&event, "r"),
                    cell_type: attr(&event, "t"),
                    style_index: attr(&event, "s").and_then(|value| value.parse().ok()),
                    ..RawCell::default()
                });
            }
            Event::Empty(event) if event.local_name().as_ref() == b"c" => {
                cells.push(raw_cell_to_model(
                    RawCell {
                        reference: attr(&event, "r"),
                        cell_type: attr(&event, "t"),
                        style_index: attr(&event, "s").and_then(|value| value.parse().ok()),
                        ..RawCell::default()
                    },
                    shared_strings,
                    styles,
                )?);
            }
            Event::Start(event) if event.local_name().as_ref() == b"f" => {
                if let Some(cell) = current.as_mut() {
                    cell.formula_type = attr(&event, "t");
                    cell.formula_index = attr(&event, "si").and_then(|value| value.parse().ok());
                }
                text_field = Some(CellTextField::Formula)
            }
            Event::Empty(event) if event.local_name().as_ref() == b"f" => {
                if let Some(cell) = current.as_mut() {
                    cell.formula_type = attr(&event, "t");
                    cell.formula_index = attr(&event, "si").and_then(|value| value.parse().ok());
                }
            }
            Event::Start(event) if event.local_name().as_ref() == b"v" => {
                text_field = Some(CellTextField::Value)
            }
            Event::Start(event) if event.local_name().as_ref() == b"is" => in_inline = true,
            Event::Start(event) if event.local_name().as_ref() == b"t" && in_inline => {
                text_field = Some(CellTextField::Inline)
            }
            Event::Text(text) => {
                let text = text.unescape()?;
                append_cell_text(&mut current, text_field, &text)
            }
            Event::CData(text) => {
                append_cell_text(&mut current, text_field, &String::from_utf8_lossy(&text))
            }
            Event::End(event)
                if event.local_name().as_ref() == b"f"
                    || event.local_name().as_ref() == b"v"
                    || event.local_name().as_ref() == b"t" =>
            {
                text_field = None
            }
            Event::End(event) if event.local_name().as_ref() == b"is" => {
                in_inline = false;
                text_field = None;
            }
            Event::End(event) if event.local_name().as_ref() == b"c" => {
                let mut raw = current
                    .take()
                    .ok_or(XlsxError::InvalidPart("worksheet 结束了未知 cell".into()))?;
                resolve_shared_formula(&mut raw, &mut shared_formulas)?;
                cells.push(raw_cell_to_model(raw, shared_strings, styles)?);
                text_field = None;
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    if current.is_some() {
        return Err(XlsxError::UnexpectedEof("worksheet cell"));
    }
    Ok(cells)
}

fn resolve_shared_formula(
    raw: &mut RawCell,
    shared: &mut HashMap<u32, ((u32, u32), String)>,
) -> Result<(), XlsxError> {
    if raw.formula_type.as_deref() != Some("shared") {
        return Ok(());
    }
    let reference = raw
        .reference
        .as_deref()
        .ok_or_else(|| XlsxError::InvalidPart("shared formula cell 缺少 r".into()))?;
    let address = parse_a1(reference)?;
    let index = raw
        .formula_index
        .ok_or_else(|| XlsxError::InvalidPart(format!("shared formula {reference} 缺少 si")))?;
    if let Some(formula) = raw.formula.as_ref().filter(|formula| !formula.is_empty()) {
        shared.insert(index, (address, formula.clone()));
    } else {
        let (origin, formula) = shared.get(&index).ok_or_else(|| {
            XlsxError::InvalidPart(format!("shared formula {reference} 引用未知 si={index}"))
        })?;
        raw.formula = Some(translate_shared_formula(
            formula,
            i64::from(address.0) - i64::from(origin.0),
            i64::from(address.1) - i64::from(origin.1),
        ));
    }
    raw.formula_type = Some("normal".into());
    Ok(())
}

/// Expand an OOXML shared formula into the follower cell. This scanner keeps
/// quoted strings intact and translates ordinary relative A1 references while
/// preserving `$`-absolute axes. It is adapter logic, not formula evaluation.
fn translate_shared_formula(formula: &str, row_delta: i64, column_delta: i64) -> String {
    let chars = formula.chars().collect::<Vec<_>>();
    let mut out = String::with_capacity(formula.len());
    let mut index = 0;
    let mut in_string = false;
    while index < chars.len() {
        if chars[index] == '"' {
            in_string = !in_string;
            out.push(chars[index]);
            index += 1;
            continue;
        }
        if in_string {
            out.push(chars[index]);
            index += 1;
            continue;
        }
        let start = index;
        let column_absolute = chars.get(index) == Some(&'$');
        if column_absolute {
            index += 1;
        }
        let letters_start = index;
        while chars.get(index).is_some_and(|c| c.is_ascii_alphabetic()) {
            index += 1;
        }
        if letters_start == index {
            out.push(chars[start]);
            index = start + 1;
            continue;
        }
        let row_absolute = chars.get(index) == Some(&'$');
        if row_absolute {
            index += 1;
        }
        let digits_start = index;
        while chars.get(index).is_some_and(|c| c.is_ascii_digit()) {
            index += 1;
        }
        let identifier_before =
            start > 0 && chars[start - 1].is_ascii_alphanumeric() && chars[start - 1] != '!';
        let identifier_after = chars
            .get(index)
            .is_some_and(|c| c.is_ascii_alphanumeric() || matches!(*c, '_' | '!' | '('));
        if digits_start == index || identifier_before || identifier_after {
            out.extend(chars[start..index].iter());
            continue;
        }
        let token = chars[start..index].iter().collect::<String>();
        let cleaned = token.replace('$', "");
        let Ok((row, column)) = parse_a1(&cleaned) else {
            out.push_str(&token);
            continue;
        };
        let translated_row = if row_absolute {
            i64::from(row)
        } else {
            i64::from(row) + row_delta
        };
        let translated_column = if column_absolute {
            i64::from(column)
        } else {
            i64::from(column) + column_delta
        };
        if translated_row < 0 || translated_column < 0 {
            out.push_str("#REF!");
            continue;
        }
        if column_absolute {
            out.push('$');
        }
        out.push_str(&column_name(translated_column as u32));
        if row_absolute {
            out.push('$');
        }
        let _ = write!(out, "{}", translated_row + 1);
    }
    out
}

fn append_cell_text(current: &mut Option<RawCell>, field: Option<CellTextField>, text: &str) {
    let Some(current) = current.as_mut() else {
        return;
    };
    match field {
        Some(CellTextField::Formula) => current
            .formula
            .get_or_insert_with(String::new)
            .push_str(text),
        Some(CellTextField::Value) => current.value.get_or_insert_with(String::new).push_str(text),
        Some(CellTextField::Inline) => current.inline_value.push_str(text),
        None => {}
    }
}

fn raw_cell_to_model(
    raw: RawCell,
    shared_strings: &[String],
    styles: &[CellStyle],
) -> Result<CellModel, XlsxError> {
    let reference = raw
        .reference
        .ok_or_else(|| XlsxError::InvalidPart("cell 缺少 r".into()))?;
    let (row, column) = parse_a1(&reference)?;
    if let Some(formula_type) = raw
        .formula_type
        .as_deref()
        .filter(|formula_type| !matches!(*formula_type, "normal" | "array"))
    {
        return Err(XlsxError::UnsupportedFeature(format!(
            "公式类型 {formula_type}"
        )));
    }
    let value = match raw.cell_type.as_deref() {
        Some("s") => {
            let index = raw
                .value
                .as_deref()
                .unwrap_or_default()
                .parse::<usize>()
                .map_err(|_| {
                    XlsxError::InvalidPart(format!("shared string index {}", reference))
                })?;
            Some(
                shared_strings
                    .get(index)
                    .ok_or_else(|| XlsxError::InvalidPart(format!("shared string {index} 越界")))?
                    .clone()
                    .into(),
            )
        }
        Some("inlineStr") => Some(raw.inline_value.into()),
        Some("str") => raw.value.map(Value::String),
        Some("b") => raw
            .value
            .map(|value| Value::Bool(value == "1" || value.eq_ignore_ascii_case("true"))),
        Some("n") | None => raw.value.and_then(parse_number),
        Some(other) => return Err(XlsxError::UnsupportedCellType(other.into())),
    };
    let formula = raw.formula.map(|formula| {
        if formula.starts_with('=') {
            formula
        } else {
            format!("={formula}")
        }
    });
    Ok(CellModel {
        row,
        column,
        value,
        formula,
        attrs: Map::new(),
        // OOXML omits `s` for cellXfs[0], not for an absence of formatting.
        style: styles
            .get(raw.style_index.unwrap_or(0))
            .filter(|style| {
                raw.style_index.is_some()
                    || (**style != CellStyle::default()
                        && **style
                            != CellStyle {
                                number_format: Some("General".into()),
                                ..CellStyle::default()
                            })
            })
            .cloned(),
    })
}

fn parse_number(value: String) -> Option<Value> {
    if let Ok(integer) = value.parse::<i64>() {
        return Some(Value::Number(integer.into()));
    }
    value
        .parse::<f64>()
        .ok()
        .and_then(Number::from_f64)
        .map(Value::Number)
}

fn parse_a1(reference: &str) -> Result<(u32, u32), XlsxError> {
    let chars = reference.chars().collect::<Vec<_>>();
    let mut index = 0;
    let mut column = 0u32;
    while chars
        .get(index)
        .is_some_and(|character| character.is_ascii_alphabetic())
    {
        column = column
            .checked_mul(26)
            .and_then(|value| {
                value.checked_add((chars[index].to_ascii_uppercase() as u8 - b'A' + 1) as u32)
            })
            .ok_or_else(|| XlsxError::InvalidCellReference(reference.into()))?;
        index += 1;
    }
    let row_start = index;
    while chars
        .get(index)
        .is_some_and(|character| character.is_ascii_digit())
    {
        index += 1;
    }
    if index == 0 || row_start == index || index != chars.len() {
        return Err(XlsxError::InvalidCellReference(reference.into()));
    }
    let row = chars[row_start..]
        .iter()
        .collect::<String>()
        .parse::<u32>()
        .ok()
        .and_then(|value| value.checked_sub(1))
        .ok_or_else(|| XlsxError::InvalidCellReference(reference.into()))?;
    Ok((
        row,
        column
            .checked_sub(1)
            .ok_or_else(|| XlsxError::InvalidCellReference(reference.into()))?,
    ))
}

fn attr(event: &BytesStart<'_>, name: &str) -> Option<String> {
    event.attributes().flatten().find_map(|attribute| {
        (attribute.key.local_name().as_ref() == name.as_bytes())
            .then(|| {
                attribute
                    .unescape_value()
                    .ok()
                    .map(|value| value.into_owned())
            })
            .flatten()
    })
}

fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn content_types_xml(sheet_count: usize, has_shared_strings: bool, has_styles: bool) -> String {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types"><Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/><Default Extension="xml" ContentType="application/xml"/><Override PartName="/xl/workbook.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sheet.main+xml"/>"#,
    );
    for index in 1..=sheet_count {
        let _ = write!(xml, "<Override PartName=\"/xl/worksheets/sheet{index}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.spreadsheetml.worksheet+xml\"/>");
    }
    if has_shared_strings {
        xml.push_str(r#"<Override PartName="/xl/sharedStrings.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.sharedStrings+xml"/>"#);
    }
    if has_styles {
        xml.push_str(r#"<Override PartName="/xl/styles.xml" ContentType="application/vnd.openxmlformats-officedocument.spreadsheetml.styles+xml"/>"#);
    }
    xml.push_str("</Types>");
    xml
}

fn root_relationships_xml() -> String {
    r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships"><Relationship Id="rId1" Type="http://schemas.openxmlformats.org/officeDocument/2006/relationships/officeDocument" Target="xl/workbook.xml"/></Relationships>"#.into()
}

fn workbook_xml(model: &SpreadsheetModel) -> String {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships">"#,
    );
    if model.metadata.date_system == DateSystem::Excel1904 {
        xml.push_str(r#"<workbookPr date1904="1"/>"#);
    }
    let active_tab = model
        .metadata
        .active_sheet_id
        .as_ref()
        .and_then(|id| model.sheets.iter().position(|sheet| &sheet.id == id));
    xml.push_str("<bookViews><workbookView");
    if let Some(active_tab) = active_tab {
        let _ = write!(xml, " activeTab=\"{active_tab}\"");
    }
    xml.push_str("/></bookViews><sheets>");
    for (index, sheet) in model.sheets.iter().enumerate() {
        let state = match sheet.metadata.visibility {
            SheetVisibility::Visible => "",
            SheetVisibility::Hidden => " state=\"hidden\"",
            SheetVisibility::VeryHidden => " state=\"veryHidden\"",
        };
        let _ = write!(
            xml,
            "<sheet name=\"{}\" sheetId=\"{}\" r:id=\"rId{}\"{} />",
            xml_escape(&sheet.name),
            index + 1,
            index + 1,
            state
        );
    }
    xml.push_str("</sheets>");
    if !model.metadata.named_ranges.is_empty() {
        xml.push_str("<definedNames>");
        for named in &model.metadata.named_ranges {
            let local = named
                .scope_sheet_id
                .as_ref()
                .and_then(|id| model.sheets.iter().position(|sheet| &sheet.id == id))
                .map(|index| format!(" localSheetId=\"{index}\""))
                .unwrap_or_default();
            let target = model
                .sheets
                .iter()
                .find(|sheet| sheet.id == named.sheet_id)
                .map(|sheet| sheet.name.replace('\'', "''"))
                .unwrap_or_default();
            let range = format!(
                "${}${}:${}${}",
                column_name(named.range.start_column),
                named.range.start_row + 1,
                column_name(named.range.end_column),
                named.range.end_row + 1
            );
            let _ = write!(
                xml,
                "<definedName name=\"{}\"{local}>'{}'!{}</definedName>",
                xml_escape(&named.name),
                xml_escape(&target),
                range
            );
        }
        xml.push_str("</definedNames>");
    }
    if model.metadata.calculation_mode == oo_schema::CalculationMode::Manual {
        xml.push_str(r#"<calcPr calcMode="manual"/>"#);
    }
    xml.push_str("</workbook>");
    xml
}

fn workbook_relationships_xml(
    sheet_count: usize,
    has_shared_strings: bool,
    has_styles: bool,
) -> String {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">"#,
    );
    for index in 1..=sheet_count {
        let _ = write!(xml, "<Relationship Id=\"rId{index}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/worksheet\" Target=\"worksheets/sheet{index}.xml\"/>");
    }
    if has_shared_strings {
        let _ = write!(xml, "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/sharedStrings\" Target=\"sharedStrings.xml\"/>", sheet_count + 1);
    }
    if has_styles {
        let id = sheet_count + 1 + usize::from(has_shared_strings);
        let _ = write!(xml, "<Relationship Id=\"rId{}\" Type=\"http://schemas.openxmlformats.org/officeDocument/2006/relationships/styles\" Target=\"styles.xml\"/>", id);
    }
    xml.push_str("</Relationships>");
    xml
}

fn shared_strings_xml(shared: &BTreeMap<String, usize>) -> String {
    let mut xml = format!(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><sst xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" count="{}" uniqueCount="{}">"#,
        shared.len(),
        shared.len()
    );
    let mut ordered = shared.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|(_, index)| **index);
    for (value, _) in ordered {
        let preserve = value.chars().next().is_some_and(char::is_whitespace)
            || value.chars().last().is_some_and(char::is_whitespace);
        if preserve {
            let _ = write!(
                xml,
                "<si><t xml:space=\"preserve\">{}</t></si>",
                xml_escape(value)
            );
        } else {
            let _ = write!(xml, "<si><t>{}</t></si>", xml_escape(value));
        }
    }
    xml.push_str("</sst>");
    xml
}

fn worksheet_xml(
    sheet: &SheetModel,
    shared: &HashMap<&str, usize>,
    styles: &HashMap<String, usize>,
    differential_styles: &HashMap<String, usize>,
) -> Result<String, XlsxError> {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
    );
    if let (Some(rows), Some(columns)) = (sheet.metadata.row_count, sheet.metadata.column_count) {
        if rows > 0 && columns > 0 {
            let _ = write!(
                xml,
                "<dimension ref=\"A1:{}{}\"/>",
                column_name(columns - 1),
                rows
            );
        }
    }
    if sheet.metadata.freeze != FreezePane::default() {
        xml.push_str("<sheetViews><sheetView workbookViewId=\"0\"><pane");
        if sheet.metadata.freeze.columns > 0 {
            let _ = write!(xml, " xSplit=\"{}\"", sheet.metadata.freeze.columns);
        }
        if sheet.metadata.freeze.rows > 0 {
            let _ = write!(xml, " ySplit=\"{}\"", sheet.metadata.freeze.rows);
        }
        let _ = write!(
            xml,
            " topLeftCell=\"{}{}\" state=\"frozen\"/></sheetView></sheetViews>",
            column_name(sheet.metadata.freeze.columns),
            sheet.metadata.freeze.rows + 1
        );
    }
    xml.push_str("<sheetData>");
    let mut rows = std::collections::BTreeMap::<u32, Vec<&CellModel>>::new();
    for cell in &sheet.cells {
        rows.entry(cell.row).or_default().push(cell);
    }
    for entry in &sheet.metadata.row_layout {
        rows.entry(entry.row).or_default();
    }
    for (row, cells) in rows {
        let _ = write!(xml, "<row r=\"{}\"", row + 1);
        if let Some(layout) = sheet
            .metadata
            .row_layout
            .iter()
            .find(|entry| entry.row == row)
        {
            if let Some(height) = layout.height {
                let _ = write!(xml, " ht=\"{}\" customHeight=\"1\"", height);
            }
            if layout.hidden {
                xml.push_str(" hidden=\"1\"");
            }
        }
        xml.push('>');
        for cell in cells {
            let reference = format!("{}{}", column_name(cell.column), cell.row + 1);
            let mut attributes = String::new();
            let mut body = String::new();
            if let Some(formula) = &cell.formula {
                body.push_str("<f>");
                body.push_str(&xml_escape(formula.trim_start_matches('=')));
                body.push_str("</f>");
                if let Some(value) = &cell.value {
                    append_value(&mut attributes, &mut body, value, true, shared)?;
                }
            } else if let Some(value) = &cell.value {
                append_value(&mut attributes, &mut body, value, false, shared)?;
            }
            if let Some(style) = &cell.style {
                let key = serde_json::to_string(style).unwrap_or_default();
                if let Some(index) = styles.get(&key) {
                    let _ = write!(attributes, " s=\"{}\"", index);
                }
            }
            let _ = write!(xml, "<c r=\"{}\"{}>{}</c>", reference, attributes, body);
        }
        xml.push_str("</row>");
    }
    xml.push_str("</sheetData>");
    if !sheet.metadata.merged_ranges.is_empty() {
        let _ = write!(
            xml,
            "<mergeCells count=\"{}\">",
            sheet.metadata.merged_ranges.len()
        );
        for range in &sheet.metadata.merged_ranges {
            let _ = write!(xml, "<mergeCell ref=\"{}\"/>", range_ref(*range));
        }
        xml.push_str("</mergeCells>");
    }
    if let Some(filter) = &sheet.metadata.auto_filter {
        let _ = write!(xml, "<autoFilter ref=\"{}\">", range_ref(filter.range));
        for column in &filter.columns {
            let _ = write!(
                xml,
                "<filterColumn colId=\"{}\">",
                column.column - filter.range.start_column
            );
            match &column.predicate {
                oo_schema::FilterPredicate::Values(values) => {
                    xml.push_str("<filters>");
                    for value in values {
                        let text = value
                            .as_str()
                            .map(str::to_owned)
                            .unwrap_or_else(|| value.to_string());
                        let _ = write!(xml, "<filter val=\"{}\"/>", xml_escape(&text));
                    }
                    xml.push_str("</filters>");
                }
                predicate => {
                    let (operator, value) = match predicate {
                        oo_schema::FilterPredicate::Contains(value) => {
                            ("equal", format!("*{value}*"))
                        }
                        oo_schema::FilterPredicate::Equals(value) => (
                            "equal",
                            value
                                .as_str()
                                .map(str::to_owned)
                                .unwrap_or_else(|| value.to_string()),
                        ),
                        oo_schema::FilterPredicate::GreaterThan(value) => {
                            ("greaterThan", value.to_string())
                        }
                        oo_schema::FilterPredicate::LessThan(value) => {
                            ("lessThan", value.to_string())
                        }
                        oo_schema::FilterPredicate::Values(_) => unreachable!(),
                    };
                    let _ = write!(xml, "<customFilters><customFilter operator=\"{operator}\" val=\"{}\"/></customFilters>", xml_escape(&value));
                }
            }
            xml.push_str("</filterColumn>");
        }
        xml.push_str("</autoFilter>");
    }
    if let Some(sort) = &sheet.metadata.sort {
        let _ = write!(xml, "<sortState ref=\"{}\">", range_ref(sort.range));
        for key in &sort.keys {
            let descending = if key.direction == oo_schema::SortDirection::Descending {
                " descending=\"1\""
            } else {
                ""
            };
            let _ = write!(
                xml,
                "<sortCondition ref=\"{}1:{}1048576\"{descending}/>",
                column_name(key.column),
                column_name(key.column)
            );
        }
        xml.push_str("</sortState>");
    }
    append_conditional_formats(
        &mut xml,
        &sheet.metadata.conditional_formats,
        differential_styles,
    )?;
    append_data_validations(&mut xml, &sheet.metadata.data_validations);
    xml.push_str("</worksheet>");
    Ok(xml)
}

fn range_ref(range: GridRange) -> String {
    format!(
        "{}{}:{}{}",
        column_name(range.start_column),
        range.start_row + 1,
        column_name(range.end_column),
        range.end_row + 1
    )
}

fn append_conditional_formats(
    xml: &mut String,
    rules: &[oo_schema::ConditionalFormatRule],
    differential_styles: &HashMap<String, usize>,
) -> Result<(), XlsxError> {
    for (priority, rule) in rules.iter().enumerate() {
        let dxf_id = (rule.style != CellStyle::default())
            .then(|| {
                let key = serde_json::to_string(&rule.style).unwrap_or_default();
                differential_styles.get(&key).copied().ok_or_else(|| {
                    XlsxError::InvalidPart(format!("conditional format {} 的 dxf 未登记", rule.id))
                })
            })
            .transpose()?;
        let dxf_attribute = dxf_id
            .map(|index| format!(" dxfId=\"{index}\""))
            .unwrap_or_default();
        let _ = write!(
            xml,
            "<conditionalFormatting sqref=\"{}\">",
            range_ref(rule.range)
        );
        match &rule.predicate {
            oo_schema::ConditionalPredicate::CellIs { operator, value } => {
                let operator = match operator {
                    oo_schema::ComparisonOperator::Equal => "equal",
                    oo_schema::ComparisonOperator::NotEqual => "notEqual",
                    oo_schema::ComparisonOperator::GreaterThan => "greaterThan",
                    oo_schema::ComparisonOperator::GreaterThanOrEqual => "greaterThanOrEqual",
                    oo_schema::ComparisonOperator::LessThan => "lessThan",
                    oo_schema::ComparisonOperator::LessThanOrEqual => "lessThanOrEqual",
                };
                let formula = scalar_formula(value)?;
                let _ = write!(xml, "<cfRule type=\"cellIs\" operator=\"{operator}\" priority=\"{}\"{dxf_attribute}><formula>{}</formula></cfRule>", priority + 1, xml_escape(&formula));
            }
            oo_schema::ConditionalPredicate::Formula(formula) => {
                let _ = write!(
                    xml,
                    "<cfRule type=\"expression\" priority=\"{}\"{dxf_attribute}><formula>{}</formula></cfRule>",
                    priority + 1,
                    xml_escape(formula.trim_start_matches('='))
                );
            }
            oo_schema::ConditionalPredicate::ColorScale { min, max } => {
                let _ = write!(xml, "<cfRule type=\"colorScale\" priority=\"{}\"{dxf_attribute}><colorScale><cfvo type=\"min\"/><cfvo type=\"max\"/><color rgb=\"FF{}\"/><color rgb=\"FF{}\"/></colorScale></cfRule>", priority + 1, xml_escape(min.trim_start_matches('#')), xml_escape(max.trim_start_matches('#')));
            }
        }
        xml.push_str("</conditionalFormatting>");
    }
    Ok(())
}

fn scalar_formula(value: &Value) -> Result<String, XlsxError> {
    match value {
        Value::String(value) => Ok(format!("\"{}\"", value.replace('"', "\"\""))),
        Value::Number(value) => Ok(value.to_string()),
        Value::Bool(value) => Ok(if *value { "TRUE" } else { "FALSE" }.into()),
        Value::Null | Value::Array(_) | Value::Object(_) => Err(XlsxError::UnsupportedFeature(
            "conditional-format cellIs value must be scalar".into(),
        )),
    }
}

fn append_data_validations(xml: &mut String, rules: &[oo_schema::DataValidationRule]) {
    if rules.is_empty() {
        return;
    }
    let _ = write!(xml, "<dataValidations count=\"{}\">", rules.len());
    for rule in rules {
        let (kind, first, second) = match &rule.kind {
            oo_schema::DataValidationKind::List(values) => (
                "list",
                format!("\"{}\"", values.join(",").replace('"', "\"\"")),
                None,
            ),
            oo_schema::DataValidationKind::WholeNumber { min, max } => {
                ("whole", min.to_string(), Some(max.to_string()))
            }
            oo_schema::DataValidationKind::Decimal { min, max } => {
                ("decimal", min.to_string(), Some(max.to_string()))
            }
            oo_schema::DataValidationKind::Date {
                min_serial,
                max_serial,
            } => ("date", min_serial.to_string(), Some(max_serial.to_string())),
            oo_schema::DataValidationKind::CustomFormula(formula) => {
                ("custom", formula.trim_start_matches('=').to_string(), None)
            }
        };
        let allow_blank = if rule.allow_blank { "1" } else { "0" };
        let error = rule
            .error_message
            .as_deref()
            .map(|value| format!(" error=\"{}\"", xml_escape(value)))
            .unwrap_or_default();
        let _ = write!(xml, "<dataValidation type=\"{kind}\" allowBlank=\"{allow_blank}\" sqref=\"{}\"{error}><formula1>{}</formula1>", range_ref(rule.range), xml_escape(&first));
        if let Some(second) = second {
            let _ = write!(xml, "<formula2>{}</formula2>", xml_escape(&second));
        }
        xml.push_str("</dataValidation>");
    }
    xml.push_str("</dataValidations>");
}

fn styles_xml(styles: &[CellStyle], differential_styles: &[CellStyle]) -> String {
    let mut fonts = vec![FontStyle::default()];
    let mut fills = vec![FillStyle::default(), FillStyle::default()];
    let mut borders = vec![CellBorders::default()];
    let mut custom_formats = BTreeMap::<String, u32>::new();
    for style in styles {
        if let Some(value) = &style.font {
            if !fonts.contains(value) {
                fonts.push(value.clone());
            }
        }
        if let Some(value) = &style.fill {
            if !fills.contains(value) {
                fills.push(value.clone());
            }
        }
        if let Some(value) = &style.borders {
            if !borders.contains(value) {
                borders.push(value.clone());
            }
        }
        if let Some(format) = style
            .number_format
            .as_deref()
            .filter(|format| builtin_num_format_id(format).is_none())
        {
            let next = 164 + custom_formats.len() as u32;
            custom_formats.entry(format.to_string()).or_insert(next);
        }
    }
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main">"#,
    );
    if !custom_formats.is_empty() {
        let _ = write!(xml, "<numFmts count=\"{}\">", custom_formats.len());
        for (format, id) in &custom_formats {
            let _ = write!(
                xml,
                "<numFmt numFmtId=\"{id}\" formatCode=\"{}\"/>",
                xml_escape(format)
            );
        }
        xml.push_str("</numFmts>");
    }
    let _ = write!(xml, "<fonts count=\"{}\">", fonts.len());
    for font in &fonts {
        xml.push_str("<font>");
        if let Some(family) = &font.family {
            let family = oo_schema::font_family::primary_font_family(family);
            let _ = write!(xml, "<name val=\"{}\"/>", xml_escape(&family));
        }
        if let Some(size) = font.size {
            let _ = write!(xml, "<sz val=\"{size}\"/>");
        }
        if font.bold {
            xml.push_str("<b/>");
        }
        if font.italic {
            xml.push_str("<i/>");
        }
        if font.underline {
            xml.push_str("<u/>");
        }
        if font.strikethrough {
            xml.push_str("<strike/>");
        }
        if let Some(color) = &font.color {
            let _ = write!(
                xml,
                "<color rgb=\"FF{}\"/>",
                xml_escape(color.trim_start_matches('#'))
            );
        }
        xml.push_str("</font>");
    }
    let _ = write!(xml, "</fonts><fills count=\"{}\"><fill><patternFill patternType=\"none\"/></fill><fill><patternFill patternType=\"gray125\"/></fill>", fills.len());
    for fill in fills.iter().skip(2) {
        xml.push_str("<fill><patternFill patternType=\"solid\">");
        if let Some(color) = &fill.foreground {
            let _ = write!(
                xml,
                "<fgColor rgb=\"FF{}\"/>",
                xml_escape(color.trim_start_matches('#'))
            );
        }
        if let Some(color) = &fill.background {
            let _ = write!(
                xml,
                "<bgColor rgb=\"FF{}\"/>",
                xml_escape(color.trim_start_matches('#'))
            );
        }
        xml.push_str("</patternFill></fill>");
    }
    let _ = write!(
        xml,
        "</fills><borders count=\"{}\"><border/>",
        borders.len()
    );
    for border in borders.iter().skip(1) {
        xml.push_str("<border>");
        for (name, edge) in [
            ("left", &border.left),
            ("right", &border.right),
            ("top", &border.top),
            ("bottom", &border.bottom),
        ] {
            match edge {
                Some(edge) => {
                    let style = edge
                        .style
                        .as_deref()
                        .map(|value| format!(" style=\"{}\"", xml_escape(value)))
                        .unwrap_or_default();
                    let _ = write!(xml, "<{name}{style}>");
                    if let Some(color) = &edge.color {
                        let _ = write!(
                            xml,
                            "<color rgb=\"FF{}\"/>",
                            xml_escape(color.trim_start_matches('#'))
                        );
                    }
                    let _ = write!(xml, "</{name}>");
                }
                None => {
                    let _ = write!(xml, "<{name}/>");
                }
            }
        }
        xml.push_str("<diagonal/></border>");
    }
    xml.push_str(r#"</borders><cellStyleXfs count="1"><xf/></cellStyleXfs><cellXfs count=""#);
    let _ = write!(xml, "{}\">", styles.len());
    for style in styles {
        let format_id = style
            .number_format
            .as_deref()
            .and_then(builtin_num_format_id)
            .or_else(|| {
                style
                    .number_format
                    .as_ref()
                    .and_then(|format| custom_formats.get(format).copied())
            })
            .unwrap_or(0);
        let font_id = style
            .font
            .as_ref()
            .and_then(|value| fonts.iter().position(|known| known == value))
            .unwrap_or(0);
        let fill_id = style
            .fill
            .as_ref()
            .and_then(|value| fills.iter().position(|known| known == value))
            .unwrap_or(0);
        let border_id = style
            .borders
            .as_ref()
            .and_then(|value| borders.iter().position(|known| known == value))
            .unwrap_or(0);
        let _ = write!(xml, "<xf numFmtId=\"{format_id}\" fontId=\"{font_id}\" fillId=\"{fill_id}\" borderId=\"{border_id}\" xfId=\"0\" applyNumberFormat=\"1\" applyFont=\"1\" applyFill=\"1\" applyBorder=\"1\"");
        if let Some(alignment) = &style.alignment {
            xml.push_str(" applyAlignment=\"1\"><alignment");
            if let Some(horizontal) = &alignment.horizontal {
                let _ = write!(xml, " horizontal=\"{}\"", xml_escape(horizontal));
            }
            if let Some(vertical) = &alignment.vertical {
                let vertical = if vertical == "middle" {
                    "center"
                } else {
                    vertical
                };
                let _ = write!(xml, " vertical=\"{}\"", xml_escape(vertical));
            }
            if alignment.wrap {
                xml.push_str(" wrapText=\"1\"");
            }
            xml.push_str("/></xf>");
        } else {
            xml.push_str("/>");
        }
    }
    xml.push_str(r#"</cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles>"#);
    if !differential_styles.is_empty() {
        let _ = write!(xml, "<dxfs count=\"{}\">", differential_styles.len());
        for style in differential_styles {
            append_differential_style(&mut xml, style);
        }
        xml.push_str("</dxfs>");
    }
    xml.push_str("</styleSheet>");
    xml
}

fn append_differential_style(xml: &mut String, style: &CellStyle) {
    xml.push_str("<dxf>");
    if let Some(format) = &style.number_format {
        let id = builtin_num_format_id(format).unwrap_or(164);
        let _ = write!(
            xml,
            "<numFmt numFmtId=\"{id}\" formatCode=\"{}\"/>",
            xml_escape(format)
        );
    }
    if let Some(font) = &style.font {
        xml.push_str("<font>");
        if let Some(family) = &font.family {
            let family = oo_schema::font_family::primary_font_family(family);
            let _ = write!(xml, "<name val=\"{}\"/>", xml_escape(&family));
        }
        if let Some(size) = font.size {
            let _ = write!(xml, "<sz val=\"{size}\"/>");
        }
        if font.bold {
            xml.push_str("<b/>");
        }
        if font.italic {
            xml.push_str("<i/>");
        }
        if font.underline {
            xml.push_str("<u/>");
        }
        if font.strikethrough {
            xml.push_str("<strike/>");
        }
        if let Some(color) = &font.color {
            let _ = write!(
                xml,
                "<color rgb=\"FF{}\"/>",
                xml_escape(color.trim_start_matches('#'))
            );
        }
        xml.push_str("</font>");
    }
    if let Some(fill) = &style.fill {
        xml.push_str("<fill><patternFill patternType=\"solid\">");
        if let Some(color) = &fill.foreground {
            let _ = write!(
                xml,
                "<fgColor rgb=\"FF{}\"/>",
                xml_escape(color.trim_start_matches('#'))
            );
        }
        if let Some(color) = &fill.background {
            let _ = write!(
                xml,
                "<bgColor rgb=\"FF{}\"/>",
                xml_escape(color.trim_start_matches('#'))
            );
        }
        xml.push_str("</patternFill></fill>");
    }
    if let Some(borders) = &style.borders {
        xml.push_str("<border>");
        for (name, edge) in [
            ("left", &borders.left),
            ("right", &borders.right),
            ("top", &borders.top),
            ("bottom", &borders.bottom),
        ] {
            if let Some(edge) = edge {
                let style = edge
                    .style
                    .as_deref()
                    .map(|value| format!(" style=\"{}\"", xml_escape(value)))
                    .unwrap_or_default();
                let _ = write!(xml, "<{name}{style}>");
                if let Some(color) = &edge.color {
                    let _ = write!(
                        xml,
                        "<color rgb=\"FF{}\"/>",
                        xml_escape(color.trim_start_matches('#'))
                    );
                }
                let _ = write!(xml, "</{name}>");
            } else {
                let _ = write!(xml, "<{name}/>");
            }
        }
        xml.push_str("<diagonal/></border>");
    }
    if let Some(alignment) = &style.alignment {
        xml.push_str("<alignment");
        if let Some(horizontal) = &alignment.horizontal {
            let _ = write!(xml, " horizontal=\"{}\"", xml_escape(horizontal));
        }
        if let Some(vertical) = &alignment.vertical {
            let vertical = if vertical == "middle" {
                "center"
            } else {
                vertical
            };
            let _ = write!(xml, " vertical=\"{}\"", xml_escape(vertical));
        }
        if alignment.wrap {
            xml.push_str(" wrapText=\"1\"");
        }
        xml.push_str("/>");
    }
    xml.push_str("</dxf>");
}

fn builtin_num_format_id(format: &str) -> Option<u32> {
    match format {
        "General" => Some(0),
        "0" => Some(1),
        "0.00" => Some(2),
        "0%" => Some(9),
        "0.00%" => Some(10),
        "mm-dd-yy" => Some(14),
        "h:mm" => Some(20),
        "h:mm:ss" => Some(21),
        "m/d/yy h:mm" => Some(22),
        _ => None,
    }
}

fn append_value(
    attributes: &mut String,
    body: &mut String,
    value: &Value,
    formula: bool,
    shared: &HashMap<&str, usize>,
) -> Result<(), XlsxError> {
    match value {
        Value::String(value) if !formula => {
            let index = *shared
                .get(value.as_str())
                .ok_or_else(|| XlsxError::InvalidPart("shared string 未登记".into()))?;
            attributes.push_str(" t=\"s\"");
            write!(body, "<v>{index}</v>").unwrap();
        }
        Value::String(value) => {
            attributes.push_str(" t=\"str\"");
            write!(body, "<v>{}</v>", xml_escape(value)).unwrap();
        }
        Value::Number(value) => {
            write!(body, "<v>{}</v>", value).unwrap();
        }
        Value::Bool(value) => {
            attributes.push_str(" t=\"b\"");
            body.push_str(if *value { "<v>1</v>" } else { "<v>0</v>" });
        }
        Value::Null => {}
        Value::Array(_) | Value::Object(_) => {
            return Err(XlsxError::UnsupportedFeature(
                "cell value 必须是字符串、数字或布尔值".into(),
            ))
        }
    }
    Ok(())
}

fn column_name(mut column: u32) -> String {
    let mut name = String::new();
    loop {
        name.insert(0, (b'A' + (column % 26) as u8) as char);
        if column < 26 {
            break;
        }
        column = column / 26 - 1;
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_model() -> SpreadsheetModel {
        SpreadsheetModel {
            metadata: SpreadsheetMetadata::default(),
            sheets: vec![SheetModel {
                id: "sheet-1".into(),
                name: "Data".into(),
                cells: vec![
                    CellModel {
                        row: 0,
                        column: 0,
                        value: Some(Value::String("hello & world".into())),
                        ..CellModel::default()
                    },
                    CellModel {
                        row: 1,
                        column: 1,
                        value: Some(Value::Number(42.into())),
                        ..CellModel::default()
                    },
                    CellModel {
                        row: 2,
                        column: 2,
                        formula: Some("=A1+B2".into()),
                        value: Some(Value::Number(43.into())),
                        ..CellModel::default()
                    },
                    CellModel {
                        row: 3,
                        column: 3,
                        value: Some(Value::Bool(true)),
                        ..CellModel::default()
                    },
                ],
                metadata: SheetMetadata::default(),
            }],
        }
    }

    fn zip_with_parts(parts: &[(&str, &str)]) -> Vec<u8> {
        let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, content) in parts {
            archive
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            archive.write_all(content.as_bytes()).unwrap();
        }
        archive.finish().unwrap().into_inner()
    }

    #[test]
    fn row_layout_roundtrips_including_empty_hidden_rows() {
        let mut model = fixture_model();
        model.sheets[0].metadata.row_layout = vec![
            oo_schema::SheetRowLayout {
                row: 0,
                height: Some(42.0),
                hidden: false,
            },
            oo_schema::SheetRowLayout {
                row: 20,
                height: Some(18.0),
                hidden: true,
            },
        ];
        let imported = read_xlsx(&write_xlsx(&model).unwrap()).unwrap();
        assert_eq!(
            imported.sheets[0].metadata.row_layout,
            model.sheets[0].metadata.row_layout
        );
    }

    #[test]
    fn font_export_roundtrip_preserves_formatting_and_neutral_cells() {
        let mut model = fixture_model();
        let font = FontStyle {
            family: Some("\"Noto Serif SC\", serif".into()),
            size: Some(18.0),
            bold: true,
            italic: true,
            underline: true,
            strikethrough: true,
            color: Some("#123ABC".into()),
        };
        model.sheets[0].cells[0].style = Some(CellStyle {
            font: Some(font.clone()),
            ..CellStyle::default()
        });
        let imported = read_xlsx(&write_xlsx(&model).unwrap()).unwrap();
        let restored = imported.sheets[0].cells[0]
            .style
            .as_ref()
            .unwrap()
            .font
            .as_ref()
            .unwrap();
        assert_eq!(
            restored,
            &FontStyle {
                family: Some("Noto Serif SC".into()),
                ..font
            }
        );
        assert!(imported.sheets[0].cells[1].style.is_none());
    }

    #[test]
    fn fill_alignment_and_borders_roundtrip_as_typed_styles() {
        let mut model = fixture_model();
        let style = CellStyle {
            number_format: Some("yyyy-mm-dd".into()),
            fill: Some(FillStyle {
                foreground: Some("#FFF1F0".into()),
                background: Some("#FFFFFF".into()),
            }),
            alignment: Some(AlignmentStyle {
                horizontal: Some("center".into()),
                vertical: Some("middle".into()),
                wrap: true,
            }),
            borders: Some(CellBorders {
                top: Some(CellBorderEdge {
                    style: Some("thin".into()),
                    color: Some("#123ABC".into()),
                }),
                bottom: Some(CellBorderEdge {
                    style: Some("double".into()),
                    color: Some("#456DEF".into()),
                }),
                left: None,
                right: None,
            }),
            ..CellStyle::default()
        };
        model.sheets[0].cells[0].style = Some(style.clone());
        let imported = read_xlsx(&write_xlsx(&model).unwrap()).unwrap();
        assert_eq!(imported.sheets[0].cells[0].style.as_ref(), Some(&style));
    }

    #[test]
    fn cells_without_style_attribute_inherit_workbook_default_font() {
        let styles = parse_styles(br#"<styleSheet><fonts count="2"><font><name val="Noto Serif SC"/><sz val="18"/><b/></font><font><name val="Lora"/><sz val="24"/></font></fonts><cellXfs count="2"><xf numFmtId="0" fontId="0"/><xf numFmtId="0" fontId="1"/></cellXfs></styleSheet>"#).unwrap();
        let cells = parse_worksheet(br#"<worksheet><sheetData><row r="1"><c r="A1"><v>1</v></c><c r="B1" s="1"><v>2</v></c><c r="C1" s="0"><v>3</v></c></row></sheetData></worksheet>"#, &[], &styles).unwrap();
        let default_font = cells[0].style.as_ref().unwrap().font.as_ref().unwrap();
        assert_eq!(default_font.family.as_deref(), Some("Noto Serif SC"));
        assert_eq!(default_font.size, Some(18.0));
        assert!(default_font.bold);
        assert_eq!(cells[2].style, cells[0].style);
        assert_eq!(
            cells[1]
                .style
                .as_ref()
                .unwrap()
                .font
                .as_ref()
                .unwrap()
                .family
                .as_deref(),
            Some("Lora")
        );
    }

    #[test]
    fn export_import_roundtrip_preserves_supported_sparse_cells() {
        let model = fixture_model();
        let bytes = write_xlsx(&model).unwrap();
        let imported = read_xlsx(&bytes).unwrap();
        assert_eq!(imported.sheets[0].name, "Data");
        assert_eq!(imported.sheets[0].cells, model.sheets[0].cells);
    }

    #[test]
    fn parser_reads_inline_shared_number_and_formula_cells() {
        let parts = [
            (CONTENT_TYPES, "<Types/>"),
            (ROOT_RELS, "<Relationships/>"),
            (
                WORKBOOK,
                r#"<workbook xmlns:r="urn:r"><sheets><sheet name="Data" sheetId="7" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                WORKBOOK_RELS,
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1" t="inlineStr"><is><t>inline</t></is></c><c r="B1"><v>12.5</v></c><c r="C1" t="s"><v>0</v></c><c r="D1"><f>A1+B1</f><v>13.5</v></c></row></sheetData></worksheet>"#,
            ),
            (SHARED_STRINGS, r#"<sst><si><t>shared</t></si></sst>"#),
        ];
        let imported = read_xlsx(&zip_with_parts(&parts)).unwrap();
        let cells = &imported.sheets[0].cells;
        assert_eq!(cells[0].value, Some(Value::String("inline".into())));
        assert_eq!(
            cells[1].value,
            Some(Value::Number(Number::from_f64(12.5).unwrap()))
        );
        assert_eq!(cells[2].value, Some(Value::String("shared".into())));
        assert_eq!(cells[3].formula.as_deref(), Some("=A1+B1"));
        assert_eq!(
            cells[3].value,
            Some(Value::Number(Number::from_f64(13.5).unwrap()))
        );
    }

    #[test]
    fn unsupported_parts_are_reported_not_silently_claimed() {
        let model = fixture_model();
        let mut bytes = write_xlsx(&model).unwrap();
        let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).unwrap();
        let mut parts = Vec::new();
        for index in 0..archive.len() {
            let mut file = archive.by_index(index).unwrap();
            let mut content = Vec::new();
            file.read_to_end(&mut content).unwrap();
            parts.push((file.name().to_string(), content));
        }
        drop(archive);
        let mut output = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, content) in parts {
            output
                .start_file(name, SimpleFileOptions::default())
                .unwrap();
            output.write_all(&content).unwrap();
        }
        output
            .start_file("xl/comments1.xml", SimpleFileOptions::default())
            .unwrap();
        output.write_all(b"<comments/>").unwrap();
        bytes = output.finish().unwrap().into_inner();
        let result = read_xlsx_with_report(&bytes).unwrap();
        assert_eq!(result.ignored_parts, vec!["xl/comments1.xml"]);
        assert_eq!(result.unsupported_parts[0].part, "xl/comments1.xml");
        assert!(result.unsupported_parts[0].reason.contains("comment"));
    }

    #[test]
    fn unsupported_attrs_fail_export() {
        let mut model = fixture_model();
        model.sheets[0].cells[0]
            .attrs
            .insert("fill".into(), Value::String("red".into()));
        assert!(matches!(
            write_xlsx(&model),
            Err(XlsxError::UnsupportedFeature(_))
        ));
    }

    #[test]
    fn duplicate_relationship_ids_are_rejected() {
        let parts = [
            (CONTENT_TYPES, "<Types/>"),
            (ROOT_RELS, "<Relationships/>"),
            (
                WORKBOOK,
                r#"<workbook xmlns:r="urn:r"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                WORKBOOK_RELS,
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/><Relationship Id="rId1" Target="worksheets/sheet2.xml"/></Relationships>"#,
            ),
        ];
        let bytes = zip_with_parts(&parts);
        assert!(matches!(
            read_xlsx(&bytes),
            Err(XlsxError::InvalidPart(message)) if message.contains("Relationship rId1 重复")
        ));
    }

    #[test]
    fn shared_formula_is_expanded_with_relative_and_absolute_axes() {
        let parts = [
            (CONTENT_TYPES, "<Types/>"),
            (ROOT_RELS, "<Relationships/>"),
            (
                WORKBOOK,
                r#"<workbook xmlns:r="urn:r"><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                WORKBOOK_RELS,
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                r#"<worksheet><sheetData><row r="1"><c r="A1"><f t="shared" si="0" ref="A1:A2">SUM(Sheet1!A2,$B$2)</f><v>3</v></c></row><row r="2"><c r="A2"><f t="shared" si="0"/><v>4</v></c></row></sheetData></worksheet>"#,
            ),
        ];
        let model = read_xlsx(&zip_with_parts(&parts)).unwrap();
        assert_eq!(
            model.sheets[0].cells[0].formula.as_deref(),
            Some("=SUM(Sheet1!A2,$B$2)")
        );
        assert_eq!(
            model.sheets[0].cells[1].formula.as_deref(),
            Some("=SUM(Sheet1!A3,$B$2)")
        );
    }

    #[test]
    fn worksheet_rules_merge_freeze_filter_and_sort_roundtrip() {
        let mut model = fixture_model();
        model.sheets[0].id = "source-sheet".into();
        model.metadata.active_sheet_id = Some("source-sheet".into());
        model.metadata.named_ranges = vec![oo_schema::SpreadsheetNamedRange {
            name: "InputArea".into(),
            scope_sheet_id: Some("source-sheet".into()),
            sheet_id: "source-sheet".into(),
            range: GridRange {
                start_row: 1,
                start_column: 1,
                end_row: 3,
                end_column: 2,
            },
        }];
        let metadata = &mut model.sheets[0].metadata;
        metadata.row_count = Some(50);
        metadata.column_count = Some(10);
        metadata.freeze = FreezePane {
            rows: 1,
            columns: 2,
        };
        metadata.merged_ranges = vec![GridRange {
            start_row: 5,
            start_column: 0,
            end_row: 5,
            end_column: 2,
        }];
        metadata.auto_filter = Some(oo_schema::FilterSpec {
            range: GridRange {
                start_row: 0,
                start_column: 0,
                end_row: 20,
                end_column: 2,
            },
            columns: vec![oo_schema::FilterColumn {
                column: 1,
                predicate: oo_schema::FilterPredicate::GreaterThan(10.0),
            }],
        });
        metadata.sort = Some(oo_schema::SortSpec {
            range: GridRange {
                start_row: 0,
                start_column: 0,
                end_row: 20,
                end_column: 2,
            },
            keys: vec![oo_schema::SortKey {
                column: 1,
                direction: oo_schema::SortDirection::Descending,
            }],
        });
        metadata.data_validations = vec![oo_schema::DataValidationRule {
            id: "validation-source".into(),
            range: GridRange {
                start_row: 1,
                start_column: 3,
                end_row: 10,
                end_column: 3,
            },
            kind: oo_schema::DataValidationKind::WholeNumber { min: 0, max: 10 },
            allow_blank: true,
            error_message: Some("0 到 10".into()),
        }];
        metadata.conditional_formats = vec![oo_schema::ConditionalFormatRule {
            id: "conditional-source".into(),
            range: GridRange {
                start_row: 1,
                start_column: 4,
                end_row: 10,
                end_column: 4,
            },
            predicate: oo_schema::ConditionalPredicate::CellIs {
                operator: oo_schema::ComparisonOperator::GreaterThan,
                value: Value::from(5),
            },
            style: CellStyle::default(),
        }];
        let expected = metadata.clone();

        let imported = read_xlsx(&write_xlsx(&model).unwrap()).unwrap();
        assert_eq!(
            imported.metadata.active_sheet_id.as_deref(),
            Some("sheet-1")
        );
        let restored = &imported.sheets[0].metadata;
        assert_eq!(restored.freeze, expected.freeze);
        assert_eq!(restored.merged_ranges, expected.merged_ranges);
        assert_eq!(restored.auto_filter, expected.auto_filter);
        assert_eq!(restored.sort, expected.sort);
        assert_eq!(
            restored.data_validations[0].range,
            expected.data_validations[0].range
        );
        assert_eq!(
            restored.data_validations[0].kind,
            expected.data_validations[0].kind
        );
        assert_eq!(
            restored.conditional_formats[0].range,
            expected.conditional_formats[0].range
        );
        assert_eq!(
            restored.conditional_formats[0].predicate,
            expected.conditional_formats[0].predicate
        );
        assert!(semantic_diff(&model, &imported).is_equivalent());
        let mut changed = imported.clone();
        changed.sheets[0].metadata.freeze.rows = 2;
        assert!(semantic_diff(&model, &changed)
            .differences
            .iter()
            .any(|path| path.contains("freeze.rows")));
    }

    #[test]
    fn named_ranges_are_typed_while_macros_and_drawings_enter_the_loss_report() {
        let parts = [
            (CONTENT_TYPES, "<Types/>"),
            (ROOT_RELS, "<Relationships/>"),
            (
                WORKBOOK,
                r#"<workbook xmlns:r="urn:r"><definedNames><definedName name="Total">Data!$A$1</definedName></definedNames><sheets><sheet name="Data" sheetId="1" r:id="rId1"/></sheets></workbook>"#,
            ),
            (
                WORKBOOK_RELS,
                r#"<Relationships><Relationship Id="rId1" Target="worksheets/sheet1.xml"/></Relationships>"#,
            ),
            (
                "xl/worksheets/sheet1.xml",
                "<worksheet><sheetData/></worksheet>",
            ),
            ("xl/vbaProject.bin", "macro bytes"),
            ("xl/drawings/drawing1.xml", "<drawing/>"),
            ("xl/charts/chart1.xml", "<chart/>"),
        ];
        let imported = read_xlsx_with_report(&zip_with_parts(&parts)).unwrap();
        assert_eq!(imported.model.metadata.named_ranges.len(), 1);
        assert_eq!(imported.model.metadata.named_ranges[0].name, "Total");
        let report = imported.unsupported_parts;
        assert!(report
            .iter()
            .any(|loss| loss.part.ends_with("vbaProject.bin")
                && loss.reason.contains("never be executed")));
        assert!(report
            .iter()
            .any(|loss| loss.part.contains("drawing") && loss.reason.contains("drawing")));
        assert!(report
            .iter()
            .any(|loss| loss.part.contains("chart") && loss.reason.contains("chart")));
    }

    #[test]
    fn export_preflight_reports_every_unmapped_model_feature() {
        let mut model = fixture_model();
        model.sheets[0].cells[0]
            .attrs
            .insert("vendor".into(), Value::Bool(true));
        model.sheets[0].cells[1].style = Some(CellStyle {
            number_format: Some("yyyy-mm-dd".into()),
            ..CellStyle::default()
        });
        model.sheets[0]
            .metadata
            .conditional_formats
            .push(oo_schema::ConditionalFormatRule {
                id: "styled-rule".into(),
                range: GridRange::default(),
                predicate: oo_schema::ConditionalPredicate::CellIs {
                    operator: oo_schema::ComparisonOperator::Equal,
                    value: Value::from(1),
                },
                style: CellStyle {
                    fill: Some(FillStyle {
                        foreground: Some("#FF0000".into()),
                        background: None,
                    }),
                    ..CellStyle::default()
                },
            });
        let report = inspect_xlsx_model(&model);
        assert_eq!(report.unsupported.len(), 1);
        assert!(write_xlsx(&model).is_err());
    }

    #[test]
    fn conditional_format_differential_styles_roundtrip_semantically() {
        let mut model = fixture_model();
        model.sheets[0]
            .metadata
            .conditional_formats
            .push(oo_schema::ConditionalFormatRule {
                id: "styled-rule".into(),
                range: GridRange {
                    start_row: 0,
                    start_column: 0,
                    end_row: 4,
                    end_column: 1,
                },
                predicate: oo_schema::ConditionalPredicate::CellIs {
                    operator: oo_schema::ComparisonOperator::Equal,
                    value: Value::from("high \"risk\""),
                },
                style: CellStyle {
                    number_format: Some("0.00".into()),
                    font: Some(FontStyle {
                        bold: true,
                        color: Some("#FFFFFF".into()),
                        ..FontStyle::default()
                    }),
                    fill: Some(FillStyle {
                        foreground: Some("#C00000".into()),
                        background: None,
                    }),
                    alignment: Some(AlignmentStyle {
                        horizontal: Some("center".into()),
                        vertical: Some("middle".into()),
                        wrap: true,
                    }),
                    borders: Some(CellBorders {
                        bottom: Some(CellBorderEdge {
                            style: Some("thin".into()),
                            color: Some("#111111".into()),
                        }),
                        ..CellBorders::default()
                    }),
                },
            });
        let imported = read_xlsx(&write_xlsx(&model).unwrap()).unwrap();
        let diff = semantic_diff(&model, &imported);
        assert!(diff.is_equivalent(), "{:?}", diff.differences);
    }
}
