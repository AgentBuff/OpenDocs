import type { TableBlock, TableRange } from "@open-office/schema/artifact";

/**
 * Table interaction state is intentionally separate from the persisted table
 * payload.  The document owns row/column ids; the view owns only a semantic
 * selection of those ids.  This keeps selection stable when a transaction
 * inserts or removes a row/column before the active one.
 */
export type TableSelection =
  | { kind: "cell"; rowId: string; cellId: string }
  | { kind: "range"; startRowId: string; endRowId: string; startColumnId: string; endColumnId: string }
  | { kind: "row"; id: string }
  | { kind: "column"; id: string }
  | { kind: "all" };

export interface TableRowGeometry {
  id: string;
  top: number;
  height: number;
}

export interface TableColumnGeometry {
  id: string;
  left: number;
  width: number;
}

export interface TableGeometry {
  tableTop: number;
  tableLeft: number;
  tableWidth: number;
  rowBoundaries: number[];
  columnBoundaries: number[];
  rows: TableRowGeometry[];
  columns: TableColumnGeometry[];
}

export const TABLE_RANGE_DRAG_THRESHOLD = 4;

/**
 * A cell pointer gesture starts as native content editing. It is promoted to
 * a rectangular table selection only after it has crossed into another cell
 * and moved far enough to be intentional. Text drags that start on inline
 * content therefore remain native DOM selections.
 */
export function shouldPromotePointerToTableRange({
  canStartRange,
  sameCell,
  startX,
  startY,
  currentX,
  currentY,
}: {
  canStartRange: boolean;
  sameCell: boolean;
  startX: number;
  startY: number;
  currentX: number;
  currentY: number;
}): boolean {
  if (!canStartRange || sameCell) return false;
  return Math.hypot(currentX - startX, currentY - startY) >= TABLE_RANGE_DRAG_THRESHOLD;
}

/**
 * A document table row boundary controls the row above it. Moving the
 * boundary changes only that row's height; following rows keep their own
 * heights and are naturally reflowed by table layout.
 */
export function resizeTableRowHeight(startHeight: number, pointerDelta: number, minimum = 34): number {
  return Math.max(minimum, startHeight + pointerDelta);
}

export interface TableContextTarget {
  x: number;
  y: number;
  selection: TableSelection;
}

export interface TableFormatState {
  bold: boolean;
  italic: boolean;
  underline: boolean;
  strikethrough: boolean;
  fontSize: number | null;
  textColor: string | null;
  highlightColor: string | null;
  fillColor: string | null;
  horizontalAlign: "left" | "center" | "right" | null;
  verticalAlign: "top" | "middle" | "bottom" | null;
}

function indexBounds(table: TableBlock, range: TableRange): { rows: [number, number]; columns: [number, number] } | null {
  const startRow = table.rows.findIndex((row) => row.id === range.startRowId);
  const endRow = table.rows.findIndex((row) => row.id === range.endRowId);
  const startColumn = table.columns.findIndex((column) => column.id === range.startColumnId);
  const endColumn = table.columns.findIndex((column) => column.id === range.endColumnId);
  if ([startRow, endRow, startColumn, endColumn].some((index) => index < 0)) return null;
  return {
    rows: [Math.min(startRow, endRow), Math.max(startRow, endRow)],
    columns: [Math.min(startColumn, endColumn), Math.max(startColumn, endColumn)],
  };
}

function boundsOverlap(
  left: { rows: [number, number]; columns: [number, number] },
  right: { rows: [number, number]; columns: [number, number] },
): boolean {
  return left.rows[0] <= right.rows[1]
    && right.rows[0] <= left.rows[1]
    && left.columns[0] <= right.columns[1]
    && right.columns[0] <= left.columns[1];
}

function boundsContain(
  outer: { rows: [number, number]; columns: [number, number] },
  inner: { rows: [number, number]; columns: [number, number] },
): boolean {
  return outer.rows[0] <= inner.rows[0]
    && outer.rows[1] >= inner.rows[1]
    && outer.columns[0] <= inner.columns[0]
    && outer.columns[1] >= inner.columns[1];
}

