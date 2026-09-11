import { SpreadsheetRowMenu, type RowMenuAction } from "./SpreadsheetRowMenu.js";
import {
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import type { CellModel, DataValidationKind, FilterPredicate, GridRange, SheetModel, CellStyle, ComparisonOperator } from "@open-office/schema/artifact";
import type { SpreadsheetGridCell } from "@open-office/schema/api";

import { SpreadsheetFindPanel } from "./SpreadsheetFindPanel.js";
import { SpreadsheetGrid } from "./SpreadsheetGrid.js";
import { SpreadsheetToolbar, type RibbonTab } from "./SpreadsheetToolbar.js";
import {
  setRowLayoutCommand,
  autoSumTarget,
  sumFormula,
  upsertConditionalFormatCommand,
  deleteConditionalFormatCommand,
  cellRef,
  columnToLetters,
  clearRangeCommand,
  pasteRangeCommand,
  formatRangeCommand,
  tableStyleCommands,
  createSheetCommand,
  deleteSheetCommand,
  deleteColumnsCommand,
  deleteRowsCommand,
  insertColumnsCommand,
  insertRowsCommand,
  mergeAlignment,
  mergeBorders,
  mergeCellsCommand,
  mergeFill,
  mergeFontColor,
  mergeFontDecoration,
  mergeFontSize,
  mergeFontStyle,
  mergeNumberFormat,
  mergeVerticalAlignment,
  mergeWrap,
  renameSheetCommand,
  setCellCommand,
  setAutoFilterCommand,
  upsertFilterColumnCommand,
  clearFilterColumnCommand,
  upsertDataValidationCommand,
  deleteDataValidationCommand,
  setCellFormulaCommand,
  setFreezePaneCommand,
  sortRangeCommand,
  toolbarStyleState,
  unmergeCellsCommand,
  type BorderPreset,
  type TableStylePreset,
  type TableStyleOptions,
  type CellStyleField,
} from "@open-office/spreadsheet-ui";
import {
  moveCell,
  mergedAnchor,
  projectClipboardRange,
  parseSpreadsheetClipboard,
  serializeSpreadsheetClipboard,
  selectionRange,
  type CellCoordinate,
  type CellSelection,
  type GridBounds,
  type NavKey,
  type SpreadsheetClipboardProjection,
} from "@open-office/spreadsheet-ui";
import { useSpreadsheetSession } from "./useSpreadsheetSession.js";
import "./spreadsheet.css";

const EMPTY_CELL_STYLE: CellStyle = { numberFormat: null, font: null, fill: null, alignment: null, borders: null };
const DEFAULT_ROWS = 1200;
const DEFAULT_COLUMNS = 104;

interface Props {
  id: string;
  title: string;
  onBack: () => void;
}

export function SpreadsheetStudio({ id, title, onBack }: Props) {
  const session = useSpreadsheetSession(id);
  const {
    model,
    revision,
    loading,
    saving,
    error,
    activeSheetId,
    setActiveSheetId,
    submit,
    submitHistory,
    projectRange,
    canUndo,
    canRedo,
    availableCapabilities,
    capabilitiesLoaded,
  } = session;
  const [selection, setSelection] = useState<CellSelection | null>(null);
  const [activeCell, setActiveCell] = useState<CellCoordinate>({ row: 0, column: 0 });
  const [editing, setEditing] = useState<CellCoordinate | null>(null);
  const [editingValue, setEditingValue] = useState("");
  const [renameSheetId, setRenameSheetId] = useState<string | null>(null);
  const [tableStyleContext, setTableStyleContext] = useState<{ sheetId: string; range: GridRange; preset: TableStylePreset; options: TableStyleOptions } | null>(null);
  const [ribbonTab, setRibbonTab] = useState<RibbonTab>("home");
  const [searchDestination, setSearchDestination] = useState<CellCoordinate | null>(null);
  const [findReplaceOpen, setFindReplaceOpen] = useState(false);
  const [clipboardHasContent, setClipboardHasContent] = useState(false);
  const [editingInitial, setEditingInitial] = useState<string | null>(null);
  const [rowMenu, setRowMenu] = useState<{ x: number; y: number; startRow: number; endRow: number } | null>(null);
  const [rowDialog, setRowDialog] = useState<{ type: "height" | "format"; startRow: number; endRow: number } | null>(null);
  const [rowInput, setRowInput] = useState("21");
  const [menuNotice, setMenuNotice] = useState("");
  const copiedCellsRef = useRef<SpreadsheetClipboardProjection | null>(null);
  const copiedTextRef = useRef<string | null>(null);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number } | null>(null);
  /** 格式刷：armed 时持有聚焦样式，下一次范围选择应用。 */
  const [paintStyle, setPaintStyle] = useState<CellStyle | null>(null);
  const [projectedCells, setProjectedCells] = useState<ReadonlyMap<string, CellModel>>(new Map());

  useEffect(() => {
    setProjectedCells(new Map());
  }, [activeSheetId, revision]);

  const receiveProjectionCells = useCallback((cells: readonly SpreadsheetGridCell[]) => {
    setProjectedCells((current) => {
      const next = new Map(current);
      for (const cell of cells) {
        next.set(`${cell.address.row}:${cell.address.column}`, {
          row: cell.address.row,
          column: cell.address.column,
          ...(cell.value === undefined ? {} : { value: cell.value }),
          ...(cell.formula === undefined ? {} : { formula: cell.formula }),
          attrs: cell.attrs,
          ...(cell.style === undefined ? {} : { style: cell.style }),
        });
      }
      return next;
    });
  }, []);

  const sheet: SheetModel | null = useMemo(() => {
    if (!model || !activeSheetId) return null;
    const structure = model.sheets.find((candidate) => candidate.id === activeSheetId);
    return structure ? { ...structure, cells: [...projectedCells.values()] } : null;
  }, [activeSheetId, model, projectedCells]);

  const bounds: GridBounds = useMemo(() => ({
    rows: Math.max(DEFAULT_ROWS, sheet?.metadata.rowCount ?? DEFAULT_ROWS),
    columns: Math.max(DEFAULT_COLUMNS, sheet?.metadata.columnCount ?? DEFAULT_COLUMNS),
  }), [sheet]);

  const cellsByCoordinate = useMemo(() => {
    const index = new Map<string, CellModel>();
    for (const cell of sheet?.cells ?? []) index.set(`${cell.row}:${cell.column}`, cell);
    return index;
  }, [sheet]);

  const findCell = useCallback((row: number, column: number): CellModel | null => {
    return cellsByCoordinate.get(`${row}:${column}`) ?? null;
  }, [cellsByCoordinate]);

  const focusedCell = useMemo(() => mergedAnchor(selection?.kind === "cells" ? selection.anchor : activeCell, sheet?.metadata.mergedRanges ?? []), [activeCell, selection, sheet]);
  const focusedCellModel = findCell(focusedCell.row, focusedCell.column);
  const activeLabel = cellRef(focusedCell.row, focusedCell.column);
  const activeFormula = focusedCellModel?.formula ?? "";
  const activeValue = focusedCellModel?.value;

  const formulaBarValue = editing ? editingValue : (activeFormula || cellDisplayValue(activeValue));

  const range = useMemo(() => selectionRange(selection, bounds, sheet?.metadata.mergedRanges), [bounds, selection, sheet]);

  const currentStyle = useCallback((row: number, column: number): CellStyle => {
    return findCell(row, column)?.style ?? EMPTY_CELL_STYLE;
  }, [findCell]);

  const submitRangeFormat = useCallback((apply: (base: CellStyle) => CellStyle, fields: CellStyleField[]) => {
    if (!range || !activeSheetId) return;
    const target = apply(currentStyle(range.startRow, range.startColumn));
    // The engine merges selected properties per cell; the anchor alone cannot
    // establish whether a mixed range is already formatted.
    void submit([formatRangeCommand(activeSheetId, range, target, fields)]);
  }, [activeSheetId, currentStyle, range, submit]);

  const toggleBold = useCallback(() => {
    const base = currentStyle(focusedCell.row, focusedCell.column);
    const bold = !(base.font?.bold ?? false);
    submitRangeFormat((cell) => ({ ...cell, font: mergeFontStyle(cell.font, { bold }) }), ["fontBold"]);
  }, [currentStyle, focusedCell, submitRangeFormat]);

  const toggleItalic = useCallback(() => {
    const base = currentStyle(focusedCell.row, focusedCell.column);
    const italic = !(base.font?.italic ?? false);
    submitRangeFormat((cell) => ({ ...cell, font: mergeFontStyle(cell.font, { italic }) }), ["fontItalic"]);
  }, [currentStyle, focusedCell, submitRangeFormat]);

  const applyFill = useCallback((color: string) => {
    submitRangeFormat((cell) => ({ ...cell, fill: mergeFill(cell.fill, { background: color }) }), ["fillBackground"]);
  }, [submitRangeFormat]);

  const applyAlign = useCallback((horizontal: "left" | "center" | "right") => {
    submitRangeFormat((cell) => ({ ...cell, alignment: mergeAlignment(cell.alignment, { horizontal }) }), ["horizontalAlignment"]);
  }, [submitRangeFormat]);

  const selectCell = useCallback((row: number, column: number, extend: boolean) => {
    setEditing(null);
    if (extend && selection?.kind === "cells") {
      setSelection({ kind: "cells", anchor: selection.anchor, focus: { row, column } });
    } else if (extend && selection) {
      setSelection({ kind: "cells", anchor: focusedCell, focus: { row, column } });
    } else {
      // 格式刷：armed 且非扩展选择时，把刷子样式应用到点击的单格。
      if (paintStyle && activeSheetId) {
        const brushRange = {
          startRow: row,
          startColumn: column,
          endRow: row,
          endColumn: column,
        };
        void submit([formatRangeCommand(activeSheetId, brushRange, paintStyle)]);
        setPaintStyle(null);
      }
      setSelection({ kind: "cells", anchor: { row, column }, focus: { row, column } });
      setActiveCell({ row, column });
    }
  }, [activeSheetId, focusedCell, paintStyle, selection, submit]);

  const selectRow = useCallback((row: number, extend = false) => {
    setEditing(null);
    setSelection(current => ({ kind: "row", row: extend && current?.kind === "row" ? current.row : row, endRow: row }));
    setActiveCell({ row, column: 0 });
  }, []);

  const selectColumn = useCallback((column: number) => {
    setEditing(null);
    setSelection({ kind: "column", column });
    setActiveCell((current) => ({ row: current.row, column }));
  }, []);

  const selectAll = useCallback(() => {
    setEditing(null);
    setSelection({ kind: "all" });
  }, []);

  const activateCell = useCallback((row: number, column: number) => {
    const cell = findCell(row, column);
    const text = cell?.formula ?? cellDisplayValue(cell?.value);
    setSelection({ kind: "cells", anchor: { row, column }, focus: { row, column } });
    setActiveCell({ row, column });
    setEditing({ row, column });
    setEditingValue(text);
    setEditingInitial(text);
  }, [findCell]);

  /** Excel 语义：选中后直接键入可打印字符，覆盖式进入编辑。 */
  const typeToEdit = useCallback((row: number, column: number, character: string) => {
    if (saving) return;
    setSelection({ kind: "cells", anchor: { row, column }, focus: { row, column } });
    setActiveCell({ row, column });
    setEditing({ row, column });
    setEditingValue(character);
    setEditingInitial(character);
  }, [saving]);

  const commitEdit = useCallback((text: string) => {
    if (!editing || !activeSheetId) return;
    const { row, column } = editing;
    setEditing(null);
    const trimmed = text.trim();
    const existing = findCell(row, column);
    if (!trimmed) {
      if (existing) void submit([clearRangeCommand(activeSheetId, { startRow: row, endRow: row, startColumn: column, endColumn: column }, "contents")]);
      return;
    }
    if (trimmed.startsWith("=")) {
      if (existing?.formula !== trimmed) void submit([setCellFormulaCommand(activeSheetId, row, column, trimmed)]);
    } else {
      void submit([setCellCommand(activeSheetId, row, column, parseCellValue(trimmed))]);
    }
  }, [activeSheetId, editing, findCell, submit]);

  const commitFormulaBar = useCallback(() => {
    if (!activeSheetId) return;
    const { row, column } = focusedCell;
    const trimmed = formulaBarValue.trim();
    const existing = findCell(row, column);
    if (!trimmed) {
      if (existing) void submit([clearRangeCommand(activeSheetId, { startRow: row, endRow: row, startColumn: column, endColumn: column }, "contents")]);
      return;
    }
    if (trimmed.startsWith("=")) {
      if (existing?.formula !== trimmed) void submit([setCellFormulaCommand(activeSheetId, row, column, trimmed)]);
    } else {
      void submit([setCellCommand(activeSheetId, row, column, parseCellValue(trimmed))]);
    }
  }, [activeSheetId, findCell, focusedCell, formulaBarValue, submit]);

  const handleFormulaBarKey = useCallback((event: ReactKeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Enter") {
      event.preventDefault();
      commitFormulaBar();
    } else if (event.key === "Escape") {
      event.preventDefault();
      setEditing(null);
    }
  }, [commitFormulaBar]);

  const copySelection = useCallback(async (): Promise<boolean> => {
    if (!range || !activeSheetId) return false;
    const cellCount = (range.endRow - range.startRow + 1) * (range.endColumn - range.startColumn + 1);
    if (cellCount > 100_000) {
      setMenuNotice("单次复制最多支持 100,000 个单元格");
      return false;
    }
    let cells: readonly SpreadsheetGridCell[];
    try {
      cells = await projectRange(activeSheetId, range);
    } catch (reason) {
      setMenuNotice(reason instanceof Error ? reason.message : "复制失败，请重试");
      return false;
    }
    const lookup = new Map(cells.map((cell) => [`${cell.address.row}:${cell.address.column}`, {
      row: cell.address.row,
      column: cell.address.column,
      ...(cell.value === undefined ? {} : { value: cell.value }),
      ...(cell.formula === undefined ? {} : { formula: cell.formula }),
      attrs: cell.attrs,
      ...(cell.style === undefined ? {} : { style: cell.style }),
    }]));
    const projection = projectClipboardRange(range, (row, column) => lookup.get(`${row}:${column}`) ?? null);
    const text = serializeSpreadsheetClipboard(projection);
    copiedCellsRef.current = projection;
    copiedTextRef.current = text;
    setClipboardHasContent(true);
    void navigator.clipboard?.writeText(text).catch(() => {
      setMenuNotice("无法写入系统剪贴板，仍可在当前表格内粘贴");
    });
    return true;
  }, [activeSheetId, projectRange, range]);

  const pasteClipboard = useCallback(async (mode: "all" | "values" | "formats" = "all") => {
    if (!activeSheetId || saving) return;
    let source = copiedCellsRef.current;
    try {
      const systemText = await navigator.clipboard?.readText();
      if (systemText && systemText !== copiedTextRef.current) source = parseSpreadsheetClipboard(systemText);
    } catch {
      // Clipboard permission can be denied; the detached in-app projection is
      // still safe and preserves formatting/formula copy semantics.
    }
    if (!source) return;
    const rowCount = Math.min(source.rowCount, bounds.rows - focusedCell.row);
    const columnCount = Math.min(source.columnCount, bounds.columns - focusedCell.column);
    if (rowCount <= 0 || columnCount <= 0) return;
    const clipped: SpreadsheetClipboardProjection = {
      rowCount,
      columnCount,
      cells: source.cells.filter((cell) => cell.rowOffset < rowCount && cell.columnOffset < columnCount),
      ...(source.sourceOrigin ? { sourceOrigin: source.sourceOrigin } : {}),
    };
    await submit([pasteRangeCommand(activeSheetId, focusedCell.row, focusedCell.column, clipped, mode)], "local", { retryOnConflict: true });
  }, [activeSheetId, bounds, focusedCell, saving, submit]);

  const clearSelection = useCallback(() => {
    if (!range || !activeSheetId) return;
    void submit([clearRangeCommand(activeSheetId, range, "contents")]);
  }, [activeSheetId, range, submit]);

  const handleKeyDown = useCallback((event: ReactKeyboardEvent<HTMLElement>) => {
    if (editing || rowMenu || rowDialog) return;
    const target = event.target as HTMLElement;
    if (target.tagName === "INPUT" || target.tagName === "TEXTAREA") return;
    const mod = event.metaKey || event.ctrlKey || event.altKey;
    if (mod && event.key.toLowerCase() === "c") {
      event.preventDefault();
      void copySelection();
      return;
    }
    if (mod && event.key.toLowerCase() === "x") {
      event.preventDefault();
      void copySelection();
      clearSelection();
      return;
    }
    if (mod && event.key.toLowerCase() === "v") {
      event.preventDefault();
      void pasteClipboard();
      return;
    }
    if (mod && event.key.toLowerCase() === "f") {
      event.preventDefault();
      setFindReplaceOpen(true);
      return;
    }
    if (event.key === "Delete" || event.key === "Backspace") {
      if (range) {
        event.preventDefault();
        clearSelection();
      }
      return;
    }
    // Excel 语义：可打印字符（无修饰键）直接进入覆盖式编辑。
    // IME composition（key === "Process"）与功能键不触发；event.isComposing 双保险。
    if (!mod && !event.nativeEvent.isComposing && event.key.length === 1 && event.key !== " ") {
      typeToEdit(focusedCell.row, focusedCell.column, event.key);
      return;
    }
    if (!mod && event.key === " ") {
      // 空格也是合法起始字符，但避免吞掉滚动语义——Excel 里空格进入编辑。
      event.preventDefault();
      typeToEdit(focusedCell.row, focusedCell.column, " ");
      return;
    }
    const navKey = navKeyFromEvent(event);
    if (navKey) {
      event.preventDefault();
      setEditing(null);
      let next = moveCell(focusedCell, navKey, bounds, 20, sheet?.metadata.mergedRanges);
      const hidden = new Set(sheet?.metadata.rowLayout?.filter(entry => entry.hidden).map(entry => entry.row));
      const step = navKey === "up" || navKey === "pageUp" ? -1 : 1;
      while (hidden.has(next.row) && next.row + step >= 0 && next.row + step < bounds.rows) next = { ...next, row: next.row + step };
      if (hidden.has(next.row)) return;
      setSearchDestination(next);
      setActiveCell(next);
      setSelection({ kind: "cells", anchor: next, focus: next });
    }
  }, [bounds, clearSelection, copySelection, editing, focusedCell, pasteClipboard, range, sheet, typeToEdit, rowMenu, rowDialog]);

  const freezeActive = sheet?.metadata.freeze;
  const isFrozen = (freezeActive?.rows ?? 0) > 0 || (freezeActive?.columns ?? 0) > 0;

  const toggleFreeze = useCallback(() => {
    if (!activeSheetId) return;
    // M1-S：细粒度冻结命令替代整体 SheetMetadata 覆盖。
    // Excel 冻结语义：冻结线在当前格上侧与左侧——
    // A1（无上无左）退化为冻结首行；首行其他列只冻结左侧列。
    const rows = focusedCell.row > 0
      ? focusedCell.row
      : (focusedCell.column > 0 ? 0 : 1);
    const columns = focusedCell.column > 0 ? focusedCell.column : 0;
    const next = isFrozen ? { rows: 0, columns: 0 } : { rows, columns };
    void submit([setFreezePaneCommand(activeSheetId, next.rows, next.columns)]);
  }, [activeSheetId, focusedCell, isFrozen, submit]);

  const canMerge = useMemo(() => {
    if (!range || !sheet) return false;
    if (range.startRow === range.endRow && range.startColumn === range.endColumn) return false;
    const overlaps = (a: GridRange, b: GridRange) =>
      a.startRow <= b.endRow && a.endRow >= b.startRow && a.startColumn <= b.endColumn && a.endColumn >= b.startColumn;
    return !sheet.metadata.mergedRanges.some((mergedRange) => overlaps(mergedRange, range) && !(mergedRange.startRow === range.startRow && mergedRange.startColumn === range.startColumn && mergedRange.endRow === range.endRow && mergedRange.endColumn === range.endColumn));
  }, [range, sheet]);

  const activeCellInsideMerge = useMemo(() => {
    if (!sheet || !range) return null;
    return sheet.metadata.mergedRanges.find((mergedRange) =>
      range.startRow >= mergedRange.startRow && range.startColumn >= mergedRange.startColumn
      && range.endRow <= mergedRange.endRow && range.endColumn <= mergedRange.endColumn,
    ) ?? null;
  }, [range, sheet]);

  const toggleMerge = useCallback(() => {
    if (!sheet || !activeSheetId || !range) return;
    // Merge/unmerge now travel as dedicated semantic commands; the engine
    // owns overlap rejection and non-anchor content dropping.
    const targetMerge = activeCellInsideMerge;
    if (targetMerge) {
      void submit([unmergeCellsCommand(activeSheetId, targetMerge)]);
      return;
    }
    if (canMerge) void submit([mergeCellsCommand(activeSheetId, range)]);
  }, [activeCellInsideMerge, activeSheetId, canMerge, range, submit]);

  const rowForDelete = selection?.kind === "row" ? selection.row : focusedCell.row;
  const columnForDelete = selection?.kind === "column" ? selection.column : focusedCell.column;
  const insertRow = useCallback((at: number) => {
    if (activeSheetId) void submit([insertRowsCommand(activeSheetId, at)]);
  }, [activeSheetId, submit]);
  const deleteRow = useCallback((at: number) => {
    if (activeSheetId) void submit([deleteRowsCommand(activeSheetId, at)]);
  }, [activeSheetId, submit]);
  const insertColumn = useCallback((at: number) => {
    if (activeSheetId) void submit([insertColumnsCommand(activeSheetId, at)]);
  }, [activeSheetId, submit]);
  const deleteColumn = useCallback((at: number) => {
    if (activeSheetId) void submit([deleteColumnsCommand(activeSheetId, at)]);
  }, [activeSheetId, submit]);

  // ---- 工具栏新能力：字号 / 字体颜色 / 垂直对齐 / 换行 / 数字格式 / 排序 / 筛选 ----
  const applyFontSize = useCallback((size: number) => {
    submitRangeFormat((cell) => mergeFontSize(cell, size), ["fontSize"]);
  }, [submitRangeFormat]);

  const applyFontColor = useCallback((color: string) => {
    submitRangeFormat((cell) => mergeFontColor(cell, color), ["fontColor"]);
  }, [submitRangeFormat]);

  const applyVerticalAlign = useCallback((vertical: "top" | "middle" | "bottom") => {
    submitRangeFormat((cell) => mergeVerticalAlignment(cell, vertical), ["verticalAlignment"]);
  }, [submitRangeFormat]);

  const toggleWrap = useCallback(() => {
    const wrap = !(currentStyle(focusedCell.row, focusedCell.column).alignment?.wrap ?? false);
    submitRangeFormat((cell) => mergeWrap(cell, wrap), ["wrap"]);
  }, [currentStyle, focusedCell, submitRangeFormat]);

  const applyNumberFormat = useCallback((format: string) => {
    submitRangeFormat((cell) => mergeNumberFormat(cell, format), ["numberFormat"]);
  }, [submitRangeFormat]);

  const clearSelectionContents = useCallback(() => {
    if (!activeSheetId || !range) return;
    // 选区内没有可清内容时不发事务（幂等路径不产生无谓请求）。
    const hasContent = sheet?.cells.some(
      (cell) =>
        cell.row >= range.startRow && cell.row <= range.endRow &&
        cell.column >= range.startColumn && cell.column <= range.endColumn &&
        (cell.value !== undefined && cell.value !== null || cell.formula !== undefined),
    );
    if (!hasContent) return;
    void submit([clearRangeCommand(activeSheetId, range, "contents")]);
  }, [activeSheetId, range, sheet, submit]);

  const clearSelectionFormatting = useCallback(() => {
    if (!activeSheetId || !range) return;
    // 选区内没有任何带样式格子时不发事务。
    const hasStyled = sheet?.cells.some(
      (cell) =>
        cell.row >= range.startRow && cell.row <= range.endRow &&
        cell.column >= range.startColumn && cell.column <= range.endColumn &&
        cell.style !== undefined && cell.style !== null,
    );
    if (!hasStyled) return;
    void submit([clearRangeCommand(activeSheetId, range, "formats")]);
  }, [activeSheetId, range, sheet, submit]);

  const sortSelection = useCallback((direction: "ascending" | "descending") => {
    if (!activeSheetId || !range) return;
    // Sort by the focused column; the engine refuses regions with formulas.
    void submit([
      sortRangeCommand(activeSheetId, range, [{ column: focusedCell.column, direction }]),
    ]);
  }, [activeSheetId, focusedCell.column, range, submit]);

  /** AutoSum：按 Excel 语义推导范围并在目标格插入 =SUM(...)。 */
  const autoSumSelection = useCallback(() => {
    if (!activeSheetId || !sheet || !range) return;
    const numericAt = (row: number, column: number) => {
      const cell = sheet.cells.find((candidate) => candidate.row === row && candidate.column === column);
      return typeof cell?.value === "number";
    };
    const target = autoSumTarget(range, numericAt);
    if (!target) return;
    void submit([
      setCellFormulaCommand(
        activeSheetId,
        target.row,
        target.column,
        sumFormula(target.range),
      ),
    ]);
  }, [activeSheetId, range, sheet, submit]);

  /** 格式刷：armed = 复制当前聚焦样式；下一次单格选择时应用（见 selectCell）。 */
  const togglePaintFormat = useCallback(() => {
    setPaintStyle((current) =>
      current ? null : currentStyle(focusedCell.row, focusedCell.column),
    );
  }, [currentStyle, focusedCell]);

  /** 条件格式：对当前选区 upsert 一条 CellIs greaterThan 规则。 */
  const applyConditionalFormat = useCallback(({ id, threshold, operator = "greaterThan" }: { id: string; threshold: number; operator?: ComparisonOperator }) => {
    if (!activeSheetId || !range) return;
    void submit([
      upsertConditionalFormatCommand(activeSheetId, {
        id,
        range,
        predicate: { type: "cellIs", value: { operator, value: threshold } },
        style: {
          numberFormat: null,
          font: { family: null, size: null, bold: true, italic: false, strikethrough: false, underline: false, color: "#f53f3f" },
          fill: { foreground: null, background: "#fff1f0" },
          alignment: null,
          borders: null,
        },
      }),
    ]);
  }, [activeSheetId, range, submit]);

  const removeConditionalFormat = useCallback((ruleId: string) => {
    if (!activeSheetId) return;
    void submit([deleteConditionalFormatCommand(activeSheetId, ruleId)]);
  }, [activeSheetId, submit]);

  /** 当前 sheet 的条件格式规则投影（Toolbar 面板渲染用）。 */
  const conditionalFormatRules = useMemo(() => {
    if (!sheet) return [] as Array<{ id: string; operator: string; value: number }>;
    return sheet.metadata.conditionalFormats.map((rule) => {
      const value =
        rule.predicate.type === "cellIs" && rule.predicate.value?.value != null
          ? Number(rule.predicate.value.value)
          : Number.NaN;
      const operator =
        rule.predicate.type === "cellIs" ? String(rule.predicate.value?.operator ?? "?") : rule.predicate.type;
      return { id: rule.id, operator, value };
    });
  }, [sheet]);

  const filterActive = sheet?.metadata.autoFilter !== null && sheet?.metadata.autoFilter !== undefined;
  const filterRule = useMemo(() => {
    const rule = sheet?.metadata.autoFilter?.columns.find((entry) => entry.column === focusedCell.column);
    return rule ? { column: rule.column, label: `${columnToLetters(rule.column)} 列` } : null;
  }, [focusedCell.column, sheet]);

  const applyFilterRule = useCallback((type: "contains" | "equals" | "greaterThan" | "lessThan", rawValue: string) => {
    if (!activeSheetId || !range || !sheet) return;
    const value = rawValue.trim();
    if (!value) return;
    let predicate: FilterPredicate;
    if (type === "contains") predicate = { type, value };
    else if (type === "equals") {
      const number = Number(value);
      const normalized = value === "true" ? true : value === "false" ? false : Number.isFinite(number) ? number : value;
      predicate = { type, value: normalized };
    } else {
      const number = Number(value);
      if (!Number.isFinite(number)) {
        session.reportError("大于/小于筛选需要有效数字。");
        return;
      }
      predicate = { type, value: number };
    }
    const existing = sheet.metadata.autoFilter;
    const commands = existing ? [] : [setAutoFilterCommand(activeSheetId, range)];
    if (existing && (focusedCell.column < existing.range.startColumn || focusedCell.column > existing.range.endColumn)) {
      session.reportError("当前列不在已启用的筛选范围内，请先关闭筛选并重新选择范围。");
      return;
    }
    commands.push(upsertFilterColumnCommand(activeSheetId, focusedCell.column, predicate));
    void submit(commands);
  }, [activeSheetId, focusedCell.column, range, session, sheet, submit]);

  const clearFilterRule = useCallback(() => {
    if (!activeSheetId || !filterRule) return;
    void submit([clearFilterColumnCommand(activeSheetId, filterRule.column)]);
  }, [activeSheetId, filterRule, submit]);

  const dataValidationRules = useMemo(() => (sheet?.metadata.dataValidations ?? []).map((rule) => ({
    id: rule.id,
    label: `${cellRef(rule.range.startRow, rule.range.startColumn)}:${cellRef(rule.range.endRow, rule.range.endColumn)} · ${rule.kind.type}`,
  })), [sheet]);

  const applyDataValidation = useCallback((input: { type: "list" | "wholeNumber" | "decimal" | "date"; first: string; second: string; allowBlank: boolean; errorMessage: string }) => {
    if (!activeSheetId || !range) return;
    let kind: DataValidationKind;
    if (input.type === "list") {
      const values = input.first.split(",").map((value) => value.trim()).filter(Boolean);
      if (!values.length) {
        session.reportError("列表验证至少需要一个允许值。");
        return;
      }
      kind = { type: "list", value: values };
    } else {
      const min = Number(input.first);
      const max = Number(input.second);
      if (!Number.isFinite(min) || !Number.isFinite(max) || min > max) {
        session.reportError("验证范围需要有效数字，且最小值不能大于最大值。");
        return;
      }
      if (input.type === "wholeNumber" && (!Number.isInteger(min) || !Number.isInteger(max))) {
        session.reportError("整数验证的上下限必须是整数。");
        return;
      }
      kind = input.type === "date"
        ? { type: "date", value: { minSerial: min, maxSerial: max } }
        : { type: input.type, value: { min, max } };
    }
    const id = `validation-${crypto.randomUUID()}`;
    void submit([upsertDataValidationCommand(activeSheetId, {
      id,
      range,
      kind,
      allowBlank: input.allowBlank,
      errorMessage: input.errorMessage.trim() || null,
    })]);
  }, [activeSheetId, range, session, submit]);

  const removeDataValidation = useCallback((ruleId: string) => {
    if (!activeSheetId) return;
    void submit([deleteDataValidationCommand(activeSheetId, ruleId)]);
  }, [activeSheetId, submit]);

  const toggleFilter = useCallback(() => {
    if (!activeSheetId) return;
    // M1-S：细粒度筛选命令替代整体 SheetMetadata 覆盖。
    const filterRange = range
      ?? (sheet
        ? {
            startRow: 0,
            startColumn: 0,
            endRow: Math.max(0, sheet.metadata.rowCount ?? 1) - 1,
            endColumn: Math.max(0, sheet.metadata.columnCount ?? 1) - 1,
          }
        : null);
    void submit([setAutoFilterCommand(activeSheetId, filterActive ? null : filterRange)]);
  }, [activeSheetId, filterActive, range, sheet, submit]);

  // ---- Ribbon 新能力：装饰 / 边框 / 清除格式 / 字体族 / 查找替换 ----
  const cutSelection = useCallback(() => {
    void copySelection().then((copied) => { if (copied) clearSelection(); });
  }, [clearSelection, copySelection]);

  const applyFontFamily = useCallback((family: string) => {
    submitRangeFormat((cell) => ({ ...cell, font: mergeFontStyle(cell.font, { family }) }), ["fontFamily"]);
  }, [submitRangeFormat]);

  const toggleDecoration = useCallback((decoration: "strikethrough" | "underline") => {
    const base = currentStyle(focusedCell.row, focusedCell.column);
    const next = !(base.font?.[decoration] ?? false);
    submitRangeFormat((cell) => mergeFontDecoration(cell, decoration, next), [decoration === "underline" ? "fontUnderline" : "fontStrikethrough"]);
  }, [currentStyle, focusedCell, submitRangeFormat]);

  const applyBorderPreset = useCallback((preset: BorderPreset) => {
    submitRangeFormat((cell) => mergeBorders(cell, preset), [({ all: "borders", none: "borders", outer: "outerBorders", top: "borderTop", bottom: "borderBottom", left: "borderLeft", right: "borderRight" } as const)[preset.side]]);
  }, [submitRangeFormat]);

  const replaceMatches = useCallback(async (cells: CellModel[], search: string, replacement: string, matchCase: boolean, exact: boolean) => {
    if (!activeSheetId || !search) return false;
    const escaped = search.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
    const pattern = new RegExp(escaped, matchCase ? "g" : "gi");
    return submit(cells.map(cell => {
      const command = setCellCommand(activeSheetId, cell.row, cell.column,
        exact ? replacement : String(cell.value).replace(pattern, () => replacement));
      return { ...command, payload: { ...command.payload, attrs: cell.attrs } };
    }));
  }, [activeSheetId, submit]);

  const addSheet = useCallback(() => {
    if (!model || !activeSheetId) return;
    const nextIndex = model.sheets.length + 1;
    const id = `sheet-${crypto.randomUUID?.() ?? `${Date.now()}-${nextIndex}`}`;
    void submit([createSheetCommand(id, `Sheet ${nextIndex}`)]);
  }, [activeSheetId, model, submit]);

  const renameSheet = useCallback((sheetId: string, name: string) => {
    setRenameSheetId(null);
    const trimmed = name.trim();
    if (!trimmed) return;
    const target = model?.sheets.find((candidate) => candidate.id === sheetId);
    if (target?.name === trimmed) return;
    void submit([renameSheetCommand(sheetId, trimmed)]);
  }, [model, submit]);

  const deleteSheet = useCallback((sheetId: string) => {
    if (!model || model.sheets.length <= 1) return;
    void submit([deleteSheetCommand(sheetId)]);
  }, [model, submit]);

  const fallbackActiveSheetId = useMemo(() => {
    if (!model || model.sheets.length === 0) return null;
    if (activeSheetId && model.sheets.some((candidate) => candidate.id === activeSheetId)) return activeSheetId;
    return model.sheets[0].id;
  }, [activeSheetId, model]);

  const effectiveSheetId = activeSheetId ?? fallbackActiveSheetId;
  const tableStyleVisible = !!(tableStyleContext && activeSheetId === tableStyleContext.sheetId && range &&
    range.startRow <= tableStyleContext.range.endRow && range.endRow >= tableStyleContext.range.startRow &&
    range.startColumn <= tableStyleContext.range.endColumn && range.endColumn >= tableStyleContext.range.startColumn);
  useEffect(() => { if (!tableStyleVisible && ribbonTab === "tableStyle") setRibbonTab("home"); }, [tableStyleVisible, ribbonTab]);
  const applyTableStyle = async (preset: TableStylePreset, options: TableStyleOptions, target = range) => {
    if (!activeSheetId || !target) return;
    const previous = tableStyleContext;
    const updatingContext = previous?.range === target && previous.sheetId === activeSheetId;
    if (updatingContext) setTableStyleContext({ ...previous, preset, options });
    if (await submit(tableStyleCommands(activeSheetId, target, preset, options))) {
      setTableStyleContext({ sheetId: activeSheetId, range: target, preset, options });
      setRibbonTab("tableStyle");
    } else if (updatingContext) setTableStyleContext(previous);
  };

  const closeRowMenu = () => { setRowMenu(null); document.querySelector<HTMLElement>(".ss-grid__viewport")?.focus(); };
  const updateRows = (startRow: number, endRow: number, patch: { height?: number; resetHeight?: boolean; hidden?: boolean }) => {
    if (activeSheetId && !saving) void submit([setRowLayoutCommand(activeSheetId, startRow, endRow, patch)]);
  };
  const rowMenuAction = (action: RowMenuAction, count = 1) => {
    if (!rowMenu || !activeSheetId || saving) return;
    const { startRow, endRow } = rowMenu;
    closeRowMenu();
    if (action === "copy") copySelection();
    else if (action === "cut") cutSelection();
    else if (action === "paste" || action === "pasteValues" || action === "pasteFormats") void pasteClipboard(action === "pasteValues" ? "values" : action === "pasteFormats" ? "formats" : "all");
    else if (action === "insertAbove" || action === "insertBelow") void submit([insertRowsCommand(activeSheetId, action === "insertAbove" ? startRow : endRow + 1, count)]);
    else if (action === "delete") void submit([deleteRowsCommand(activeSheetId, startRow, endRow - startRow + 1)]);
    else if (action === "hide") updateRows(startRow, endRow, { hidden: true });
    else if (action === "unhide") {
      const hidden = new Set(sheet?.metadata.rowLayout?.filter(entry => entry.hidden).map(entry => entry.row));
      let first = startRow, last = endRow;
      while (first > 0 && hidden.has(first - 1)) first--;
      while (hidden.has(last + 1)) last++;
      updateRows(first, last, { hidden: false });
    }
    else if (action === "autoHeight") updateRows(startRow, endRow, { resetHeight: true });
    else if (action === "height" || action === "format") {
      setRowInput(action === "height" ? String(sheet?.metadata.rowLayout?.find(entry => entry.row === startRow)?.height ?? 21) : "General");
      setRowDialog({ type: action, startRow, endRow });
    }
    else if (action === "clearContents") clearSelectionContents();
    else if (action === "clearFormats") clearSelectionFormatting();
    else if (action === "clearAll" && range) void submit([clearRangeCommand(activeSheetId, range, "all")]);
    else if (action === "merge") toggleMerge();
    else if (action === "link") {
      const url = new URL(window.location.href); url.hash = new URLSearchParams({ sheet: activeSheetId, rows: `${startRow}:${endRow}` }).toString();
      void navigator.clipboard.writeText(url.toString()).then(() => setMenuNotice("范围链接已复制"), () => setMenuNotice("无法访问系统剪贴板，请允许浏览器复制权限"));
    }
  };
  useEffect(() => {
    if (!model) return;
    const hash = new URLSearchParams(window.location.hash.slice(1));
    const linkedSheet = hash.get("sheet"), rows = hash.get("rows")?.match(/^(\d+):(\d+)$/);
    if (!rows || !model.sheets.some(sheet => sheet.id === linkedSheet)) return;
    const first = Number(rows[1]), last = Number(rows[2]);
    if (first > last || last >= (model.sheets.find(sheet => sheet.id === linkedSheet)?.metadata.rowCount ?? DEFAULT_ROWS)) return;
    setActiveSheetId(linkedSheet); setSelection({ kind: "row", row: first, endRow: last }); setActiveCell({ row: first, column: 0 }); setSearchDestination({ row: first, column: 0 });
    // Read a deep link once per document, not on each persisted edit.
  }, [id, !!model]);

  const rangeLabel = range && (range.startRow !== range.endRow || range.startColumn !== range.endColumn)
    ? `${cellRef(range.startRow, range.startColumn)}:${cellRef(range.endRow, range.endColumn)}`
    : null;

  // Wait for the capability catalog before the first paint: both requests hit the
  // same server, and rendering the ribbon before the catalog arrives would flash
  // engine-backed controls in and out as the gate settles.
  if (loading || !capabilitiesLoaded) return <MainShell onBack={onBack} title={title}><p className="ss-status">正在加载表格…</p></MainShell>;
  if (!model) return <MainShell onBack={onBack} title={title}><p className="ss-status">{error ?? "无法加载表格。"}</p></MainShell>;

  return (
    <main className="ss" aria-label="电子表格编辑器" onKeyDown={handleKeyDown}>
      <header className="ss__header">
        <button className="ss__back" type="button" onClick={onBack}>‹ 所有文件</button>
        <div className="ss__title">
          <span className="ss__eyebrow">电子表格</span>
          <strong>{title || "未命名表格"}</strong>
        </div>
        <span className={`ss__status${saving ? " is-saving" : ""}`} aria-live="polite">
          {saving ? "正在保存…" : `revision ${revision}`}
        </span>
        <button type="button" className="ss__refresh" onClick={() => void session.refresh()}>刷新</button>
      </header>

      {error && <div className="ss__error" role="alert">{error}</div>}

      <SpreadsheetToolbar
        disabled={saving}
        availableCapabilities={availableCapabilities}
        styleState={toolbarStyleState(currentStyle(focusedCell.row, focusedCell.column))}
        selectionActive={selection !== null}
        canMergeToggle={canMerge || activeCellInsideMerge !== null}
        mergeActive={activeCellInsideMerge !== null}
        filterActive={filterActive}
        frozen={isFrozen}
        canUndo={canUndo}
        canRedo={canRedo}
        clipboardHasContent={clipboardHasContent}
        tab={ribbonTab}
        onTabChange={setRibbonTab}
        onUndo={() => void submitHistory("undo")}
        onRedo={() => void submitHistory("redo")}
        onCopy={() => void copySelection()}
        onCut={cutSelection}
        onPaste={() => void pasteClipboard()}
        onClearContents={clearSelectionContents}
        onClearFormatting={clearSelectionFormatting}
        onTableStyle={(preset, options) => void applyTableStyle(preset, options)}
        tableStyleContext={tableStyleVisible ? tableStyleContext : null}
        onContextTableStyle={(preset, options) => { if (tableStyleContext) void applyTableStyle(preset, options, tableStyleContext.range); }}
        onClearTableStyle={() => { if (tableStyleContext) void submit([formatRangeCommand(tableStyleContext.sheetId, tableStyleContext.range, EMPTY_CELL_STYLE, ["fillBackground", "fontColor", "fontBold", "borders"]), ...(tableStyleContext.options.filter ? [setAutoFilterCommand(tableStyleContext.sheetId, null)] : [])]).then(ok => { if (ok) { setTableStyleContext(null); setRibbonTab("home"); } }); }}
        onFontFamily={applyFontFamily}
        onFontSize={applyFontSize}
        onBold={toggleBold}
        onItalic={toggleItalic}
        onStrikethrough={() => toggleDecoration("strikethrough")}
        onUnderline={() => toggleDecoration("underline")}
        onFontColor={applyFontColor}
        onFillColor={applyFill}
        onBorderPreset={applyBorderPreset}
        onAlignHorizontal={applyAlign}
        onAlignVertical={applyVerticalAlign}
        onWrap={toggleWrap}
        onMergeToggle={toggleMerge}
        onMergeCenter={() => { if (activeSheetId && range && canMerge) void submit([mergeCellsCommand(activeSheetId, range), formatRangeCommand(activeSheetId, range, { ...EMPTY_CELL_STYLE, alignment: mergeAlignment(null, { horizontal: "center" }) }, ["horizontalAlignment"])]); }}
        onNumberFormat={applyNumberFormat}
        onFindReplace={() => setFindReplaceOpen(true)}
        onSort={sortSelection}
        onFilterToggle={toggleFilter}
        onFreezeToggle={toggleFreeze}
        onAutoSum={autoSumSelection}
        onUpsertConditionalFormat={applyConditionalFormat}
        onDeleteConditionalFormat={removeConditionalFormat}
        onClearConditionalFormats={() => { if (activeSheetId && sheet) void submit(sheet.metadata.conditionalFormats.map(rule => deleteConditionalFormatCommand(activeSheetId, rule.id))); }}
        conditionalFormatRules={conditionalFormatRules}
        filterRule={filterRule}
        onApplyFilterRule={applyFilterRule}
        onClearFilterRule={clearFilterRule}
        dataValidationRules={dataValidationRules}
        onApplyDataValidation={applyDataValidation}
        onDeleteDataValidation={removeDataValidation}
        onPaintFormatToggle={togglePaintFormat}
        paintFormatArmed={paintStyle !== null}
        onInsertRowAbove={() => insertRow(focusedCell.row)}
        onInsertRowBelow={() => insertRow(focusedCell.row + 1)}
        onDeleteRow={() => deleteRow(rowForDelete)}
        onInsertColumnLeft={() => insertColumn(focusedCell.column)}
        onInsertColumnRight={() => insertColumn(focusedCell.column + 1)}
        onDeleteColumn={() => deleteColumn(columnForDelete)}
      />

      <div className="ss__formula-bar">
        <span className="ss__formula-cell">{activeLabel}</span>
        <span className="ss__formula-fn">ƒx</span>
        <input
          className="ss__formula-input"
          value={formulaBarValue}
          placeholder="输入值或 =公式"
          onChange={(event) => {
            setEditingValue(event.target.value);
            if (!editing) setEditing(null);
          }}
          onKeyDown={handleFormulaBarKey}
          aria-label="公式栏"
        />
      </div>

      {findReplaceOpen && <SpreadsheetFindPanel cells={sheet?.cells ?? []} selection={range} saving={saving}
        onSelect={cell => { setSearchDestination({ row: cell.row, column: cell.column }); setActiveCell(cell); setSelection({ kind: "cells", anchor: cell, focus: cell }); }}
        onReplace={replaceMatches} onClose={() => setFindReplaceOpen(false)} />}

      {rowMenu && <SpreadsheetRowMenu {...rowMenu} busy={saving} canPaste={clipboardHasContent} canMerge={canMerge} onAction={rowMenuAction} onClose={closeRowMenu} />}
      {rowDialog && <div className="ss__row-dialog" onPointerDown={event => { if (event.target === event.currentTarget) setRowDialog(null); }}>
        <form role="dialog" aria-modal="true" aria-label={rowDialog.type === "height" ? "设置行高" : "设置单元格格式"} onKeyDown={event => {
          event.stopPropagation(); if (event.key === "Escape") { event.preventDefault(); setRowDialog(null); }
          if (event.key === "Tab") { const fields = Array.from(event.currentTarget.querySelectorAll<HTMLElement>("input, select, button")); const index = fields.indexOf(document.activeElement as HTMLElement); if (event.shiftKey && index === 0 || !event.shiftKey && index === fields.length - 1) { event.preventDefault(); fields[event.shiftKey ? fields.length - 1 : 0]?.focus(); } }
        }} onSubmit={event => { event.preventDefault();
          if (rowDialog.type === "height") updateRows(rowDialog.startRow, rowDialog.endRow, { height: Number(rowInput) });
          else applyNumberFormat(rowInput);
          setRowDialog(null); document.querySelector<HTMLElement>(".ss-grid__viewport")?.focus();
        }}>
          <h2>{rowDialog.type === "height" ? "设置行高" : "设置单元格格式"}</h2>
          {rowDialog.type === "height" ? <label>行高（磅）<input autoFocus required type="number" min="0.75" max="409.5" step="0.25" value={rowInput} onChange={event => setRowInput(event.target.value)} /></label>
            : <label>数字格式<select autoFocus value={rowInput} onChange={event => setRowInput(event.target.value)}><option value="General">常规</option><option value="0.00">数值</option><option value="¥#,##0.00">货币</option><option value="0.00%">百分比</option><option value="yyyy-mm-dd">日期</option><option value="@">文本</option></select></label>}
          <footer><button type="button" onClick={() => setRowDialog(null)}>取消</button><button type="submit" disabled={saving}>确定</button></footer>
        </form>
      </div>}

      {contextMenu && (
        <>
          <div
            className="ss__ctxmenu-overlay"
            onPointerDown={() => setContextMenu(null)}
            onContextMenu={(event) => { event.preventDefault(); setContextMenu(null); }}
          />
          <div
            className="ss__ctxmenu"
            role="menu"
            style={{ left: contextMenu.x, top: contextMenu.y }}
            onPointerDown={(event) => event.stopPropagation()}
          >
            <button role="menuitem" onClick={() => { copySelection(); setContextMenu(null); }}>复制</button>
            <button role="menuitem" onClick={() => { cutSelection(); setContextMenu(null); }}>剪切</button>
            <button role="menuitem" onClick={() => { void pasteClipboard(); setContextMenu(null); }}>粘贴</button>
            <span className="ss__separator" />
            <button role="menuitem" onClick={() => { clearSelectionContents(); setContextMenu(null); }}>清除内容</button>
            <button role="menuitem" onClick={() => { clearSelectionFormatting(); setContextMenu(null); }}>清除格式</button>
            <span className="ss__separator" />
            <button role="menuitem" onClick={() => { insertRow(focusedCell.row); setContextMenu(null); }}>在上方插入行</button>
            <button role="menuitem" onClick={() => { insertRow(focusedCell.row + 1); setContextMenu(null); }}>在下方插入行</button>
            <button role="menuitem" onClick={() => { deleteRow(rowForDelete); setContextMenu(null); }}>删除行</button>
            <span className="ss__separator" />
            <button role="menuitem" onClick={() => { insertColumn(focusedCell.column); setContextMenu(null); }}>在左侧插入列</button>
            <button role="menuitem" onClick={() => { insertColumn(focusedCell.column + 1); setContextMenu(null); }}>在右侧插入列</button>
            <button role="menuitem" onClick={() => { deleteColumn(columnForDelete); setContextMenu(null); }}>删除列</button>
            <span className="ss__separator" />
            <button role="menuitem" disabled={!range} onClick={() => { sortSelection("ascending"); setContextMenu(null); }}>升序排序</button>
            <button role="menuitem" disabled={!range} onClick={() => { sortSelection("descending"); setContextMenu(null); }}>降序排序</button>
          </div>
        </>
      )}

      <div className="ss__workspace">
        {sheet && (<SpreadsheetGrid
          revealCell={searchDestination}
          id={id}
          sheetId={sheet.id}
          sheet={sheet}
          revision={revision}
          selection={selection}
          editing={editing}
          canEdit={!saving}
          editingInitial={editingInitial}
          onCellContextMenu={(row, column, position) => {
            setRowMenu(null);
            selectCell(row, column, false);
            setContextMenu(position);
          }}
          onRowContextMenu={(row, position) => {
            setContextMenu(null);
            const preserve = selection?.kind === "row" && range && row >= range.startRow && row <= range.endRow;
            const startRow = preserve ? range.startRow : row, endRow = preserve ? range.endRow : row;
            setEditing(null); setSelection({ kind: "row", row: startRow, endRow }); setActiveCell({ row: startRow, column: 0 });
            setRowMenu({ ...position, startRow, endRow });
          }}
          onUnhideRows={(first, last) => updateRows(first, last, { hidden: false })}
          onSelect={selectCell}
          onSelectRow={selectRow}
          onSelectColumn={selectColumn}
          onSelectAll={selectAll}
          onActivate={activateCell}
          onEditCommit={commitEdit}
          onEditCancel={() => setEditing(null)}
          onProjectionCells={receiveProjectionCells}
        />)}
        {!sheet && <p className="ss-status">请选择一个工作表。</p>}
      </div>

      <div className="ss__statusbar" aria-live="polite">
        <span>单元格 {activeLabel}</span>
        {menuNotice && <span>{menuNotice}</span>}
        {rangeLabel && <span>选区 {rangeLabel}</span>}
        <span className="ss__statusbar-spacer" />
        <span>{model.sheets.length} 个工作表</span>
        <span>revision {revision}</span>
      </div>

      <footer className="ss__sheetbar">
        {model.sheets.map((sheetItem) => (
          <div
            key={sheetItem.id}
            className={`ss__sheet-tab${sheetItem.id === effectiveSheetId ? " is-active" : ""}`}
            onClick={() => {
              setSelection(null);
              setEditing(null);
              setActiveSheetId(sheetItem.id);
            }}
          >
            {renameSheetId === sheetItem.id ? (
              <input
                className="ss__sheet-rename"
                defaultValue={sheetItem.name}
                autoFocus
                onClick={(event) => event.stopPropagation()}
                onBlur={(event) => renameSheet(sheetItem.id, event.target.value)}
                onKeyDown={(event) => {
                  if (event.key === "Enter") event.currentTarget.blur();
                  if (event.key === "Escape") setRenameSheetId(null);
                }}
              />
            ) : (
              <span
                title="双击重命名"
                onDoubleClick={(event) => {
                  event.stopPropagation();
                  setRenameSheetId(sheetItem.id);
                }}
              >
                {sheetItem.name}
              </span>
            )}
            {model.sheets.length > 1 && (
              <button className="ss__sheet-close" title="删除工作表" onClick={(event) => { event.stopPropagation(); deleteSheet(sheetItem.id); }}>×</button>
            )}
          </div>
        ))}
        <button className="ss__sheet-add" title="新建工作表" onClick={addSheet}>＋</button>
      </footer>
    </main>
  );
}

