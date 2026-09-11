import type { GridRange, SortDirection } from "@open-office/schema/artifact";

/**
 * Typed Spreadsheet semantic command surface.
 *
 * Every variant mirrors one Rust `SpreadsheetCommand` serde-tagged variant
 * (`type` tag, camelCase fields) plus the protocol `typeId` envelope. The
 * union is the single contract between the grid renderer, toolbars and the
 * transaction API: a command that is not expressible here cannot be sent,
 * and a Rust command without a mirror here is a schema drift that the
 * contract tests catch.
 */

export interface SpreadsheetSetCellPayload {
  type: "setCell";
  sheetId: string;
  row: number;
  column: number;
  value: unknown;
  formula: string | null;
  attrs: Record<string, unknown>;
}

export interface SpreadsheetClearCellPayload {
  type: "clearCell";
  sheetId: string;
  row: number;
  column: number;
}

export interface SpreadsheetSetCellStylePayload {
  type: "setCellStyle";
  sheetId: string;
  row: number;
  column: number;
  style: import("@open-office/schema/artifact").CellStyle;
}

export interface SpreadsheetSetSheetMetadataPayload {
  type: "setSheetMetadata";
  sheetId: string;
  metadata: import("@open-office/schema/artifact").SheetMetadata;
}

export interface SpreadsheetCreateSheetPayload {
  type: "createSheet";
  id: string;
  name: string;
}

export interface SpreadsheetRenameSheetPayload {
  type: "renameSheet";
  sheetId: string;
  name: string;
}

export interface SpreadsheetDeleteSheetPayload {
  type: "deleteSheet";
  sheetId: string;
}

export interface SpreadsheetInsertRowsPayload {
  type: "insertRows";
  sheetId: string;
  at: number;
  count: number;
}

export interface SpreadsheetDeleteRowsPayload {
  type: "deleteRows";
  sheetId: string;
  at: number;
  count: number;
}

export interface SpreadsheetInsertColumnsPayload {
  type: "insertColumns";
  sheetId: string;
  at: number;
  count: number;
}

export interface SpreadsheetDeleteColumnsPayload {
  type: "deleteColumns";
  sheetId: string;
  at: number;
  count: number;
}

export interface SpreadsheetMergeCellsPayload {
  type: "mergeCells";
  sheetId: string;
  range: GridRange;
}

export interface SpreadsheetUnmergeCellsPayload {
  type: "unmergeCells";
  sheetId: string;
  range: GridRange;
}

export interface SpreadsheetSortRangePayload {
  type: "sortRange";
  sheetId: string;
  range: GridRange;
  keys: Array<{ column: number; direction: SortDirection }>;
}

export type CellStyleField =
  | "numberFormat" | "fontFamily" | "fontSize" | "fontBold" | "fontItalic"
  | "fontUnderline" | "fontStrikethrough" | "fontColor" | "fillBackground"
  | "horizontalAlignment" | "verticalAlignment" | "wrap" | "borders"
  | "borderTop" | "borderBottom" | "borderLeft" | "borderRight" | "outerBorders";

export interface SpreadsheetFormatRangePayload {
  fields?: CellStyleField[];
  rowPattern?: "firstRow" | "alternatingRows";
  type: "formatRange";
  sheetId: string;
  range: GridRange;
  style: import("@open-office/schema/artifact").CellStyle;
}

export interface SpreadsheetClearRangePayload {
  type: "clearRange";
  sheetId: string;
  range: GridRange;
  mode: "contents" | "formats" | "all";
}

export interface SpreadsheetReplaceRangePayload {
  type: "replaceRange";
  sheetId: string;
  range: GridRange | null;
  search: string;
  replace: string;
  matchCase: boolean;
}

export type SpreadsheetPasteMode = "all" | "values" | "formats";

export interface SpreadsheetPasteRangePayload {
  type: "pasteRange";
  sheetId: string;
  startRow: number;
  startColumn: number;
  rowCount: number;
  columnCount: number;
  cells: Array<{
    rowOffset: number;
    columnOffset: number;
    value?: unknown;
    formula?: string | null;
    attrs: Record<string, unknown>;
    style?: import("@open-office/schema/artifact").CellStyle | null;
  }>;
  mode: SpreadsheetPasteMode;
  sourceOrigin?: { row: number; column: number };
}

export interface SpreadsheetFillRangePayload {
  type: "fillRange";
  sourceSheetId: string;
  sourceRange: GridRange;
  destinationSheetId: string;
  destinationRange: GridRange;
  mode: SpreadsheetPasteMode;
}

export interface SpreadsheetSetFreezePanePayload {
  type: "setFreezePane";
  sheetId: string;
  rows: number;
  columns: number;
}

export interface SpreadsheetSetAutoFilterPayload {
  type: "setAutoFilter";
  sheetId: string;
  range: GridRange | null;
}

export interface SpreadsheetUpsertFilterColumnPayload {
  type: "upsertFilterColumn";
  sheetId: string;
  column: number;
  predicate: import("@open-office/schema/artifact").FilterPredicate;
}