function rangeFromBounds(
  table: TableBlock,
  bounds: { rows: [number, number]; columns: [number, number] },
): TableRange {
  return {
    startRowId: table.rows[bounds.rows[0]].id,
    endRowId: table.rows[bounds.rows[1]].id,
    startColumnId: table.columns[bounds.columns[0]].id,
    endColumnId: table.columns[bounds.columns[1]].id,
  };
}

/**
 * Expands a rectangular selection until it contains every merged region it
 * touches. A merged cell is indivisible in an office grid: keeping only part
 * of it in a visual range makes the highlight lie and makes a later merge
 * command ambiguous.
 */
export function normalizeTableRange(table: TableBlock, range: TableRange): TableRange | null {
  const initialBounds = indexBounds(table, range);
  if (!initialBounds) return null;
  const bounds = {
    rows: [...initialBounds.rows] as [number, number],
    columns: [...initialBounds.columns] as [number, number],
  };
  let expanded = true;
  while (expanded) {
    expanded = false;
    for (const mergedRange of table.mergedRanges) {
      const mergedBounds = indexBounds(table, mergedRange);
      if (!mergedBounds || !boundsOverlap(bounds, mergedBounds) || boundsContain(bounds, mergedBounds)) continue;
      bounds.rows[0] = Math.min(bounds.rows[0], mergedBounds.rows[0]);
      bounds.rows[1] = Math.max(bounds.rows[1], mergedBounds.rows[1]);
      bounds.columns[0] = Math.min(bounds.columns[0], mergedBounds.columns[0]);
      bounds.columns[1] = Math.max(bounds.columns[1], mergedBounds.columns[1]);
      expanded = true;
    }
  }
  return rangeFromBounds(table, bounds);
}

/** Normalize only rectangular pointer/keyboard ranges; row/column controls retain their own semantics. */
export function normalizeTableSelection(table: TableBlock, selection: TableSelection): TableSelection {
  if (selection.kind !== "range") return selection;
  const range = normalizeTableRange(table, selection);
  return range ? { kind: "range", ...range } : selection;
}

/**
 * Converts every multi-cell view selection into the canonical, forward
 * ordered range accepted by the document engine.  The editor keeps stable
 * ids in its selection state; this helper is the only place where a view
 * selection is projected to a persisted command range.
 */
export function tableRangeForSelection(table: TableBlock, selection: TableSelection | null): TableRange | null {
  if (!selection || table.rows.length === 0 || table.columns.length === 0) return null;

  let rowIndexes: [number, number];
  let columnIndexes: [number, number];
  if (selection.kind === "cell") return null;
  if (selection.kind === "all") {
    rowIndexes = [0, table.rows.length - 1];
    columnIndexes = [0, table.columns.length - 1];
  } else if (selection.kind === "row") {
    const rowIndex = table.rows.findIndex((row) => row.id === selection.id);
    if (rowIndex < 0) return null;
    rowIndexes = [rowIndex, rowIndex];
    columnIndexes = [0, table.columns.length - 1];
  } else if (selection.kind === "column") {
    const columnIndex = table.columns.findIndex((column) => column.id === selection.id);
    if (columnIndex < 0) return null;
    rowIndexes = [0, table.rows.length - 1];
    columnIndexes = [columnIndex, columnIndex];
  } else {
    const bounds = indexBounds(table, selection);
    if (!bounds) return null;
    rowIndexes = bounds.rows;
    columnIndexes = bounds.columns;
  }

  return rangeFromBounds(table, { rows: rowIndexes, columns: columnIndexes });
}

/**
 * Canonical range for a merge command. The engine accepts a rectangle only
 * when it contains each existing merged cell it touches, so normalize here
 * before both toolbar enablement and command dispatch.
 */
export function mergeableTableRange(table: TableBlock, selection: TableSelection | null): TableRange | null {
  const range = tableRangeForSelection(table, selection);
  return range ? normalizeTableRange(table, range) : null;
}