function MainShell({ onBack, title, children }: { onBack: () => void; title: string; children: ReactNode }) {
  return (
    <main className="ss" aria-label="电子表格编辑器">
      <header className="ss__header">
        <button className="ss__back" type="button" onClick={onBack}>‹ 所有文件</button>
        <div className="ss__title">
          <span className="ss__eyebrow">电子表格</span>
          <strong>{title || "未命名表格"}</strong>
        </div>
      </header>
      <div className="ss__workspace">{children}</div>
      <div className="ss__statusbar" />
      <footer className="ss__sheetbar" />
    </main>
  );
}

function navKeyFromEvent(event: ReactKeyboardEvent): NavKey | null {
  switch (event.key) {
    case "ArrowUp": return "up";
    case "ArrowDown": return "down";
    case "ArrowLeft": return "left";
    case "ArrowRight": return "right";
    case "Home": return "home";
    case "End": return "end";
    case "PageUp": return "pageUp";
    case "PageDown": return "pageDown";
    case "Tab": return event.shiftKey ? "shiftTab" : "tab";
    default: return null;
  }
}

function parseCellValue(text: string): unknown {
  const trimmed = text.trim();
  if (/^[+-]?\d+(\.\d+)?$/.test(trimmed)) return Number(trimmed);
  if (trimmed === "TRUE") return true;
  if (trimmed === "FALSE") return false;
  return text;
}

function cellDisplayValue(value: unknown): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "string") return value;
  if (typeof value === "number") return String(value);
  if (typeof value === "boolean") return value ? "TRUE" : "FALSE";
  return String(value);
}
