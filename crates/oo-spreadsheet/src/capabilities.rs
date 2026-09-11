//! Spreadsheet-specific capability and projection contracts.
//!
//! The catalog is consumed by REST/SDK/MCP adapters. It is deliberately independent of the
//! Document command namespace: a spreadsheet cell is addressed by `sheetId,row,column`, and
//! metadata commands never masquerade as block updates.

use serde::{Deserialize, Serialize};
use serde_json::Map;

pub const SPREADSHEET_CAPABILITY_VERSION: u16 = 1;
pub const SPREADSHEET_NAMESPACE: &str = "spreadsheet";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SpreadsheetCapability {
    EditCell,
    EditFormula,
    FormatCells,
    FreezePane,
    Filter,
    Sort,
    ConditionalFormat,
    DataValidation,
    MergeCells,
    InsertRows,
    InsertColumns,
    DeleteRows,
    DeleteColumns,
    History,
    SparseViewport,
    XlsxImport,
    XlsxExport,
}

impl SpreadsheetCapability {
    pub const fn id(self) -> &'static str {
        match self {
            Self::EditCell => "spreadsheet.editCell",
            Self::EditFormula => "spreadsheet.editFormula",
            Self::FormatCells => "spreadsheet.formatCells",
            Self::FreezePane => "spreadsheet.freezePane",
            Self::Filter => "spreadsheet.filter",
            Self::Sort => "spreadsheet.sort",
            Self::ConditionalFormat => "spreadsheet.conditionalFormat",
            Self::DataValidation => "spreadsheet.dataValidation",
            Self::MergeCells => "spreadsheet.mergeCells",
            Self::InsertRows => "spreadsheet.insertRows",
            Self::InsertColumns => "spreadsheet.insertColumns",
            Self::DeleteRows => "spreadsheet.deleteRows",
            Self::DeleteColumns => "spreadsheet.deleteColumns",
            Self::History => "spreadsheet.history",
            Self::SparseViewport => "spreadsheet.sparseViewport",
            Self::XlsxImport => "spreadsheet.xlsxImport",
            Self::XlsxExport => "spreadsheet.xlsxExport",
        }
    }

    pub const fn command_type_id(self) -> Option<&'static str> {
        match self {
            Self::EditCell => Some("spreadsheet.setCell"),
            Self::EditFormula => Some("spreadsheet.setCell"),
            Self::FormatCells => Some("spreadsheet.setCellStyle"),
            Self::FreezePane | Self::Filter | Self::ConditionalFormat | Self::DataValidation => {
                Some("spreadsheet.setSheetMetadata")
            }
            Self::InsertRows => Some("spreadsheet.insertRows"),
            Self::InsertColumns => Some("spreadsheet.insertColumns"),
            Self::DeleteRows => Some("spreadsheet.deleteRows"),
            Self::DeleteColumns => Some("spreadsheet.deleteColumns"),
            Self::MergeCells => Some("spreadsheet.mergeCells"),
            Self::History => Some("spreadsheet.history"),
            Self::Sort => Some("spreadsheet.sortRange"),
            Self::SparseViewport | Self::XlsxImport | Self::XlsxExport => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpreadsheetCapabilityDescriptor {
    pub id: String,
    pub command_type_id: Option<String>,
    pub requires_revision: bool,
    pub supports_idempotency: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpreadsheetCapabilityCatalog {
    pub version: u16,
    pub namespace: String,
    pub capabilities: Vec<SpreadsheetCapabilityDescriptor>,
}

pub fn spreadsheet_capability_catalog() -> SpreadsheetCapabilityCatalog {
    let capabilities = [
        SpreadsheetCapability::EditCell,
        SpreadsheetCapability::EditFormula,
        SpreadsheetCapability::FormatCells,
        SpreadsheetCapability::FreezePane,
        SpreadsheetCapability::Filter,
        SpreadsheetCapability::Sort,
        SpreadsheetCapability::ConditionalFormat,
        SpreadsheetCapability::DataValidation,
        SpreadsheetCapability::MergeCells,
        SpreadsheetCapability::InsertRows,
        SpreadsheetCapability::InsertColumns,
        SpreadsheetCapability::DeleteRows,
        SpreadsheetCapability::DeleteColumns,
        SpreadsheetCapability::History,
        SpreadsheetCapability::SparseViewport,
        SpreadsheetCapability::XlsxImport,
        SpreadsheetCapability::XlsxExport,
    ]
    .into_iter()
    .map(|capability| SpreadsheetCapabilityDescriptor {
        id: capability.id().into(),
        command_type_id: capability.command_type_id().map(str::to_string),
        requires_revision: capability.command_type_id().is_some(),
        supports_idempotency: capability.command_type_id().is_some(),
    })
    .collect();
    SpreadsheetCapabilityCatalog {
        version: SPREADSHEET_CAPABILITY_VERSION,
        namespace: SPREADSHEET_NAMESPACE.into(),
        capabilities,
    }
}

/// 单个命令的 wire 身份与作用域（M0 能力契约：单一事实源）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpreadsheetCommandDescriptor {
    pub type_id: &'static str,
    /// 命令操作的实体域：sheet 级结构或 cell 级内容。
    pub scope: &'static str,
}

/// Spreadsheet 引擎命令注册表——`/api/capabilities` 与服务端 typeId 校验的
/// 唯一事实源。此列表必须与 `SpreadsheetCommand` 枚举一一对应；一致性由
/// `registry_entries_deserialize_into_real_commands` 测试强制（每个条目
/// 必须有可反序列化的样例 payload），由 `oo-server` 的 catalog 契约测试
/// 锁定 wire 面。
pub fn spreadsheet_command_registry() -> Vec<SpreadsheetCommandDescriptor> {
    use super::SpreadsheetCommand as C;
    let scope_of = |command: &C| match command {
        C::CreateSheet { .. }
        | C::RenameSheet { .. }
        | C::DeleteSheet { .. }
        | C::SetRowLayout { .. }
        | C::SetSheetMetadata { .. }
        | C::InsertRows { .. }
        | C::DeleteRows { .. }
        | C::InsertColumns { .. }
        | C::DeleteColumns { .. }
        | C::MergeCells { .. }
        | C::UnmergeCells { .. }
        | C::SortRange { .. }
        | C::SetFreezePane { .. }
        | C::SetAutoFilter { .. }
        | C::UpsertFilterColumn { .. }
        | C::ClearFilterColumn { .. }
        | C::SetRowDimensions { .. }
        | C::SetColumnDimensions { .. }
        | C::UpsertConditionalFormat { .. }
        | C::DeleteConditionalFormat { .. }
        | C::UpsertDataValidation { .. }
        | C::DeleteDataValidation { .. } => "spreadsheet.sheet",
        C::SetCell { .. }
        | C::SetCellStyle { .. }
        | C::ClearCell { .. }
        | C::FormatRange { .. }
        | C::ClearRange { .. }
        | C::ReplaceRange { .. }
        | C::PasteRange { .. }
        | C::FillRange { .. } => "spreadsheet.cell",
        C::SetCalculationMode { .. } => "spreadsheet.workbook",
    };
    // 枚举全部命令变体，编译器在此强制新变体必须登记。
    let commands = vec![
        C::SetRowLayout {
            sheet_id: String::new(),
            start_row: 0,
            end_row: 0,
            height: None,
            reset_height: false,
            hidden: Some(true),
        },
        C::CreateSheet {
            id: String::new(),
            name: String::new(),
        },
        C::RenameSheet {
            sheet_id: String::new(),
            name: String::new(),
        },
        C::DeleteSheet {
            sheet_id: String::new(),
        },
        C::SetCell {
            sheet_id: String::new(),
            row: 0,
            column: 0,
            value: None,
            formula: None,
            attrs: Map::new(),
        },
        C::SetCellStyle {
            sheet_id: String::new(),
            row: 0,
            column: 0,
            style: None,
        },
        C::SetSheetMetadata {
            sheet_id: String::new(),
            metadata: Default::default(),
        },
        C::ClearCell {
            sheet_id: String::new(),
            row: 0,
            column: 0,
        },
        C::InsertRows {
            sheet_id: String::new(),
            at: 0,
            count: 0,
        },
        C::DeleteRows {
            sheet_id: String::new(),
            at: 0,
            count: 0,
        },
        C::InsertColumns {
            sheet_id: String::new(),
            at: 0,
            count: 0,
        },
        C::DeleteColumns {
            sheet_id: String::new(),
            at: 0,
            count: 0,
        },
        C::MergeCells {
            sheet_id: String::new(),
            range: Default::default(),
        },
        C::UnmergeCells {
            sheet_id: String::new(),
            range: Default::default(),
        },
        C::SortRange {
            sheet_id: String::new(),
            range: Default::default(),
            keys: Vec::new(),
        },
        C::FormatRange {
            fields: None,
            row_pattern: None,
            sheet_id: String::new(),
            range: Default::default(),
            style: Default::default(),
        },
        C::ClearRange {
            sheet_id: String::new(),
            range: Default::default(),
            mode: Default::default(),
        },
        C::ReplaceRange {
            sheet_id: String::new(),
            range: Default::default(),
            search: String::new(),
            replace: String::new(),
            match_case: false,
        },
        C::PasteRange {
            sheet_id: String::new(),
            start_row: 0,
            start_column: 0,
            row_count: 1,
            column_count: 1,
            cells: Vec::new(),
            mode: Default::default(),
            source_origin: None,
        },
        C::FillRange {
            source_sheet_id: String::new(),
            source_range: Default::default(),
            destination_sheet_id: String::new(),
            destination_range: Default::default(),
            mode: Default::default(),
        },
        C::SetFreezePane {
            sheet_id: String::new(),
            rows: 0,
            columns: 0,
        },
        C::SetAutoFilter {
            sheet_id: String::new(),
            range: None,
        },
        C::UpsertFilterColumn {
            sheet_id: String::new(),
            column: 0,
            predicate: oo_schema::FilterPredicate::Contains(String::new()),
        },
        C::ClearFilterColumn {
            sheet_id: String::new(),
            column: None,
        },
        C::SetCalculationMode {
            calculation_mode: Default::default(),
        },
        C::SetRowDimensions {
            sheet_id: String::new(),
            rows: None,
        },
        C::SetColumnDimensions {
            sheet_id: String::new(),
            columns: None,
        },
        C::UpsertConditionalFormat {
            sheet_id: String::new(),
            rule: oo_schema::ConditionalFormatRule {
                id: String::new(),
                range: Default::default(),
                predicate: oo_schema::ConditionalPredicate::Formula(String::new()),
                style: Default::default(),
            },
        },
        C::DeleteConditionalFormat {
            sheet_id: String::new(),
            rule_id: String::new(),
        },
        C::UpsertDataValidation {
            sheet_id: String::new(),
            rule: oo_schema::DataValidationRule {
                id: String::new(),
                range: Default::default(),
                kind: oo_schema::DataValidationKind::CustomFormula(String::new()),
                allow_blank: false,
                error_message: None,
            },
        },
        C::DeleteDataValidation {
            sheet_id: String::new(),
            rule_id: String::new(),
        },
    ];
    commands
        .into_iter()
        .map(|command| {
            let type_id = match &command {
                C::CreateSheet { .. } => "spreadsheet.createSheet",
                C::RenameSheet { .. } => "spreadsheet.renameSheet",
                C::DeleteSheet { .. } => "spreadsheet.deleteSheet",
                C::SetCell { .. } => "spreadsheet.setCell",
                C::SetCellStyle { .. } => "spreadsheet.setCellStyle",
                C::SetRowLayout { .. } => "spreadsheet.setRowLayout",
                C::SetSheetMetadata { .. } => "spreadsheet.setSheetMetadata",
                C::ClearCell { .. } => "spreadsheet.clearCell",
                C::InsertRows { .. } => "spreadsheet.insertRows",
                C::DeleteRows { .. } => "spreadsheet.deleteRows",
                C::InsertColumns { .. } => "spreadsheet.insertColumns",
                C::DeleteColumns { .. } => "spreadsheet.deleteColumns",
                C::MergeCells { .. } => "spreadsheet.mergeCells",
                C::UnmergeCells { .. } => "spreadsheet.unmergeCells",
                C::SortRange { .. } => "spreadsheet.sortRange",
                C::FormatRange { .. } => "spreadsheet.formatRange",
                C::ClearRange { .. } => "spreadsheet.clearRange",
                C::ReplaceRange { .. } => "spreadsheet.replaceRange",
                C::PasteRange { .. } => "spreadsheet.pasteRange",
                C::FillRange { .. } => "spreadsheet.fillRange",
                C::SetFreezePane { .. } => "spreadsheet.setFreezePane",
                C::SetAutoFilter { .. } => "spreadsheet.setAutoFilter",
                C::UpsertFilterColumn { .. } => "spreadsheet.upsertFilterColumn",
                C::ClearFilterColumn { .. } => "spreadsheet.clearFilter",
                C::SetCalculationMode { .. } => "spreadsheet.setCalculationMode",
                C::SetRowDimensions { .. } => "spreadsheet.setRowDimensions",
                C::SetColumnDimensions { .. } => "spreadsheet.setColumnDimensions",
                C::UpsertConditionalFormat { .. } => "spreadsheet.upsertConditionalFormat",
                C::DeleteConditionalFormat { .. } => "spreadsheet.deleteConditionalFormat",
                C::UpsertDataValidation { .. } => "spreadsheet.upsertDataValidation",
                C::DeleteDataValidation { .. } => "spreadsheet.deleteDataValidation",
            };
            SpreadsheetCommandDescriptor {
                type_id,
                scope: scope_of(&command),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_is_stable_and_does_not_leak_document_names() {
        let catalog = spreadsheet_capability_catalog();
        assert_eq!(catalog.version, 1);
        assert_eq!(catalog.namespace, "spreadsheet");
        assert!(catalog
            .capabilities
            .iter()
            .any(|item| item.id == "spreadsheet.freezePane"));
        assert!(catalog
            .capabilities
            .iter()
            .all(|item| !item.id.contains("block")));
    }

    /// M0 门禁：registry 的每个条目必须真的反序列化为对应命令变体，且
    /// typeId 映射与命令一一对应。新增命令变体时若忘记登记或样例失配，
    /// 此测试立即失败。
    #[test]
    fn registry_entries_deserialize_into_real_commands() {
        use super::super::SpreadsheetCommand as C;
        let registry = spreadsheet_command_registry();
        // 每个变体一个最小合法 payload（serde camelCase wire 形态）。
        let samples: &[(&str, serde_json::Value)] = &[
            (
                "spreadsheet.setRowLayout",
                serde_json::json!({"type":"setRowLayout","sheetId":"s","startRow":0,"endRow":0,"hidden":true}),
            ),
            (
                "spreadsheet.createSheet",
                serde_json::json!({"type": "createSheet", "id": "s", "name": "n"}),
            ),
            (
                "spreadsheet.renameSheet",
                serde_json::json!({"type": "renameSheet", "sheetId": "s", "name": "n"}),
            ),
            (
                "spreadsheet.deleteSheet",
                serde_json::json!({"type": "deleteSheet", "sheetId": "s"}),
            ),
            (
                "spreadsheet.setCell",
                serde_json::json!({"type": "setCell", "sheetId": "s", "row": 0, "column": 0, "value": 1, "formula": null, "attrs": {}}),
            ),
            (
                "spreadsheet.setCellStyle",
                serde_json::json!({"type": "setCellStyle", "sheetId": "s", "row": 0, "column": 0, "style": null}),
            ),
            (
                "spreadsheet.setSheetMetadata",
                serde_json::json!({"type": "setSheetMetadata", "sheetId": "s", "metadata": {"visibility": "visible", "freeze": {"rows": 0, "columns": 0}, "conditionalFormats": [], "dataValidations": [], "mergedRanges": [], "media": []}}),
            ),
            (
                "spreadsheet.clearCell",
                serde_json::json!({"type": "clearCell", "sheetId": "s", "row": 0, "column": 0}),
            ),
            (
                "spreadsheet.insertRows",
                serde_json::json!({"type": "insertRows", "sheetId": "s", "at": 0, "count": 1}),
            ),
            (
                "spreadsheet.deleteRows",
                serde_json::json!({"type": "deleteRows", "sheetId": "s", "at": 0, "count": 1}),
            ),
            (
                "spreadsheet.insertColumns",
                serde_json::json!({"type": "insertColumns", "sheetId": "s", "at": 0, "count": 1}),
            ),
            (
                "spreadsheet.deleteColumns",
                serde_json::json!({"type": "deleteColumns", "sheetId": "s", "at": 0, "count": 1}),
            ),
            (
                "spreadsheet.mergeCells",
                serde_json::json!({"type": "mergeCells", "sheetId": "s", "range": {"startRow": 0, "startColumn": 0, "endRow": 1, "endColumn": 1}}),
            ),
            (
                "spreadsheet.unmergeCells",
                serde_json::json!({"type": "unmergeCells", "sheetId": "s", "range": {"startRow": 0, "startColumn": 0, "endRow": 1, "endColumn": 1}}),
            ),
            (
                "spreadsheet.sortRange",
                serde_json::json!({"type": "sortRange", "sheetId": "s", "range": {"startRow": 0, "startColumn": 0, "endRow": 1, "endColumn": 1}, "keys": [{"column": 0, "direction": "ascending"}]}),
            ),
            (
                "spreadsheet.formatRange",
                serde_json::json!({"type": "formatRange", "sheetId": "s", "range": {"startRow": 0, "startColumn": 0, "endRow": 1, "endColumn": 1}, "style": {}}),
            ),
            (
                "spreadsheet.clearRange",
                serde_json::json!({"type": "clearRange", "sheetId": "s", "range": {"startRow": 0, "startColumn": 0, "endRow": 1, "endColumn": 1}, "mode": "contents"}),
            ),
            (
                "spreadsheet.replaceRange",
                serde_json::json!({"type": "replaceRange", "sheetId": "s", "range": {"startRow": 0, "startColumn": 0, "endRow": 1, "endColumn": 1}, "search": "a", "replace": "b", "matchCase": false}),
            ),
            (
                "spreadsheet.pasteRange",
                serde_json::json!({"type": "pasteRange", "sheetId": "s", "startRow": 0, "startColumn": 0, "rowCount": 1, "columnCount": 1, "cells": [], "mode": "all"}),
            ),
            (
                "spreadsheet.fillRange",
                serde_json::json!({"type": "fillRange", "sourceSheetId": "s", "sourceRange": {"startRow": 0, "startColumn": 0, "endRow": 0, "endColumn": 0}, "destinationSheetId": "s", "destinationRange": {"startRow": 1, "startColumn": 0, "endRow": 1, "endColumn": 0}, "mode": "all"}),
            ),
            (
                "spreadsheet.setFreezePane",
                serde_json::json!({"type": "setFreezePane", "sheetId": "s", "rows": 1, "columns": 0}),
            ),
            (
                "spreadsheet.setAutoFilter",
                serde_json::json!({"type": "setAutoFilter", "sheetId": "s", "range": null}),
            ),
            (
                "spreadsheet.upsertFilterColumn",
                serde_json::json!({"type": "upsertFilterColumn", "sheetId": "s", "column": 0, "predicate": {"type": "contains", "value": "x"}}),
            ),
            (
                "spreadsheet.clearFilter",
                serde_json::json!({"type": "clearFilterColumn", "sheetId": "s", "column": null}),
            ),
            (
                "spreadsheet.setCalculationMode",
                serde_json::json!({"type": "setCalculationMode", "calculationMode": "manual"}),
            ),
            (
                "spreadsheet.setRowDimensions",
                serde_json::json!({"type": "setRowDimensions", "sheetId": "s", "rows": 100}),
            ),
            (
                "spreadsheet.setColumnDimensions",
                serde_json::json!({"type": "setColumnDimensions", "sheetId": "s", "columns": 50}),
            ),
            (
                "spreadsheet.upsertConditionalFormat",
                serde_json::json!({"type": "upsertConditionalFormat", "sheetId": "s", "rule": {"id": "cf", "range": {"startRow": 0, "startColumn": 0, "endRow": 1, "endColumn": 1}, "predicate": {"type": "formula", "value": "A1>0"}, "style": {}}}),
            ),
            (
                "spreadsheet.deleteConditionalFormat",
                serde_json::json!({"type": "deleteConditionalFormat", "sheetId": "s", "ruleId": "cf"}),
            ),
            (
                "spreadsheet.upsertDataValidation",
                serde_json::json!({"type": "upsertDataValidation", "sheetId": "s", "rule": {"id": "dv", "range": {"startRow": 0, "startColumn": 0, "endRow": 1, "endColumn": 1}, "kind": {"type": "wholeNumber", "value": {"min": 0, "max": 10}}, "allowBlank": true, "errorMessage": null}}),
            ),
            (
                "spreadsheet.deleteDataValidation",
                serde_json::json!({"type": "deleteDataValidation", "sheetId": "s", "ruleId": "dv"}),
            ),
        ];
        assert_eq!(registry.len(), samples.len(), "registry 与样例表长度不一致");
        for descriptor in &registry {
            let sample = samples
                .iter()
                .find(|(type_id, _)| *type_id == descriptor.type_id)
                .unwrap_or_else(|| panic!("registry 条目 {} 缺少反序列化样例", descriptor.type_id));
            let command: C = serde_json::from_value(sample.1.clone()).unwrap_or_else(|error| {
                panic!(
                    "registry 条目 {} 的样例反序列化失败：{error}",
                    descriptor.type_id
                )
            });
            // 反序列化出来的命令必须映射回同一个 typeId。
            let round_trip = match &command {
                C::CreateSheet { .. } => "spreadsheet.createSheet",
                C::RenameSheet { .. } => "spreadsheet.renameSheet",
                C::DeleteSheet { .. } => "spreadsheet.deleteSheet",
                C::SetCell { .. } => "spreadsheet.setCell",
                C::SetCellStyle { .. } => "spreadsheet.setCellStyle",
                C::SetRowLayout { .. } => "spreadsheet.setRowLayout",
                C::SetSheetMetadata { .. } => "spreadsheet.setSheetMetadata",
                C::ClearCell { .. } => "spreadsheet.clearCell",
                C::InsertRows { .. } => "spreadsheet.insertRows",
                C::DeleteRows { .. } => "spreadsheet.deleteRows",
                C::InsertColumns { .. } => "spreadsheet.insertColumns",
                C::DeleteColumns { .. } => "spreadsheet.deleteColumns",
                C::MergeCells { .. } => "spreadsheet.mergeCells",
                C::UnmergeCells { .. } => "spreadsheet.unmergeCells",
                C::SortRange { .. } => "spreadsheet.sortRange",
                C::FormatRange { .. } => "spreadsheet.formatRange",
                C::ClearRange { .. } => "spreadsheet.clearRange",
                C::ReplaceRange { .. } => "spreadsheet.replaceRange",
                C::PasteRange { .. } => "spreadsheet.pasteRange",
                C::FillRange { .. } => "spreadsheet.fillRange",
                C::SetFreezePane { .. } => "spreadsheet.setFreezePane",
                C::SetAutoFilter { .. } => "spreadsheet.setAutoFilter",
                C::UpsertFilterColumn { .. } => "spreadsheet.upsertFilterColumn",
                C::ClearFilterColumn { .. } => "spreadsheet.clearFilter",
                C::SetCalculationMode { .. } => "spreadsheet.setCalculationMode",
                C::SetRowDimensions { .. } => "spreadsheet.setRowDimensions",
                C::SetColumnDimensions { .. } => "spreadsheet.setColumnDimensions",
                C::UpsertConditionalFormat { .. } => "spreadsheet.upsertConditionalFormat",
                C::DeleteConditionalFormat { .. } => "spreadsheet.deleteConditionalFormat",
                C::UpsertDataValidation { .. } => "spreadsheet.upsertDataValidation",
                C::DeleteDataValidation { .. } => "spreadsheet.deleteDataValidation",
            };
            assert_eq!(round_trip, descriptor.type_id, "typeId 映射漂移");
            assert!(
                descriptor.scope == "spreadsheet.sheet"
                    || descriptor.scope == "spreadsheet.cell"
                    || descriptor.scope == "spreadsheet.workbook",
                "scope 必须是封闭词表"
            );
        }
    }
}
