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
    ArtifactEnvelope, ArtifactPayload, CellModel, CellStyle, DateSystem, FreezePane, GridRange,
    SheetMetadata, SheetModel, SheetVisibility, SpreadsheetMetadata, SpreadsheetModel,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct XlsxUnsupportedPart {
    pub part: String,
    pub reason: String,
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
    Ok(read_xlsx_with_report(bytes)?.model)
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
    let (date_system, calculation_mode) = parse_workbook_settings(&workbook_xml)?;
    let relationships = parse_relationships(&workbook_rels_xml)?;

    let shared_strings = if all_parts.iter().any(|part| part == SHARED_STRINGS) {
        parse_shared_strings(&read_part(&mut archive, SHARED_STRINGS)?)?
    } else {
        Vec::new()
    };
    let styles = if all_parts.iter().any(|part| part == STYLES) {
        parse_styles(&read_part(&mut archive, STYLES)?)?
    } else {
        Vec::new()
    };
    let style_unsupported_parts = if all_parts.iter().any(|part| part == STYLES) {
        style_unsupported_parts(&read_part(&mut archive, STYLES)?)
    } else {
        Vec::new()
    };

    let mut sheets = Vec::with_capacity(workbook.len());
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
        let mut id = format!("sheet-{}", sheet.sheet_id);
        if id == "sheet-" {
            id = format!("sheet-{}", position + 1);
        }
        while sheets.iter().any(|existing: &SheetModel| existing.id == id) {
            id.push_str("-copy");
        }
        let mut metadata = parse_worksheet_metadata(&xml)?;
        metadata.visibility = sheet.visibility;
        let cells = parse_worksheet(&xml, &shared_strings, &styles)?;
        sheets.push(SheetModel {
            id,
            name: sheet.name.clone(),
            cells,
            metadata,
        });
    }

    let model = SpreadsheetModel {
        metadata: SpreadsheetMetadata {
            calculation_mode,
            date_system,
            ..SpreadsheetMetadata::default()
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
    Ok(XlsxImportResult {
        model,
        ignored_parts,
        unsupported_parts,
    })
}

/// Export the supported SpreadsheetModel range as a valid minimal XLSX package.
pub fn write_xlsx(model: &SpreadsheetModel) -> Result<Vec<u8>, XlsxError> {
    ArtifactEnvelope::new("xlsx-adapter", ArtifactPayload::Spreadsheet(model.clone()))
        .validate()?;
    if model.sheets.is_empty() {
        return Err(XlsxError::UnsupportedFeature(
            "XLSX 至少需要一个 worksheet".into(),
        ));
    }
    for sheet in &model.sheets {
        for cell in &sheet.cells {
            if let Some(format) = cell
                .style
                .as_ref()
                .and_then(|style| style.number_format.as_deref())
            {
                if builtin_num_format_id(format).is_none() {
                    return Err(XlsxError::UnsupportedFeature(format!(
                        "sheet {} cell ({},{}) 使用未支持的自定义 number format {}",
                        sheet.id, cell.row, cell.column, format
                    )));
                }
            }
        }
        if sheet.metadata.sort.is_some()
            || !sheet.metadata.conditional_formats.is_empty()
            || !sheet.metadata.data_validations.is_empty()
            || !sheet.metadata.media.is_empty()
            || sheet
                .metadata
                .auto_filter
                .as_ref()
                .is_some_and(|filter| !filter.columns.is_empty())
        {
            return Err(XlsxError::UnsupportedFeature(format!(
                "sheet {} 包含当前 XLSX adapter 尚未可逆导出的元数据",
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
    let mut styles = Vec::<CellStyle>::new();
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
    let has_styles = !styles.is_empty();

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
        parts.push((STYLES.to_string(), styles_xml(&styles)));
    }
    for (index, sheet) in model.sheets.iter().enumerate() {
        parts.push((
            format!("xl/worksheets/sheet{}.xml", index + 1),
            worksheet_xml(sheet, &shared_lookup, &style_lookup)?,
        ));
    }

    let mut archive = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, content) in parts {
        archive.start_file(name, SimpleFileOptions::default())?;
        archive.write_all(content.as_bytes())?;
    }
    Ok(archive.finish()?.into_inner())
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
) -> Result<(DateSystem, oo_schema::CalculationMode), XlsxError> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut date_system = DateSystem::Excel1900;
    let mut calculation_mode = oo_schema::CalculationMode::Automatic;
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
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok((date_system, calculation_mode))
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

/// Parse the small, stable subset of `styles.xml` needed by the canonical model. Fonts/fills are
/// deliberately left to a later style capability; number formats are lossless and are carried as
/// a typed `CellStyle` rather than an opaque XML index.
fn parse_styles(bytes: &[u8]) -> Result<Vec<CellStyle>, XlsxError> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut custom_formats = BTreeMap::<u32, String>::new();
    let mut xfs = Vec::<u32>::new();
    let mut in_cell_xfs = false;
    loop {
        match reader.read_event_into(&mut buffer)? {
            Event::Start(event) if event.local_name().as_ref() == b"cellXfs" => {
                in_cell_xfs = true;
            }
            Event::End(event) if event.local_name().as_ref() == b"cellXfs" => {
                in_cell_xfs = false;
            }
            Event::Empty(event) | Event::Start(event)
                if event.local_name().as_ref() == b"numFmt" =>
            {
                if let (Some(id), Some(code)) =
                    (attr(&event, "numFmtId"), attr(&event, "formatCode"))
                {
                    if let Ok(id) = id.parse() {
                        custom_formats.insert(id, code);
                    }
                }
            }
            Event::Empty(event) | Event::Start(event)
                if in_cell_xfs && event.local_name().as_ref() == b"xf" =>
            {
                let id = attr(&event, "numFmtId")
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(0);
                xfs.push(id);
            }
            Event::Eof => break,
            _ => {}
        }
        buffer.clear();
    }
    Ok(xfs
        .into_iter()
        .map(|id| CellStyle {
            number_format: Some(
                custom_formats
                    .get(&id)
                    .cloned()
                    .unwrap_or_else(|| builtin_num_format(id).to_string()),
            )
            .filter(|format| !format.is_empty()),
            ..CellStyle::default()
        })
        .collect())
}

fn style_unsupported_parts(bytes: &[u8]) -> Vec<XlsxUnsupportedPart> {
    let xml = String::from_utf8_lossy(bytes);
    [
        ("fonts", "font family/weight/color mapping is not modeled"),
        ("fills", "cell fill mapping is not modeled"),
        ("borders", "cell border mapping is not modeled"),
    ]
    .into_iter()
    .filter(|(element, _)| {
        let open = format!("<{element}");
        let Some(start) = xml.find(&open) else {
            return false;
        };
        let Some(end) = xml[start..].find('>') else {
            return false;
        };
        let header = &xml[start..start + end];
        let count_gt_one = header
            .split("count=\"")
            .nth(1)
            .and_then(|value| value.split('"').next())
            .and_then(|value| value.parse::<u32>().ok())
            .is_some_and(|count| count > 1);
        count_gt_one || xml[start..].contains(&format!("<{element}><"))
    })
    .map(|(element, reason)| XlsxUnsupportedPart {
        part: format!("{STYLES}#{element}"),
        reason: reason.to_string(),
    })
    .collect()
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

fn parse_worksheet_metadata(bytes: &[u8]) -> Result<SheetMetadata, XlsxError> {
    let mut reader = Reader::from_reader(bytes);
    reader.config_mut().trim_text(true);
    let mut buffer = Vec::new();
    let mut metadata = SheetMetadata::default();
    loop {
        match reader.read_event_into(&mut buffer)? {
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
    Ok(metadata)
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
                }
                text_field = Some(CellTextField::Formula)
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
                let raw = current
                    .take()
                    .ok_or(XlsxError::InvalidPart("worksheet 结束了未知 cell".into()))?;
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
        .filter(|formula_type| *formula_type != "normal")
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
        style: raw.style_index.and_then(|index| styles.get(index).cloned()),
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
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><workbook xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main" xmlns:r="http://schemas.openxmlformats.org/officeDocument/2006/relationships"><sheets>"#,
    );
    if model.metadata.date_system == DateSystem::Excel1904 {
        xml.push_str(r#"<workbookPr date1904="1"/>"#);
    }
    if model.metadata.calculation_mode == oo_schema::CalculationMode::Manual {
        xml.push_str(r#"<calcPr calcMode="manual"/>"#);
    }
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
    xml.push_str("</sheets></workbook>");
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
) -> Result<String, XlsxError> {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><worksheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><sheetData>"#,
    );
    let mut current_row: Option<u32> = None;
    for cell in &sheet.cells {
        if current_row != Some(cell.row) {
            if current_row.is_some() {
                xml.push_str("</row>");
            }
            current_row = Some(cell.row);
            let _ = write!(xml, "<row r=\"{}\">", cell.row + 1);
        }
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
    if current_row.is_some() {
        xml.push_str("</row>");
    }
    xml.push_str("</sheetData></worksheet>");
    Ok(xml)
}

fn styles_xml(styles: &[CellStyle]) -> String {
    let mut xml = String::from(
        r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?><styleSheet xmlns="http://schemas.openxmlformats.org/spreadsheetml/2006/main"><fonts count="1"><font/></fonts><fills count="1"><fill/></fills><borders count="1"><border/></borders><cellStyleXfs count="1"><xf/></cellStyleXfs><cellXfs count=""#,
    );
    let _ = write!(xml, "{}\">", styles.len());
    for style in styles {
        let format_id = style
            .number_format
            .as_deref()
            .and_then(builtin_num_format_id)
            .unwrap_or(0);
        let _ = write!(
            xml,
            "<xf numFmtId=\"{}\" applyNumberFormat=\"1\"/>",
            format_id
        );
    }
    xml.push_str(r#"</cellXfs><cellStyles count="1"><cellStyle name="Normal" xfId="0" builtinId="0"/></cellStyles></styleSheet>"#);
    xml
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
    fn unsupported_shared_formula_is_reported() {
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
                r#"<worksheet><sheetData><row><c r="A1"><f t="shared" si="0">A2+B2</f><v>3</v></c></row></sheetData></worksheet>"#,
            ),
        ];
        let bytes = zip_with_parts(&parts);
        assert!(matches!(
            read_xlsx(&bytes),
            Err(XlsxError::UnsupportedFeature(message)) if message.contains("公式类型 shared")
        ));
    }
}