function cellColumnId(table: TableBlock, rowId: string, cellId: string): string | null {
  const row = table.rows.find((candidate) => candidate.id === rowId);
  const cellIndex = row?.cells.findIndex((cell) => cell.id === cellId) ?? -1;
  return cellIndex >= 0 ? table.columns[cellIndex]?.id ?? null : null;
}

/**
 * Finds the persisted merge range represented by a view selection. A cell
 * selection resolves only when it is the visible anchor of a merge; a range
 * selection must cover exactly one persisted range. Keeping this resolution
 * in the interaction model prevents the renderer from manufacturing indexes
 * or dispatching a split command that the engine will reject.
 */
export function mergedRangeForSelection(table: TableBlock, selection: TableSelection | null): TableRange | null {
  if (!selection) return null;
  if (selection.kind === "cell") {
    const columnId = cellColumnId(table, selection.rowId, selection.cellId);
    if (!columnId) return null;
    return table.mergedRanges.find((range) => range.startRowId === selection.rowId && range.startColumnId === columnId) ?? null;
  }
  const range = mergeableTableRange(table, selection);
  const selectedBounds = range ? indexBounds(table, range) : null;
  if (!selectedBounds) return null;
  return table.mergedRanges.find((range) => {
    const bounds = indexBounds(table, range);
    return bounds
      && bounds.rows[0] === selectedBounds.rows[0]
      && bounds.rows[1] === selectedBounds.rows[1]
      && bounds.columns[0] === selectedBounds.columns[0]
      && bounds.columns[1] === selectedBounds.columns[1];
  }) ?? null;
}

/** A range is mergeable only when it spans multiple cells and is not already one persisted merge. */
export function canMergeTableSelection(table: TableBlock, selection: TableSelection | null): boolean {
  const commandRange = mergeableTableRange(table, selection);
  if (!commandRange) return false;
  const selectedBounds = indexBounds(table, commandRange);
  if (!selectedBounds) return false;
  if (selectedBounds.rows[0] === selectedBounds.rows[1] && selectedBounds.columns[0] === selectedBounds.columns[1]) return false;
  if (table.mergedRanges.some((range) => {
    const bounds = indexBounds(table, range);
    return bounds
      && bounds.rows[0] === selectedBounds.rows[0]
      && bounds.rows[1] === selectedBounds.rows[1]
      && bounds.columns[0] === selectedBounds.columns[0]
      && bounds.columns[1] === selectedBounds.columns[1];
  })) return false;
  return !table.mergedRanges.some((range) => {
    const bounds = indexBounds(table, range);
    return bounds !== null && boundsOverlap(bounds, selectedBounds) && !boundsContain(selectedBounds, bounds);
  });
}

/**
 * Returns whether a semantic table selection covers more than one cell.
 *
 * A focused cell is deliberately not enough to show the table formatting
 * toolbar: the cell editor owns that interaction until the user selects text
 * or expands the selection. Row/column/all selections use the canonical grid
 * dimensions rather than DOM geometry so the rule remains stable for merged
 * cells and during a resize.
 */
export function tableSelectionSpansMultipleCells(table: TableBlock, selection: TableSelection | null): boolean {
  if (!selection) return false;
  if (selection.kind === "cell") return false;
  if (selection.kind === "row") return table.columns.length > 1;
  if (selection.kind === "column") return table.rows.length > 1;
  if (selection.kind === "all") return table.rows.length * table.columns.length > 1;
  const bounds = indexBounds(table, selection);
  if (!bounds) return false;
  const rowCount = bounds.rows[1] - bounds.rows[0] + 1;
  const columnCount = bounds.columns[1] - bounds.columns[0] + 1;
  return rowCount * columnCount > 1;
}

function commonValue<T>(values: readonly T[]): T | null {
  if (values.length === 0) return null;
  const first = values[0];
  return values.every((value) => Object.is(value, first)) ? first ?? null : null;
}

