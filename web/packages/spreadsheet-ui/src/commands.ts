import { TABLE_STYLE_PRESETS, type TableStylePreset, type TableStyleOptions } from "./toolbar";
import type { SemanticCommandInput } from "@open-office/sdk";
import type {
  CellStyle,
  GridRange,
  SheetMetadata,
  SortDirection,
} from "@open-office/schema/artifact";
import type { SpreadsheetSemanticCommand } from "./types";
import type { SpreadsheetClipboardProjection } from "./selection";

/**
 * Spreadsheet semantic command builders.
 *
 * Every payload maps 1:1 to a Rust `SpreadsheetCommand` variant (serde tagged
 * by `type`, camelCase fields). The grid renderer and toolbar never serialize a
 * raw snapshot patch or a generic grid update; all writes flow through these
 * named commands, which keeps the browser a pure command producer.
 */

export function setCellCommand(
  sheetId: string,
  row: number,
  column: number,
  value: unknown,
): SemanticCommandInput {
  return {
    typeId: "spreadsheet.setCell",
    payload: { type: "setCell", sheetId, row, column, value, formula: null, attrs: {} },
  };
}

export function setCellFormulaCommand(
  sheetId: string,
  row: number,
  column: number,
  formula: string | null,
): SemanticCommandInput {
  return {
    typeId: "spreadsheet.setCell",
    payload: { type: "setCell", sheetId, row, column, value: null, formula, attrs: {} },
  };
}

export function clearCellCommand(sheetId: string, row: number, column: number): SemanticCommandInput {
  return { typeId: "spreadsheet.clearCell", payload: { type: "clearCell", sheetId, row, column } };
}

export function setCellStyleCommand(
  sheetId: string,
  row: number,
  column: number,
  style: CellStyle,
): SemanticCommandInput {
  return { typeId: "spreadsheet.setCellStyle", payload: { type: "setCellStyle", sheetId, row, column, style } };
}

export function setSheetMetadataCommand(sheetId: string, metadata: SheetMetadata): SemanticCommandInput {
  return { typeId: "spreadsheet.setSheetMetadata", payload: { type: "setSheetMetadata", sheetId, metadata } };
}

export function createSheetCommand(id: string, name: string): SemanticCommandInput {
  return { typeId: "spreadsheet.createSheet", payload: { type: "createSheet", id, name } };
}

export function renameSheetCommand(sheetId: string, name: string): SemanticCommandInput {
  return { typeId: "spreadsheet.renameSheet", payload: { type: "renameSheet", sheetId, name } };
}

export function deleteSheetCommand(sheetId: string): SemanticCommandInput {
  return { typeId: "spreadsheet.deleteSheet", payload: { type: "deleteSheet", sheetId } };
}

export function insertRowsCommand(sheetId: string, at: number, count = 1): SemanticCommandInput {
  return { typeId: "spreadsheet.insertRows", payload: { type: "insertRows", sheetId, at, count } };
}

export function deleteRowsCommand(sheetId: string, at: number, count = 1): SemanticCommandInput {
  return { typeId: "spreadsheet.deleteRows", payload: { type: "deleteRows", sheetId, at, count } };
}

export function insertColumnsCommand(sheetId: string, at: number, count = 1): SemanticCommandInput {
  return { typeId: "spreadsheet.insertColumns", payload: { type: "insertColumns", sheetId, at, count } };
}

export function deleteColumnsCommand(sheetId: string, at: number, count = 1): SemanticCommandInput {
  return { typeId: "spreadsheet.deleteColumns", payload: { type: "deleteColumns", sheetId, at, count } };
}

/**
 * Merges an inclusive region into its top-left anchor. The engine drops the
 * content of every non-anchor cell (Excel semantics) and rejects overlaps.
 */
export function mergeCellsCommand(sheetId: string, range: GridRange): SemanticCommandInput {
  return { typeId: "spreadsheet.mergeCells", payload: { type: "mergeCells", sheetId, range } };
}

