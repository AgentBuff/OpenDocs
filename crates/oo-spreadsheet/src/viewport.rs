//! Bounded, sparse Grid projections for spreadsheet renderers.
//!
//! A viewport is a read-only request, not part of the persisted snapshot. The projection only
//! returns materialized cells in a half-open range, so a million-row sheet does not create a
//! million DOM/React nodes. Frozen panes are metadata and should be projected as separate ranges
//! by a renderer; this module intentionally never stores scroll position.

use oo_schema::{CellModel, CellStyle, SpreadsheetModel};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::CellAddress;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GridViewport {
    pub start_row: u32,
    pub end_row: u32,
    pub start_column: u32,
    pub end_column: u32,
}

impl GridViewport {
    /// Builds a half-open viewport `[start, end)`. Empty windows are rejected so a renderer cannot
    /// accidentally treat an invalid scroll measurement as the whole sheet.
    pub fn new(
        start_row: u32,
        end_row: u32,
        start_column: u32,
        end_column: u32,
    ) -> Result<Self, ViewportError> {
        if start_row >= end_row || start_column >= end_column {
            return Err(ViewportError::EmptyRange);
        }
        Ok(Self {
            start_row,
            end_row,
            start_column,
            end_column,
        })
    }

    pub fn contains(&self, row: u32, column: u32) -> bool {
        row >= self.start_row
            && row < self.end_row
            && column >= self.start_column
            && column < self.end_column
    }

    pub fn cell_count(&self) -> u64 {
        u64::from(self.end_row - self.start_row) * u64::from(self.end_column - self.start_column)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewportCell {
    pub address: CellAddress,
    #[serde(default)]
    pub value: Option<Value>,
    #[serde(default)]
    pub formula: Option<String>,
    #[serde(default)]
    pub style: Option<CellStyle>,
}

impl ViewportCell {
    fn from_cell(sheet_id: &str, cell: &CellModel) -> Self {
        Self {
            address: CellAddress {
                sheet_id: sheet_id.into(),
                row: cell.row,
                column: cell.column,
            },
            value: cell.value.clone(),
            formula: cell.formula.clone(),
            style: cell.style.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SparseGridViewport {
    pub sheet_id: String,
    pub viewport: GridViewport,
    pub cells: Vec<ViewportCell>,
    pub materialized_cell_count: usize,
}

impl SparseGridViewport {
    pub fn project(
        model: &SpreadsheetModel,
        sheet_id: &str,
        viewport: GridViewport,
    ) -> Result<Self, ViewportError> {
        let sheet = model
            .sheets
            .iter()
            .find(|sheet| sheet.id == sheet_id)
            .ok_or_else(|| ViewportError::MissingSheet(sheet_id.into()))?;
        let mut cells = sheet
            .cells
            .iter()
            .filter(|cell| viewport.contains(cell.row, cell.column))
            .map(|cell| ViewportCell::from_cell(sheet_id, cell))
            .collect::<Vec<_>>();
        cells.sort_by_key(|cell| (cell.address.row, cell.address.column));
        let materialized_cell_count = cells.len();
        Ok(Self {
            sheet_id: sheet_id.into(),
            viewport,
            cells,
            materialized_cell_count,
        })
    }

    pub fn is_sparse(&self) -> bool {
        (self.materialized_cell_count as u64) < self.viewport.cell_count()
    }
}

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ViewportError {
    #[error("viewport 不能是空范围")]
    EmptyRange,
    #[error("worksheet {0} 不存在")]
    MissingSheet(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use oo_schema::{CellModel, SheetModel, SpreadsheetModel};

    #[test]
    fn projects_only_materialized_cells_in_a_bounded_window() {
        let model = SpreadsheetModel {
            sheets: vec![SheetModel {
                id: "sheet-1".into(),
                name: "Sheet 1".into(),
                cells: vec![
                    CellModel {
                        row: 2,
                        column: 3,
                        value: Some(42.into()),
                        ..CellModel::default()
                    },
                    CellModel {
                        row: 100,
                        column: 100,
                        value: Some(7.into()),
                        ..CellModel::default()
                    },
                ],
                ..SheetModel::default()
            }],
            ..SpreadsheetModel::default()
        };
        let viewport = GridViewport::new(0, 10, 0, 10).unwrap();
        let projection = SparseGridViewport::project(&model, "sheet-1", viewport).unwrap();
        assert_eq!(projection.cells.len(), 1);
        assert_eq!(projection.cells[0].address.row, 2);
        assert!(projection.is_sparse());
    }

    #[test]
    fn rejects_empty_window_and_unknown_sheet() {
        assert_eq!(
            GridViewport::new(0, 0, 0, 1),
            Err(ViewportError::EmptyRange)
        );
        let viewport = GridViewport::new(0, 1, 0, 1).unwrap();
        assert_eq!(
            SparseGridViewport::project(&SpreadsheetModel::default(), "missing", viewport),
            Err(ViewportError::MissingSheet("missing".into()))
        );
    }
}