/** Computes the common formatting state for a stable cell/range selection. */
export function tableSelectionFormatState(table: TableBlock, selection: TableSelection | null): TableFormatState {
  const cells = table.rows.flatMap((row) => row.cells.filter((cell, columnIndex) => {
    const columnId = table.columns[columnIndex]?.id;
    return columnId ? selectionIncludesCell(table, selection, row.id, columnId, cell.id) : false;
  }));
  const runStyle = (key: string) => cells.flatMap((cell) => cell.content.runs.map((run) => run.style[key as keyof typeof run.style]));
  const commonString = (key: string) => {
    const value = commonValue(runStyle(key));
    return typeof value === "string" ? value : null;
  };
  const booleanActive = (key: string) => {
    const values = runStyle(key);
    return values.length > 0 && values.every((value) => value === true);
  };
  const fontSizeValue = commonValue(runStyle("fontSize"));
  const styleValues = <T extends string>(key: "fillColor" | "horizontalAlign" | "verticalAlign") => {
    const values = cells.map((cell) => cell.style?.[key]);
    const value = commonValue(values);
    return typeof value === "string" ? value as T : null;
  };
  return {
    bold: booleanActive("bold"),
    italic: booleanActive("italic"),
    underline: booleanActive("underline"),
    strikethrough: booleanActive("strikethrough"),
    fontSize: typeof fontSizeValue === "number" ? fontSizeValue : null,
    textColor: commonString("color"),
    highlightColor: commonString("highlight"),
    fillColor: styleValues("fillColor"),
    horizontalAlign: styleValues("horizontalAlign"),
    verticalAlign: styleValues("verticalAlign"),
  };
}

/**
 * Computes formatting state for a native text range inside one table cell.
 * Cell presentation (fill/alignment) still comes from the containing cell,
 * while inline marks are reduced only across intersecting persisted runs.
 */
export function tableCellTextSelectionFormatState(
  table: TableBlock,
  selection: { rowId: string; cellId: string; start: number; end: number } | null,
): TableFormatState | null {
  if (!selection) return null;
  const row = table.rows.find((candidate) => candidate.id === selection.rowId);
  const cell = row?.cells.find((candidate) => candidate.id === selection.cellId);
  if (!cell || selection.start >= selection.end) return null;
  const runs = cell.content.runs.filter((run) => run.end > selection.start && run.start < selection.end);
  const values = <K extends keyof typeof runs[number]["style"]>(key: K) => runs.map((run) => run.style[key]);
  const commonString = (key: "color" | "highlight") => {
    const value = commonValue(values(key));
    return typeof value === "string" ? value : null;
  };
  const fontSize = commonValue(values("fontSize"));
  return {
    bold: runs.length > 0 && values("bold").every((value) => value === true),
    italic: runs.length > 0 && values("italic").every((value) => value === true),
    underline: runs.length > 0 && values("underline").every((value) => value === true),
    strikethrough: runs.length > 0 && values("strikethrough").every((value) => value === true),
    fontSize: typeof fontSize === "number" ? fontSize : null,
    textColor: commonString("color"),
    highlightColor: commonString("highlight"),
    fillColor: cell.style?.fillColor ?? null,
    horizontalAlign: cell.style?.horizontalAlign ?? null,
    verticalAlign: cell.style?.verticalAlign ?? null,
  };
}

export function selectionIncludesCell(
  table: TableBlock,
  selection: TableSelection | null,
  rowId: string,
  columnId: string,
  cellId?: string,
): boolean {
  if (!selection) return false;
  if (selection.kind === "all") return true;
  if (selection.kind === "cell") return selection.rowId === rowId && selection.cellId === cellId;
  if (selection.kind === "range") {
    const startRow = table.rows.findIndex((row) => row.id === selection.startRowId);
    const endRow = table.rows.findIndex((row) => row.id === selection.endRowId);
    const startColumn = table.columns.findIndex((column) => column.id === selection.startColumnId);
    const endColumn = table.columns.findIndex((column) => column.id === selection.endColumnId);
    const currentRow = table.rows.findIndex((row) => row.id === rowId);
    const currentColumn = table.columns.findIndex((column) => column.id === columnId);
    if ([startRow, endRow, startColumn, endColumn, currentRow, currentColumn].some((index) => index < 0)) return false;
    return currentRow >= Math.min(startRow, endRow) && currentRow <= Math.max(startRow, endRow)
      && currentColumn >= Math.min(startColumn, endColumn) && currentColumn <= Math.max(startColumn, endColumn);
  }
  if (selection.kind === "row") return selection.id === rowId;
  return selection.id === columnId;
}