/** Splits a previously merged region; `range` must exactly match an existing merge. */
export function unmergeCellsCommand(sheetId: string, range: GridRange): SemanticCommandInput {
  return { typeId: "spreadsheet.unmergeCells", payload: { type: "unmergeCells", sheetId, range } };
}

/**
 * Reorders the rows that carry cells inside `range`. The engine refuses to
 * sort a region that contains formula cells (moving a formula silently
 * changes its meaning), so callers should surface that typed error.
 */
export function sortRangeCommand(
  sheetId: string,
  range: GridRange,
  keys: Array<{ column: number; direction: SortDirection }>,
): SemanticCommandInput {
  return { typeId: "spreadsheet.sortRange", payload: { type: "sortRange", sheetId, range, keys } };
}

/**
 * M1-S 范围命令：一次引擎级原子操作 = 一个历史项，替代前端逐格展开的
 * N 个 setCell/setCellStyle。大选区格式化/清除/替换全部走这四个命令。
 */
export function formatRangeCommand(
  sheetId: string,
  range: GridRange,
  style: CellStyle,
  fields?: import("./types").CellStyleField[],
  rowPattern?: import("./types").SpreadsheetFormatRangePayload["rowPattern"],
): SemanticCommandInput {
  return { typeId: "spreadsheet.formatRange", payload: { type: "formatRange", sheetId, range, style, ...(fields ? { fields } : {}), ...(rowPattern ? { rowPattern } : {}) } };
}

/** A single batch applies body, alternating rows and header without replacing number formats. */
export function tableStyleCommands(sheetId: string, range: GridRange, presetId: TableStylePreset, options?: TableStyleOptions): SemanticCommandInput[] {
  const preset = TABLE_STYLE_PRESETS.find((entry) => entry.id === presetId);
  if (!preset) return [];
  const style = (background: string, color: string, bold: boolean): CellStyle => ({
    numberFormat: null, borders: null, alignment: null,
    fill: { foreground: null, background },
    font: { family: null, size: null, bold, italic: false, underline: false, strikethrough: false, color },
  });
  const settings = options ?? { headerRow: true, headerColumn: false, outline: false, filter: false, pattern: "rows" };
  const commands = [formatRangeCommand(sheetId, range, style("#ffffff", "#1d2129", false), ["fillBackground", "fontColor", "fontBold"])];
  if (options) commands.push(formatRangeCommand(sheetId, range, style("#ffffff", "#1d2129", false), ["borders"]));
  if (settings.pattern === "rows") commands.push(formatRangeCommand(sheetId, range, style(preset.stripe, "#1d2129", false), ["fillBackground"], "alternatingRows"));
  if (settings.pattern === "columns") {
    for (let column = range.startColumn + 1; column <= range.endColumn; column += 2) commands.push(formatRangeCommand(sheetId, { ...range, startColumn: column, endColumn: column }, style(preset.stripe, "#1d2129", false), ["fillBackground"]));
  }
  if (settings.pattern === "borders") {
    const borderStyle = { ...style("#ffffff", "#1d2129", false), borders: { top: null, bottom: { style: "thin" as const, color: preset.header }, left: null, right: null } };
    commands.push(formatRangeCommand(sheetId, range, borderStyle, ["borderBottom"], "alternatingRows"));
  }
  if (settings.headerRow) commands.push(formatRangeCommand(sheetId, range, style(preset.header, "#ffffff", true), ["fillBackground", "fontColor", "fontBold"], "firstRow"));
  if (settings.headerColumn) commands.push(formatRangeCommand(sheetId, { ...range, endColumn: range.startColumn }, style(preset.header, "#ffffff", true), ["fillBackground", "fontColor", "fontBold"]));
  if (settings.outline) {
    const edge = { style: "thin" as const, color: preset.header };
    commands.push(formatRangeCommand(sheetId, range, { ...style("#ffffff", "#1d2129", false), borders: { top: edge, bottom: edge, left: edge, right: edge } }, ["outerBorders"]));
  }
  if (options) commands.push(setAutoFilterCommand(sheetId, options.filter ? range : null));
  return commands;
}

