import type { CellModel, CellStyle, GridRange } from "@open-office/schema/artifact";

/** A 0-based cell coordinate. */
export interface CellCoordinate {
  row: number;
  column: number;
}

/** Renderer selection model. Row/column/all are distinct kinds so a header
 * click never has to fake a cell anchor to be highlighted. */
export type CellSelection =
  | { kind: "cells"; anchor: CellCoordinate; focus: CellCoordinate }
  | { kind: "row"; row: number; endRow?: number }
  | { kind: "column"; column: number }
  | { kind: "all" };

export interface GridBounds {
  rows: number;
  columns: number;
}

/** The inclusive range a selection highlights, or `null` for an empty selection. */
function rawSelectionRange(
  selection: CellSelection | null,
  bounds: GridBounds,
): GridRange | null {
  if (!selection) return null;
  switch (selection.kind) {
    case "cells":
      return {
        startRow: Math.min(selection.anchor.row, selection.focus.row),
        startColumn: Math.min(selection.anchor.column, selection.focus.column),
        endRow: Math.max(selection.anchor.row, selection.focus.row),
        endColumn: Math.max(selection.anchor.column, selection.focus.column),
      };
    case "row":
      return { startRow: Math.min(selection.row, selection.endRow ?? selection.row), endRow: Math.max(selection.row, selection.endRow ?? selection.row), startColumn: 0, endColumn: bounds.columns - 1 };
    case "column":
      return { startRow: 0, endRow: bounds.rows - 1, startColumn: selection.column, endColumn: selection.column };
    case "all":
      return { startRow: 0, endRow: bounds.rows - 1, startColumn: 0, endColumn: bounds.columns - 1 };
  }
}

/** Expands a selection to whole merged cells, including merges reached by expansion. */
export function selectionRange(selection: CellSelection | null, bounds: GridBounds, merges: readonly GridRange[] = []): GridRange | null {
  const range = rawSelectionRange(selection, bounds);
  if (!range) return null;
  let changed = true;
  while (changed) {
    changed = false;
    for (const merge of merges) {
      if (merge.startRow > range.endRow || merge.endRow < range.startRow || merge.startColumn > range.endColumn || merge.endColumn < range.startColumn) continue;
      const next = { startRow: Math.min(range.startRow, merge.startRow), endRow: Math.max(range.endRow, merge.endRow), startColumn: Math.min(range.startColumn, merge.startColumn), endColumn: Math.max(range.endColumn, merge.endColumn) };
      if (next.startRow !== range.startRow || next.endRow !== range.endRow || next.startColumn !== range.startColumn || next.endColumn !== range.endColumn) {
        Object.assign(range, next);
        changed = true;
      }
    }
  }
  return range;
}

export function mergedRangeAt(point: CellCoordinate, merges: readonly GridRange[]): GridRange | undefined {
  return merges.find((range) => point.row >= range.startRow && point.row <= range.endRow && point.column >= range.startColumn && point.column <= range.endColumn);
}

export function mergedAnchor(point: CellCoordinate, merges: readonly GridRange[]): CellCoordinate {
  const merge = mergedRangeAt(point, merges);
  return merge ? { row: merge.startRow, column: merge.startColumn } : point;
}

/** One render coordinate per visible cell/merged rectangle, even when its anchor is offscreen. */
export function visibleGridCells(window: GridRange, merges: readonly GridRange[]): CellCoordinate[] {
  const intersecting = merges.filter((range) => range.startRow <= window.endRow && range.endRow >= window.startRow && range.startColumn <= window.endColumn && range.endColumn >= window.startColumn);
  const cells = new Map<string, CellCoordinate>();
  for (let row = window.startRow; row <= window.endRow; row++) {
    for (let column = window.startColumn; column <= window.endColumn; column++) {
      const anchor = mergedAnchor({ row, column }, intersecting);
      cells.set(`${anchor.row}:${anchor.column}`, anchor);
    }
  }
  return [...cells.values()];
}

/** The focused cell for the formula bar / navigation. Row/column/all selections
 * have no single focus, so the caller keeps a `lastAnchor` for keyboard travel. */
export function activeCell(selection: CellSelection | null): CellCoordinate | null {
  if (selection && selection.kind === "cells") return selection.anchor;
  return null;
}

export function clampCoordinate(row: number, column: number, bounds: GridBounds): CellCoordinate {
  return {
    row: Math.max(0, Math.min(bounds.rows - 1, row)),
    column: Math.max(0, Math.min(bounds.columns - 1, column)),
  };
}

export type NavKey = "up" | "down" | "left" | "right" | "home" | "end" | "pageUp" | "pageDown" | "tab" | "shiftTab";

/** Moves the active cell from `from` in the given direction. Returns the focused
 * coordinate (single-cell selection). Always collapses to one cell. */
export function moveCell(from: CellCoordinate, key: NavKey, bounds: GridBounds, pageSize = 20, merges: readonly GridRange[] = []): CellCoordinate {
  const merge = mergedRangeAt(from, merges);
  const anchor = mergedAnchor(from, merges);
  const origin = merge && (key === "right" || key === "tab") ? { row: anchor.row, column: merge.endColumn }
    : merge && key === "down" ? { row: merge.endRow, column: anchor.column } : anchor;
  return mergedAnchor(moveUnmergedCell(origin, key, bounds, pageSize), merges);
}