/**
 * Resolves the semantic selection targeted by a cell context-menu gesture.
 * Right-clicking inside an existing range/row/column/all selection must keep
 * that selection so every menu command continues to address the user's
 * visible target. A gesture outside the selection starts a new cell target.
 */
export function tableContextSelectionForCell(
  table: TableBlock,
  selection: TableSelection | null,
  rowId: string,
  columnId: string,
  cellId: string,
): TableSelection {
  if (selectionIncludesCell(table, selection, rowId, columnId, cellId)) {
    return selection as TableSelection;
  }
  return { kind: "cell", rowId, cellId };
}

export function rowIndexForSelection(table: TableBlock, selection: TableSelection): number | null {
  if (selection.kind !== "row") return null;
  const index = table.rows.findIndex((row) => row.id === selection.id);
  return index >= 0 ? index : null;
}

export function columnIndexForSelection(table: TableBlock, selection: TableSelection): number | null {
  if (selection.kind !== "column") return null;
  const index = table.columns.findIndex((column) => column.id === selection.id);
  return index >= 0 ? index : null;
}

export function selectionStillExists(table: TableBlock, selection: TableSelection | null): boolean {
  if (!selection || selection.kind === "all") return selection !== null;
  if (selection.kind === "cell") return table.rows.some((row) => row.id === selection.rowId && row.cells.some((cell) => cell.id === selection.cellId));
  if (selection.kind === "range") {
    return table.rows.some((row) => row.id === selection.startRowId)
      && table.rows.some((row) => row.id === selection.endRowId)
      && table.columns.some((column) => column.id === selection.startColumnId)
      && table.columns.some((column) => column.id === selection.endColumnId);
  }
  return selection.kind === "row"
    ? table.rows.some((row) => row.id === selection.id)
    : table.columns.some((column) => column.id === selection.id);
}

/** Returns the DOM span for a merged anchor, or null for covered cells. */
export function mergedCellProjection(
  table: TableBlock,
  rowId: string,
  columnId: string,
): { rowSpan: number; colSpan: number } | null | undefined {
  const rowIndex = table.rows.findIndex((row) => row.id === rowId);
  const columnIndex = table.columns.findIndex((column) => column.id === columnId);
  if (rowIndex < 0 || columnIndex < 0) return undefined;
  for (const range of table.mergedRanges) {
    const startRow = table.rows.findIndex((row) => row.id === range.startRowId);
    const endRow = table.rows.findIndex((row) => row.id === range.endRowId);
    const startColumn = table.columns.findIndex((column) => column.id === range.startColumnId);
    const endColumn = table.columns.findIndex((column) => column.id === range.endColumnId);
    if (startRow < 0 || endRow < 0 || startColumn < 0 || endColumn < 0) continue;
    if (rowIndex < startRow || rowIndex > endRow || columnIndex < startColumn || columnIndex > endColumn) continue;
    if (rowIndex === startRow && columnIndex === startColumn) {
      return { rowSpan: endRow - startRow + 1, colSpan: endColumn - startColumn + 1 };
    }
    return null;
  }
  return undefined;
}

/**
 * Returns whether a grid boundary is inside a persisted merged range.
 *
 * Resize handles operate on the whole table row/column.  A boundary that is
 * covered by a rowSpan/colSpan is therefore not a legal resize target: the
 * browser would draw a splitter through the merged cell even though there is
 * no corresponding persisted edge.  Keep this geometry rule in the table
 * interaction model so the renderer never infers it from DOM row/cell counts.
 */