export type ClearRangeMode = "contents" | "formats" | "all";

export function clearRangeCommand(
  sheetId: string,
  range: GridRange,
  mode: ClearRangeMode,
): SemanticCommandInput {
  return { typeId: "spreadsheet.clearRange", payload: { type: "clearRange", sheetId, range, mode } };
}

export function replaceRangeCommand(
  sheetId: string,
  range: GridRange | null,
  search: string,
  replace: string,
  matchCase = false,
): SemanticCommandInput {
  return {
    typeId: "spreadsheet.replaceRange",
    payload: { type: "replaceRange", sheetId, range, search, replace, matchCase },
  };
}

export function pasteRangeCommand(
  sheetId: string,
  startRow: number,
  startColumn: number,
  projection: SpreadsheetClipboardProjection,
  mode: import("./types").SpreadsheetPasteMode = "all",
): SemanticCommandInput {
  return {
    typeId: "spreadsheet.pasteRange",
    payload: {
      type: "pasteRange",
      sheetId,
      startRow,
      startColumn,
      rowCount: projection.rowCount,
      columnCount: projection.columnCount,
      cells: projection.cells,
      mode,
      ...(projection.sourceOrigin ? { sourceOrigin: projection.sourceOrigin } : {}),
    },
  };
}

export function fillRangeCommand(
  sourceSheetId: string,
  sourceRange: GridRange,
  destinationSheetId: string,
  destinationRange: GridRange,
  mode: import("./types").SpreadsheetPasteMode = "all",
): SemanticCommandInput {
  return {
    typeId: "spreadsheet.fillRange",
    payload: { type: "fillRange", sourceSheetId, sourceRange, destinationSheetId, destinationRange, mode },
  };
}

export function setFreezePaneCommand(sheetId: string, rows: number, columns: number): SemanticCommandInput {
  return { typeId: "spreadsheet.setFreezePane", payload: { type: "setFreezePane", sheetId, rows, columns } };
}

export function setAutoFilterCommand(sheetId: string, range: GridRange | null): SemanticCommandInput {
  return { typeId: "spreadsheet.setAutoFilter", payload: { type: "setAutoFilter", sheetId, range } };
}

/** M2-S：筛选列谓词 CRUD（按列号幂等）。 */
export function upsertFilterColumnCommand(
  sheetId: string,
  column: number,
  predicate: import("@open-office/schema/artifact").FilterPredicate,
): SemanticCommandInput {
  return { typeId: "spreadsheet.upsertFilterColumn", payload: { type: "upsertFilterColumn", sheetId, column, predicate } };
}

export function clearFilterColumnCommand(sheetId: string, column: number | null): SemanticCommandInput {
  return { typeId: "spreadsheet.clearFilter", payload: { type: "clearFilterColumn", sheetId, column } };
}

/** M2-S：计算模式是 workbook 级窄命令。 */
export function setCalculationModeCommand(
  calculationMode: "automatic" | "manual",
): SemanticCommandInput {
  return { typeId: "spreadsheet.setCalculationMode", payload: { type: "setCalculationMode", calculationMode } };
}

export function setRowDimensionsCommand(sheetId: string, rows: number | null): SemanticCommandInput {
  return { typeId: "spreadsheet.setRowDimensions", payload: { type: "setRowDimensions", sheetId, rows } };
}

export function setColumnDimensionsCommand(sheetId: string, columns: number | null): SemanticCommandInput {
  return { typeId: "spreadsheet.setColumnDimensions", payload: { type: "setColumnDimensions", sheetId, columns } };
}