function moveUnmergedCell(from: CellCoordinate, key: NavKey, bounds: GridBounds, pageSize: number): CellCoordinate {
  const pageRows = () => clampCoordinate(from.row - pageSize, from.column, bounds);
  switch (key) {
    case "up": return clampCoordinate(from.row - 1, from.column, bounds);
    case "down": return clampCoordinate(from.row + 1, from.column, bounds);
    case "left": return clampCoordinate(from.row, from.column - 1, bounds);
    case "right": return clampCoordinate(from.row, from.column + 1, bounds);
    case "home": return clampCoordinate(from.row, 0, bounds);
    case "end": return clampCoordinate(from.row, bounds.columns - 1, bounds);
    case "pageUp": return pageRows();
    case "pageDown": return clampCoordinate(from.row + pageSize, from.column, bounds);
    case "tab": return clampCoordinate(from.row, from.column + 1, bounds);
    case "shiftTab": return clampCoordinate(from.row, from.column - 1, bounds);
  }
}

/** The cells covered by a range selection (used by copy/paste/clear). */
export function rangeCoordinates(range: GridRange): CellCoordinate[] {
  const coordinates: CellCoordinate[] = [];
  for (let row = range.startRow; row <= range.endRow; row += 1) {
    for (let column = range.startColumn; column <= range.endColumn; column += 1) {
      coordinates.push({ row, column });
    }
  }
  return coordinates;
}

export interface ClipboardGrid {
  startRow: number;
  startColumn: number;
  /** row-major values; `undefined` = empty cell. */
  values: Array<Array<unknown | undefined>>;
}

/** A renderer-neutral, immutable snapshot of one populated clipboard cell.
 * Coordinates are relative to the copied range so the projection can be
 * pasted at any destination without retaining a SheetModel reference. */
export interface SpreadsheetClipboardCellProjection {
  readonly rowOffset: number;
  readonly columnOffset: number;
  readonly value?: unknown;
  readonly formula?: string | null;
  readonly attrs: Readonly<Record<string, unknown>>;
  readonly style?: CellStyle | null;
}

/** The smallest spreadsheet payload needed by paste commands. */
export interface SpreadsheetClipboardProjection {
  readonly rowCount: number;
  readonly columnCount: number;
  readonly cells: ReadonlyArray<SpreadsheetClipboardCellProjection>;
  readonly sourceOrigin?: CellCoordinate;
}

/** Builds a clipboard projection by visiting only coordinates in `range`.
 * The lookup is normally backed by the Studio's per-snapshot cell index, so a
 * small copy does not scan a large sparse worksheet. */
export function projectClipboardRange(
  range: GridRange,
  findCell: (row: number, column: number) => CellModel | null,
): SpreadsheetClipboardProjection {
  const cells: SpreadsheetClipboardCellProjection[] = [];
  for (let row = range.startRow; row <= range.endRow; row += 1) {
    for (let column = range.startColumn; column <= range.endColumn; column += 1) {
      const cell = findCell(row, column);
      if (!cell) continue;
      cells.push({
        rowOffset: row - range.startRow,
        columnOffset: column - range.startColumn,
        ...(Object.prototype.hasOwnProperty.call(cell, "value") ? { value: cloneClipboardData(cell.value) } : {}),
        ...(Object.prototype.hasOwnProperty.call(cell, "formula") ? { formula: cell.formula } : {}),
        attrs: cloneClipboardRecord(cell.attrs),
        ...(Object.prototype.hasOwnProperty.call(cell, "style") ? { style: cloneCellStyle(cell.style) } : {}),
      });
    }
  }
  return {
    rowCount: range.endRow - range.startRow + 1,
    columnCount: range.endColumn - range.startColumn + 1,
    cells,
    sourceOrigin: { row: range.startRow, column: range.startColumn },
  };
}

/** Serializes clipboard content as standards-friendly TSV. Formulas are
 * exported as formulas and values retain their textual representation. */
export function serializeSpreadsheetClipboard(projection: SpreadsheetClipboardProjection): string {
  const cells = new Map(projection.cells.map((cell) => [`${cell.rowOffset}:${cell.columnOffset}`, cell]));
  const rows: string[] = [];
  for (let row = 0; row < projection.rowCount; row += 1) {
    const columns: string[] = [];
    for (let column = 0; column < projection.columnCount; column += 1) {
      const cell = cells.get(`${row}:${column}`);
      const raw = cell?.formula ?? clipboardValueText(cell?.value);
      columns.push(quoteClipboardField(raw, "\t"));
    }
    rows.push(columns.join("\t"));
  }
  return rows.join("\n");
}

/** Parses external TSV or CSV without assigning it a source coordinate.
 * Consequently formulas from external apps are stored exactly as supplied;
 * only formulas copied inside this workbook receive relative translation. */
