//! Stable-id queries over a [`oo_schema::TableBlock`].
//!
//! The document engine owns mutations, while this module owns the read-side
//! projection used by those mutations and by future renderers.  It never
//! stores a second table model: a projection only borrows the canonical
//! `TableBlock` and translates stable ids into row/column indexes.

use std::collections::HashMap;

use oo_schema::{TableBlock, TableRange};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Stable-id selection for table commands.
///
/// Keeping this contract in the grid query module gives the command engine and
/// callers one selection vocabulary without coupling the query layer to
/// mutation implementation details.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum TableCellSelection {
    Cell {
        row_id: String,
        cell_id: String,
    },
    Range {
        start_row_id: String,
        end_row_id: String,
        start_column_id: String,
        end_column_id: String,
    },
    Row {
        row_id: String,
    },
    Column {
        column_id: String,
    },
    All,
}

/// Geometry-independent inclusive bounds of a stable-id range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridBounds {
    pub start_row: usize,
    pub end_row: usize,
    pub start_column: usize,
    pub end_column: usize,
}

impl GridBounds {
    pub const fn is_single_cell(self) -> bool {
        self.start_row == self.end_row && self.start_column == self.end_column
    }

    pub const fn contains(self, row: usize, column: usize) -> bool {
        row >= self.start_row
            && row <= self.end_row
            && column >= self.start_column
            && column <= self.end_column
    }

    pub const fn contains_bounds(self, other: Self) -> bool {
        self.start_row <= other.start_row
            && self.end_row >= other.end_row
            && self.start_column <= other.start_column
            && self.end_column >= other.end_column
    }

    pub const fn overlaps(self, other: Self) -> bool {
        self.start_row <= other.end_row
            && other.start_row <= self.end_row
            && self.start_column <= other.end_column
            && other.start_column <= self.end_column
    }
}

/// A resolved cell target.  Indexes are intentionally ephemeral; callers keep
/// stable ids in commands and resolve them against the current snapshot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridCellTarget {
    pub row: usize,
    pub column: usize,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum TableGridQueryError {
    #[error("不存在行 {0}")]
    MissingRow(String),
    #[error("不存在列 {0}")]
    MissingColumn(String),
    #[error("不存在单元格 {0}")]
    MissingCell(String),
    #[error("合并范围至少需要两个单元格")]
    SingleCellRange,
    #[error("范围的起止位置无效")]
    ReversedRange,
}

/// Borrowed, read-only projection over the canonical table block.
pub struct TableGridProjection<'a> {
    table: &'a TableBlock,
    row_indexes: HashMap<&'a str, usize>,
    column_indexes: HashMap<&'a str, usize>,
}

impl<'a> TableGridProjection<'a> {
    pub fn new(table: &'a TableBlock) -> Self {
        let row_indexes = table
            .rows
            .iter()
            .enumerate()
            .map(|(index, row)| (row.id.as_str(), index))
            .collect();
        let column_indexes = table
            .columns
            .iter()
            .enumerate()
            .map(|(index, column)| (column.id.as_str(), index))
            .collect();
        Self {
            table,
            row_indexes,
            column_indexes,
        }
    }