/** M2-S：条件格式/数据校验规则按 id 幂等 CRUD。 */
export function upsertConditionalFormatCommand(
  sheetId: string,
  rule: import("@open-office/schema/artifact").ConditionalFormatRule,
): SemanticCommandInput {
  return { typeId: "spreadsheet.upsertConditionalFormat", payload: { type: "upsertConditionalFormat", sheetId, rule } };
}

export function deleteConditionalFormatCommand(sheetId: string, ruleId: string): SemanticCommandInput {
  return { typeId: "spreadsheet.deleteConditionalFormat", payload: { type: "deleteConditionalFormat", sheetId, ruleId } };
}

export function upsertDataValidationCommand(
  sheetId: string,
  rule: import("@open-office/schema/artifact").DataValidationRule,
): SemanticCommandInput {
  return { typeId: "spreadsheet.upsertDataValidation", payload: { type: "upsertDataValidation", sheetId, rule } };
}

export function deleteDataValidationCommand(sheetId: string, ruleId: string): SemanticCommandInput {
  return { typeId: "spreadsheet.deleteDataValidation", payload: { type: "deleteDataValidation", sheetId, ruleId } };
}

/** Narrows a produced command to the typed union (compile-time drift guard). */
export function asSpreadsheetCommand(command: SpreadsheetSemanticCommand): SpreadsheetSemanticCommand {
  return command;
}

/** Builds one `setCellStyle` command per anchor cell in the inclusive range. */
export function formatRangeCommands(
  sheetId: string,
  range: GridRange,
  apply: (base: CellStyle) => CellStyle,
  currentStyle: (row: number, column: number) => CellStyle,
): SemanticCommandInput[] {
  const commands: SemanticCommandInput[] = [];
  for (let row = range.startRow; row <= range.endRow; row += 1) {
    for (let column = range.startColumn; column <= range.endColumn; column += 1) {
      commands.push(setCellStyleCommand(sheetId, row, column, apply(currentStyle(row, column))));
    }
  }
  return commands;
}

/** Fills every FontStyle field so a partial patch never leaves a field undefined. */
export function mergeFontStyle(base: CellStyle["font"], patch: Partial<NonNullable<CellStyle["font"]>>): NonNullable<CellStyle["font"]> {
  return {
    family: patch.family ?? base?.family ?? null,
    size: patch.size ?? base?.size ?? null,
    bold: patch.bold ?? base?.bold ?? false,
    italic: patch.italic ?? base?.italic ?? false,
    strikethrough: patch.strikethrough ?? base?.strikethrough ?? false,
    underline: patch.underline ?? base?.underline ?? false,
    color: patch.color ?? base?.color ?? null,
  };
}

export function mergeFill(base: CellStyle["fill"], patch: Partial<NonNullable<CellStyle["fill"]>>): NonNullable<CellStyle["fill"]> {
  return {
    foreground: patch.foreground ?? base?.foreground ?? null,
    background: patch.background ?? base?.background ?? null,
  };
}

export function mergeAlignment(base: CellStyle["alignment"], patch: Partial<NonNullable<CellStyle["alignment"]>>): NonNullable<CellStyle["alignment"]> {
  return {
    horizontal: patch.horizontal ?? base?.horizontal ?? null,
    vertical: patch.vertical ?? base?.vertical ?? null,
    wrap: patch.wrap ?? base?.wrap ?? false,
  };
}

/** A display-only convenience: normalize a typed cell value for the formula bar. */
export function cellDisplayValue(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number") return String(value);
  if (typeof value === "boolean") return value ? "TRUE" : "FALSE";
  return String(value);
}

/** Row height/visibility is canonical worksheet state; adapters only send intent. */
export function setRowLayoutCommand(sheetId: string, startRow: number, endRow: number, patch: { height?: number; resetHeight?: boolean; hidden?: boolean }): SemanticCommandInput {
  return { typeId: "spreadsheet.setRowLayout", payload: { type: "setRowLayout", sheetId, startRow, endRow, ...patch } };
}
