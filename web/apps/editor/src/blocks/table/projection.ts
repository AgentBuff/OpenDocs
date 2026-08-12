import type { TableBlock, TableCell, TableColumn, TableRow } from "@open-office/schema/artifact";

/**
 * Read-only runtime index for a document table.
 *
 * The persisted TableBlock remains the only source of truth. This projection
 * holds references to rows/cells and indexes their stable ids; it never clones
 * or mutates the grid. Renderers can therefore resolve a cell, row or column
 * in O(1) without scattering positional lookups through the JSX tree.
 */
export interface TableCellProjection {
  readonly cell: TableCell;
  readonly row: TableRow;
  readonly rowIndex: number;
  readonly column: TableColumn;
  readonly columnIndex: number;
}

export class TableGridProjection {
  readonly table: TableBlock;
  readonly rows: readonly TableRow[];
  readonly columns: readonly TableColumn[];

  private readonly rowIndexes = new Map<string, number>();
  private readonly columnIndexes = new Map<string, number>();
  private readonly cells = new Map<string, TableCellProjection>();

  constructor(table: TableBlock) {
    this.table = table;
    this.rows = table.rows;
    this.columns = table.columns;
    table.rows.forEach((row, rowIndex) => {
      this.rowIndexes.set(row.id, rowIndex);
      row.cells.forEach((cell, columnIndex) => {
        const column = table.columns[columnIndex];
        if (!column) return;
        this.cells.set(cell.id, { cell, row, rowIndex, column, columnIndex });
      });
    });
    table.columns.forEach((column, columnIndex) => this.columnIndexes.set(column.id, columnIndex));
  }

  rowIndex(rowId: string): number {
    return this.rowIndexes.get(rowId) ?? -1;
  }

  columnIndex(columnId: string): number {
    return this.columnIndexes.get(columnId) ?? -1;
  }

  row(rowId: string): TableRow | null {
    const index = this.rowIndex(rowId);
    return index >= 0 ? this.rows[index] ?? null : null;
  }

  column(columnId: string): TableColumn | null {
    const index = this.columnIndex(columnId);
    return index >= 0 ? this.columns[index] ?? null : null;
  }

  cell(cellId: string): TableCellProjection | null {
    return this.cells.get(cellId) ?? null;
  }

  cellAt(rowId: string, columnId: string): TableCellProjection | null {
    const rowIndex = this.rowIndex(rowId);
    const columnIndex = this.columnIndex(columnId);
    if (rowIndex < 0 || columnIndex < 0) return null;
    const cell = this.rows[rowIndex]?.cells[columnIndex];
    return cell ? { cell, row: this.rows[rowIndex], rowIndex, column: this.columns[columnIndex], columnIndex } : null;
  }

  cellsInRow(rowId: string): readonly TableCell[] {
    return this.row(rowId)?.cells ?? [];
  }

  hasRow(rowId: string): boolean {
    return this.rowIndexes.has(rowId);
  }

  hasColumn(columnId: string): boolean {
    return this.columnIndexes.has(columnId);
  }
}

export function createTableGridProjection(table: TableBlock): TableGridProjection {
  return new TableGridProjection(table);
}