    pub fn table(&self) -> &'a TableBlock {
        self.table
    }

    pub fn row_index(&self, row_id: &str) -> Result<usize, TableGridQueryError> {
        self.row_indexes
            .get(row_id)
            .copied()
            .ok_or_else(|| TableGridQueryError::MissingRow(row_id.to_owned()))
    }

    pub fn column_index(&self, column_id: &str) -> Result<usize, TableGridQueryError> {
        self.column_indexes
            .get(column_id)
            .copied()
            .ok_or_else(|| TableGridQueryError::MissingColumn(column_id.to_owned()))
    }

    pub fn cell_index(
        &self,
        row_id: &str,
        cell_id: &str,
    ) -> Result<GridCellTarget, TableGridQueryError> {
        let row = self.row_index(row_id)?;
        let column = self.table.rows[row]
            .cells
            .iter()
            .position(|cell| cell.id == cell_id)
            .ok_or_else(|| TableGridQueryError::MissingCell(cell_id.to_owned()))?;
        Ok(GridCellTarget { row, column })
    }

    /// Resolve stable ids to inclusive, geometry-independent grid bounds.
    pub fn bounds_for_range(&self, range: &TableRange) -> Result<GridBounds, TableGridQueryError> {
        let start_row = self.row_index(&range.start_row_id)?;
        let end_row = self.row_index(&range.end_row_id)?;
        let start_column = self.column_index(&range.start_column_id)?;
        let end_column = self.column_index(&range.end_column_id)?;
        if start_row > end_row || start_column > end_column {
            return Err(TableGridQueryError::ReversedRange);
        }
        Ok(GridBounds {
            start_row,
            end_row,
            start_column,
            end_column,
        })
    }

    /// Resolve a merge/split range and reject a one-cell range.
    pub fn merge_bounds(&self, range: &TableRange) -> Result<GridBounds, TableGridQueryError> {
        let bounds = self.bounds_for_range(range)?;
        if bounds.is_single_cell() {
            return Err(TableGridQueryError::SingleCellRange);
        }
        Ok(bounds)
    }

    /// Return the merged range containing a stable-id cell, if any.
    pub fn merged_range_for_cell(
        &self,
        row_id: &str,
        column_id: &str,
    ) -> Result<Option<&'a TableRange>, TableGridQueryError> {
        let row = self.row_index(row_id)?;
        let column = self.column_index(column_id)?;
        Ok(self.table.merged_ranges.iter().find(|range| {
            self.bounds_for_range(range)
                .map(|bounds| bounds.contains(row, column))
                .unwrap_or(false)
        }))
    }

    /// Resolve a semantic selection to all affected cell indexes.
    pub fn selection_targets(
        &self,
        selection: &TableCellSelection,
    ) -> Result<Vec<GridCellTarget>, TableGridQueryError> {
        match selection {
            TableCellSelection::All => Ok(self
                .table
                .rows
                .iter()
                .enumerate()
                .flat_map(|(row, cells)| {
                    cells
                        .cells
                        .iter()
                        .enumerate()
                        .map(move |(column, _)| GridCellTarget { row, column })
                })
                .collect()),
            TableCellSelection::Row { row_id } => {
                let row = self.row_index(row_id)?;
                Ok(self.table.rows[row]
                    .cells
                    .iter()
                    .enumerate()
                    .map(|(column, _)| GridCellTarget { row, column })
                    .collect())
            }
            TableCellSelection::Column { column_id } => {
                let column = self.column_index(column_id)?;
                Ok((0..self.table.rows.len())
                    .map(|row| GridCellTarget { row, column })
                    .collect())
            }
            TableCellSelection::Cell { row_id, cell_id } => {
                Ok(vec![self.cell_index(row_id, cell_id)?])
            }
            TableCellSelection::Range {
                start_row_id,
                end_row_id,
                start_column_id,
                end_column_id,
            } => {
                let start_row = self.row_index(start_row_id)?;
                let end_row = self.row_index(end_row_id)?;
                let start_column = self.column_index(start_column_id)?;
                let end_column = self.column_index(end_column_id)?;
                let bounds = if start_row <= end_row && start_column <= end_column {
                    GridBounds {
                        start_row,
                        end_row,
                        start_column,
                        end_column,
                    }
                } else {
                    // Interactive selection can be anchored in either
                    // direction; normalise it without mutating the model.
                    GridBounds {
                        start_row: start_row.min(end_row),
                        end_row: start_row.max(end_row),
                        start_column: start_column.min(end_column),
                        end_column: start_column.max(end_column),
                    }
                };
                Ok((bounds.start_row..=bounds.end_row)
                    .flat_map(|row| {
                        (bounds.start_column..=bounds.end_column)
                            .map(move |column| GridCellTarget { row, column })
                    })
                    .collect())
            }
        }
    }

    /// Resolve any semantic selection into its inclusive rectangular extent.
    /// Mutation commands use this instead of deriving geometry from the DOM.
    pub fn selection_bounds(
        &self,
        selection: &TableCellSelection,
    ) -> Result<GridBounds, TableGridQueryError> {
        let last_row = self
            .table
            .rows
            .len()
            .checked_sub(1)
            .ok_or(TableGridQueryError::ReversedRange)?;
        let last_column = self
            .table
            .columns
            .len()
            .checked_sub(1)
            .ok_or(TableGridQueryError::ReversedRange)?;
        match selection {
            TableCellSelection::All => Ok(GridBounds {
                start_row: 0,
                end_row: last_row,
                start_column: 0,
                end_column: last_column,
            }),
            TableCellSelection::Row { row_id } => {
                let row = self.row_index(row_id)?;
                Ok(GridBounds {
                    start_row: row,
                    end_row: row,
                    start_column: 0,
                    end_column: last_column,
                })
            }
            TableCellSelection::Column { column_id } => {
                let column = self.column_index(column_id)?;
                Ok(GridBounds {
                    start_row: 0,
                    end_row: last_row,
                    start_column: column,
                    end_column: column,
                })
            }
            TableCellSelection::Cell { row_id, cell_id } => {
                let target = self.cell_index(row_id, cell_id)?;
                Ok(GridBounds {
                    start_row: target.row,
                    end_row: target.row,
                    start_column: target.column,
                    end_column: target.column,
                })
            }
            TableCellSelection::Range {
                start_row_id,
                end_row_id,
                start_column_id,
                end_column_id,
            } => {
                let start_row = self.row_index(start_row_id)?;
                let end_row = self.row_index(end_row_id)?;
                let start_column = self.column_index(start_column_id)?;
                let end_column = self.column_index(end_column_id)?;
                Ok(GridBounds {
                    start_row: start_row.min(end_row),
                    end_row: start_row.max(end_row),
                    start_column: start_column.min(end_column),
                    end_column: start_column.max(end_column),
                })
            }
        }
    }

    pub fn ranges_overlap(&self, left: &TableRange, right: &TableRange) -> bool {
        let Ok(left) = self.bounds_for_range(left) else {
            return false;
        };
        let Ok(right) = self.bounds_for_range(right) else {
            return false;
        };
        left.overlaps(right)
    }
}