export function parseSpreadsheetClipboard(text: string): SpreadsheetClipboardProjection | null {
  const normalized = text.replace(/\r\n?/g, "\n");
  if (!normalized) return null;
  const delimiter = normalized.includes("\t") ? "\t" : ",";
  const matrix = parseDelimitedMatrix(normalized, delimiter);
  if (matrix.length > 1 && matrix.at(-1)?.length === 1 && matrix.at(-1)?.[0] === "") matrix.pop();
  if (matrix.length === 0) return null;
  const columnCount = Math.max(...matrix.map((row) => row.length));
  if (columnCount === 0) return null;
  const cells: SpreadsheetClipboardCellProjection[] = [];
  matrix.forEach((row, rowOffset) => row.forEach((raw, columnOffset) => {
    if (raw === "") return;
    cells.push({
      rowOffset,
      columnOffset,
      ...(raw.startsWith("=") ? { formula: raw } : { value: clipboardTextValue(raw) }),
      attrs: {},
      style: null,
    });
  }));
  return { rowCount: matrix.length, columnCount, cells };
}

function clipboardValueText(value: unknown): string {
  if (value == null) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  return JSON.stringify(value);
}

function clipboardTextValue(raw: string): unknown {
  if (/^-?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?$/i.test(raw)) return Number(raw);
  if (/^(true|false)$/i.test(raw)) return raw.toLowerCase() === "true";
  return raw;
}

function quoteClipboardField(value: string, delimiter: string): string {
  return value.includes(delimiter) || /[\n\r"]/.test(value) ? `"${value.replace(/"/g, '""')}"` : value;
}

function parseDelimitedMatrix(text: string, delimiter: string): string[][] {
  const rows: string[][] = [[""]];
  let quoted = false;
  for (let index = 0; index < text.length; index += 1) {
    const character = text[index];
    if (quoted) {
      if (character === '"' && text[index + 1] === '"') { rows.at(-1)![rows.at(-1)!.length - 1] += '"'; index += 1; }
      else if (character === '"') quoted = false;
      else rows.at(-1)![rows.at(-1)!.length - 1] += character;
    } else if (character === '"') quoted = true;
    else if (character === delimiter) rows.at(-1)!.push("");
    else if (character === "\n") rows.push([""]);
    else rows.at(-1)![rows.at(-1)!.length - 1] += character;
  }
  return rows;
}

function cloneClipboardRecord(value: Readonly<Record<string, unknown>>): Record<string, unknown> {
  return Object.fromEntries(Object.entries(value).map(([key, entry]) => [key, cloneClipboardData(entry)]));
}

function cloneClipboardData(value: unknown): unknown {
  if (Array.isArray(value)) return value.map(cloneClipboardData);
  if (value !== null && typeof value === "object") {
    return cloneClipboardRecord(value as Readonly<Record<string, unknown>>);
  }
  return value;
}

function cloneCellStyle(style: CellStyle | null | undefined): CellStyle | null | undefined {
  if (style == null) return style;
  return {
    numberFormat: style.numberFormat,
    font: style.font ? { ...style.font } : null,
    fill: style.fill ? { ...style.fill } : null,
    alignment: style.alignment ? { ...style.alignment } : null,
    borders: style.borders ? {
      top: style.borders.top ? { ...style.borders.top } : null,
      bottom: style.borders.bottom ? { ...style.borders.bottom } : null,
      left: style.borders.left ? { ...style.borders.left } : null,
      right: style.borders.right ? { ...style.borders.right } : null,
    } : null,
  };
}

/** Copies the sparse range into a row-major clipboard snapshot. */
export function copyRange(model: ReadonlyArray<{ row: number; column: number; value?: unknown }>, range: GridRange): ClipboardGrid {
  const width = range.endColumn - range.startColumn + 1;
  const height = range.endRow - range.startRow + 1;
  const values: Array<Array<unknown | undefined>> = Array.from({ length: height }, () => Array<unknown | undefined>(width).fill(undefined));
  for (const cell of model) {
    if (cell.row >= range.startRow && cell.row <= range.endRow && cell.column >= range.startColumn && cell.column <= range.endColumn) {
      values[cell.row - range.startRow][cell.column - range.startColumn] = cell.value;
    }
  }
  return { startRow: range.startRow, startColumn: range.startColumn, values };
}

/** Computes the destination coordinates when pasting `clipboard` anchored at
 * `origin`, respecting the grid bounds (no wrap-around). */
export function pasteTargets(clipboard: ClipboardGrid, origin: CellCoordinate, bounds: GridBounds): Array<{ row: number; column: number; value: unknown | undefined }> {
  const targets: Array<{ row: number; column: number; value: unknown | undefined }> = [];
  for (let r = 0; r < clipboard.values.length; r += 1) {
    const row = origin.row + r;
    if (row >= bounds.rows) break;
    for (let c = 0; c < clipboard.values[r].length; c += 1) {
      const column = origin.column + c;
      if (column >= bounds.columns) break;
      targets.push({ row, column, value: clipboard.values[r][c] });
    }
  }
  return targets;
}