export function tableBoundaryCrossesMerge(
  table: TableBlock,
  axis: "row" | "column",
  boundaryIndex: number,
): boolean {
  const size = axis === "row" ? table.rows.length : table.columns.length;
  if (!Number.isInteger(boundaryIndex) || boundaryIndex <= 0 || boundaryIndex >= size) return false;
  return table.mergedRanges.some((range) => {
    const startId = axis === "row" ? range.startRowId : range.startColumnId;
    const endId = axis === "row" ? range.endRowId : range.endColumnId;
    const start = axis === "row"
      ? table.rows.findIndex((row) => row.id === startId)
      : table.columns.findIndex((column) => column.id === startId);
    const end = axis === "row"
      ? table.rows.findIndex((row) => row.id === endId)
      : table.columns.findIndex((column) => column.id === endId);
    if (start < 0 || end < 0) return false;
    const [from, to] = start <= end ? [start, end] : [end, start];
    return from < boundaryIndex && boundaryIndex <= to;
  });
}

export function tableSelectionText(table: TableBlock, selection: TableSelection): string {
  if (selection.kind === "cell") {
    return table.rows.find((row) => row.id === selection.rowId)?.cells.find((cell) => cell.id === selection.cellId)?.content.text ?? "";
  }
  if (selection.kind === "row") {
    const row = table.rows.find((candidate) => candidate.id === selection.id);
    return row?.cells.map((cell) => cell.content.text).join("\t") ?? "";
  }
  if (selection.kind === "range") {
    const rowIndexes = [table.rows.findIndex((row) => row.id === selection.startRowId), table.rows.findIndex((row) => row.id === selection.endRowId)];
    const columnIndexes = [table.columns.findIndex((column) => column.id === selection.startColumnId), table.columns.findIndex((column) => column.id === selection.endColumnId)];
    if ([...rowIndexes, ...columnIndexes].some((index) => index < 0)) return "";
    const [fromRow, toRow] = rowIndexes[0] <= rowIndexes[1] ? rowIndexes : [rowIndexes[1], rowIndexes[0]];
    const [fromColumn, toColumn] = columnIndexes[0] <= columnIndexes[1] ? columnIndexes : [columnIndexes[1], columnIndexes[0]];
    return table.rows.slice(fromRow, toRow + 1).map((row) => row.cells.slice(fromColumn, toColumn + 1).map((cell) => cell.content.text).join("\t")).join("\n");
  }
  if (selection.kind === "column") {
    const columnIndex = table.columns.findIndex((column) => column.id === selection.id);
    return columnIndex < 0
      ? ""
      : table.rows.map((row) => row.cells[columnIndex]?.content.text ?? "").join("\n");
  }
  return table.rows.map((row) => row.cells.map((cell) => cell.content.text).join("\t")).join("\n");
}

export function emptyTableGeometry(): TableGeometry {
  return {
    tableTop: 0,
    tableLeft: 0,
    tableWidth: 0,
    rowBoundaries: [],
    columnBoundaries: [],
    rows: [],
    columns: [],
  };
}

/** Keep DOM measurement in one place so renderers only consume a stable grid projection. */
export function measureTableGeometry(
  wrapRect: DOMRect,
  tableRect: DOMRect,
  rowRects: readonly DOMRect[],
  columnRects: readonly DOMRect[],
  rowIds: readonly string[],
  columnIds: readonly string[],
): TableGeometry {
  const rowBoundaries = rowRects.length > 0
    ? [rowRects[0].top - wrapRect.top, ...rowRects.map((rect) => rect.bottom - wrapRect.top)]
    : [];
  const columnBoundaries = columnRects.length > 0
    ? [columnRects[0].left - wrapRect.left, ...columnRects.map((rect) => rect.right - wrapRect.left)]
    : [];
  return {
    tableTop: tableRect.top - wrapRect.top,
    tableLeft: tableRect.left - wrapRect.left,
    tableWidth: tableRect.width,
    rowBoundaries,
    columnBoundaries,
    rows: rowRects.map((rect, index) => ({
      id: rowIds[index] ?? `row-${index}`,
      top: rect.top - wrapRect.top,
      height: rect.height,
    })),
    columns: columnRects.map((rect, index) => ({
      id: columnIds[index] ?? `column-${index}`,
      left: rect.left - wrapRect.left,
      width: rect.width,
    })),
  };
}