#[cfg(test)]
mod tests {
    use oo_schema::{RichText, TableCell, TableCellStyle, TableColumn, TableRow};

    use super::*;

    fn table() -> TableBlock {
        TableBlock {
            columns: (0..3)
                .map(|index| TableColumn {
                    id: format!("column-{index}"),
                    width: None,
                })
                .collect(),
            rows: (0..3)
                .map(|row| TableRow {
                    id: format!("row-{row}"),
                    height: None,
                    cells: (0..3)
                        .map(|column| TableCell {
                            id: format!("cell-{row}-{column}"),
                            content: RichText::default(),
                            style: TableCellStyle::default(),
                        })
                        .collect(),
                })
                .collect(),
            merged_ranges: vec![TableRange {
                start_row_id: "row-0".into(),
                end_row_id: "row-1".into(),
                start_column_id: "column-1".into(),
                end_column_id: "column-2".into(),
            }],
        }
    }

    #[test]
    fn selection_queries_resolve_stable_ids_without_cloning_table() {
        let table = table();
        let projection = TableGridProjection::new(&table);
        assert_eq!(projection.row_index("row-2").unwrap(), 2);
        assert_eq!(projection.column_index("column-1").unwrap(), 1);
        assert_eq!(
            projection
                .selection_targets(&TableCellSelection::Cell {
                    row_id: "row-1".into(),
                    cell_id: "cell-1-2".into(),
                })
                .unwrap(),
            vec![GridCellTarget { row: 1, column: 2 }]
        );
        assert_eq!(
            projection
                .selection_targets(&TableCellSelection::Row {
                    row_id: "row-0".into(),
                })
                .unwrap()
                .len(),
            3
        );
    }

    #[test]
    fn merged_range_lookup_uses_stable_ids_and_geometry_free_bounds() {
        let table = table();
        let projection = TableGridProjection::new(&table);
        let range = projection
            .merged_range_for_cell("row-1", "column-2")
            .unwrap()
            .expect("cell should be merged");
        assert_eq!(range.start_row_id, "row-0");
        assert_eq!(
            projection.bounds_for_range(range).unwrap(),
            GridBounds {
                start_row: 0,
                end_row: 1,
                start_column: 1,
                end_column: 2,
            }
        );
        assert!(projection
            .merged_range_for_cell("row-2", "column-0")
            .unwrap()
            .is_none());
    }

    #[test]
    fn invalid_stable_ids_are_rejected_without_fallback_indexes() {
        let table = table();
        let projection = TableGridProjection::new(&table);
        assert_eq!(
            projection
                .selection_targets(&TableCellSelection::Column {
                    column_id: "unknown".into(),
                })
                .unwrap_err(),
            TableGridQueryError::MissingColumn("unknown".into())
        );
        assert_eq!(
            projection.cell_index("unknown", "cell-0-0").unwrap_err(),
            TableGridQueryError::MissingRow("unknown".into())
        );
        let one_cell = TableRange {
            start_row_id: "row-0".into(),
            end_row_id: "row-0".into(),
            start_column_id: "column-0".into(),
            end_column_id: "column-0".into(),
        };
        assert_eq!(
            projection.merge_bounds(&one_cell).unwrap_err(),
            TableGridQueryError::SingleCellRange
        );
    }
}