export interface SpreadsheetClearFilterColumnPayload {
  type: "clearFilterColumn";
  sheetId: string;
  column: number | null;
}

export interface SpreadsheetSetCalculationModePayload {
  type: "setCalculationMode";
  calculationMode: "automatic" | "manual";
}

export interface SpreadsheetSetRowDimensionsPayload {
  type: "setRowDimensions";
  sheetId: string;
  rows: number | null;
}

export interface SpreadsheetSetColumnDimensionsPayload {
  type: "setColumnDimensions";
  sheetId: string;
  columns: number | null;
}

export interface SpreadsheetUpsertConditionalFormatPayload {
  type: "upsertConditionalFormat";
  sheetId: string;
  rule: import("@open-office/schema/artifact").ConditionalFormatRule;
}

export interface SpreadsheetDeleteConditionalFormatPayload {
  type: "deleteConditionalFormat";
  sheetId: string;
  ruleId: string;
}

export interface SpreadsheetUpsertDataValidationPayload {
  type: "upsertDataValidation";
  sheetId: string;
  rule: import("@open-office/schema/artifact").DataValidationRule;
}

export interface SpreadsheetDeleteDataValidationPayload {
  type: "deleteDataValidation";
  sheetId: string;
  ruleId: string;
}

/** Discriminated union of every Spreadsheet command the server accepts. */
export type SpreadsheetSemanticCommand =
  | { typeId: "spreadsheet.setCell"; payload: SpreadsheetSetCellPayload }
  | { typeId: "spreadsheet.clearCell"; payload: SpreadsheetClearCellPayload }
  | { typeId: "spreadsheet.setCellStyle"; payload: SpreadsheetSetCellStylePayload }
  | { typeId: "spreadsheet.setSheetMetadata"; payload: SpreadsheetSetSheetMetadataPayload }
  | { typeId: "spreadsheet.createSheet"; payload: SpreadsheetCreateSheetPayload }
  | { typeId: "spreadsheet.renameSheet"; payload: SpreadsheetRenameSheetPayload }
  | { typeId: "spreadsheet.deleteSheet"; payload: SpreadsheetDeleteSheetPayload }
  | { typeId: "spreadsheet.insertRows"; payload: SpreadsheetInsertRowsPayload }
  | { typeId: "spreadsheet.deleteRows"; payload: SpreadsheetDeleteRowsPayload }
  | { typeId: "spreadsheet.insertColumns"; payload: SpreadsheetInsertColumnsPayload }
  | { typeId: "spreadsheet.deleteColumns"; payload: SpreadsheetDeleteColumnsPayload }
  | { typeId: "spreadsheet.mergeCells"; payload: SpreadsheetMergeCellsPayload }
  | { typeId: "spreadsheet.unmergeCells"; payload: SpreadsheetUnmergeCellsPayload }
  | { typeId: "spreadsheet.sortRange"; payload: SpreadsheetSortRangePayload }
  | { typeId: "spreadsheet.formatRange"; payload: SpreadsheetFormatRangePayload }
  | { typeId: "spreadsheet.clearRange"; payload: SpreadsheetClearRangePayload }
  | { typeId: "spreadsheet.replaceRange"; payload: SpreadsheetReplaceRangePayload }
  | { typeId: "spreadsheet.pasteRange"; payload: SpreadsheetPasteRangePayload }
  | { typeId: "spreadsheet.fillRange"; payload: SpreadsheetFillRangePayload }
  | { typeId: "spreadsheet.setFreezePane"; payload: SpreadsheetSetFreezePanePayload }
  | { typeId: "spreadsheet.setAutoFilter"; payload: SpreadsheetSetAutoFilterPayload }
  | { typeId: "spreadsheet.upsertFilterColumn"; payload: SpreadsheetUpsertFilterColumnPayload }
  | { typeId: "spreadsheet.clearFilter"; payload: SpreadsheetClearFilterColumnPayload }
  | { typeId: "spreadsheet.setCalculationMode"; payload: SpreadsheetSetCalculationModePayload }
  | { typeId: "spreadsheet.setRowLayout"; payload: { type: "setRowLayout"; sheetId: string; startRow: number; endRow: number; height?: number; resetHeight?: boolean; hidden?: boolean } }
  | { typeId: "spreadsheet.setRowDimensions"; payload: SpreadsheetSetRowDimensionsPayload }
  | { typeId: "spreadsheet.setColumnDimensions"; payload: SpreadsheetSetColumnDimensionsPayload }
  | { typeId: "spreadsheet.upsertConditionalFormat"; payload: SpreadsheetUpsertConditionalFormatPayload }
  | { typeId: "spreadsheet.deleteConditionalFormat"; payload: SpreadsheetDeleteConditionalFormatPayload }
  | { typeId: "spreadsheet.upsertDataValidation"; payload: SpreadsheetUpsertDataValidationPayload }
  | { typeId: "spreadsheet.deleteDataValidation"; payload: SpreadsheetDeleteDataValidationPayload };

/** The server-owned undo/redo intent; never mixed with grid commands. */
export type SpreadsheetHistoryAction = "undo" | "redo";
