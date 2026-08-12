//! Spreadsheet-specific capability and projection contracts.
//!
//! The catalog is consumed by REST/SDK/MCP adapters. It is deliberately independent of the
//! Document command namespace: a spreadsheet cell is addressed by `sheetId,row,column`, and
//! metadata commands never masquerade as block updates.

use serde::{Deserialize, Serialize};

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
            Self::FreezePane
            | Self::Filter
            | Self::Sort
            | Self::ConditionalFormat
            | Self::DataValidation => Some("spreadsheet.setSheetMetadata"),
            Self::MergeCells => Some("spreadsheet.setSheetMetadata"),
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
}
