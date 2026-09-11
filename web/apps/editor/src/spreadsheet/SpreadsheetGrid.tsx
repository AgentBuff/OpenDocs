import { spreadsheetRowMetrics } from "./row-metrics.js";
import {
  type PointerEvent as ReactPointerEvent,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import type { SheetModel } from "@open-office/schema/artifact";
import type {
  SpreadsheetCalculatedValue,
  SpreadsheetGridCell,
} from "@open-office/schema/api";
import { OpenOfficeSdk } from "@open-office/sdk";

import {
  activeCell,
  mergedAnchor,
  visibleGridCells,
  cellRef,
  formatDisplayValue,
  rangeRef,
  selectionRange,
  type CellCoordinate,
  type CellSelection,
} from "@open-office/spreadsheet-ui";

const COL_WIDTH = 100;
const ROW_HEADER_WIDTH = 48;
const COL_HEADER_HEIGHT = 28;
const BUFFER = 6;
const DEFAULT_ROW_COUNT = 1200;
const DEFAULT_COLUMN_COUNT = 104;

const sdk = new OpenOfficeSdk();

export interface SpreadsheetGridProps {
  id: string;
  sheetId: string;
  sheet: SheetModel;
  revision: number;
  selection: CellSelection | null;
  revealCell?: CellCoordinate | null;
  editing: CellCoordinate | null;
  canEdit: boolean;
  onSelect: (row: number, column: number, extend: boolean) => void;
  onSelectRow: (row: number, extend?: boolean) => void;
  onRowContextMenu?: (row: number, position: { x: number; y: number }) => void;
  onUnhideRows?: (startRow: number, endRow: number) => void;
  onSelectColumn: (column: number) => void;
  onSelectAll: () => void;
  onActivate: (row: number, column: number) => void;
  onEditCommit: (text: string) => void;
  onEditCancel: () => void;
  onProjectionCells?: (cells: readonly SpreadsheetGridCell[]) => void;
  /** 编辑框起始文本覆盖（直接打字进入编辑时 = 首个字符）。 */
  editingInitial?: string | null;
  /** 单元格右键菜单。 */
  onCellContextMenu?: (row: number, column: number, position: { x: number; y: number }) => void;
}

function visibleColumns(total: number, scrollLeft: number, width: number): [number, number] {
  const start = Math.max(0, Math.floor(scrollLeft / COL_WIDTH));
  const end = Math.min(total, Math.ceil((scrollLeft + width) / COL_WIDTH) + BUFFER);
  return [start, end];
}


function cellContent(cell: SpreadsheetGridCell | undefined, computed: SpreadsheetCalculatedValue | undefined): unknown {
  if (!cell?.formula) return cell?.value ?? null;
  if (!computed || computed.type === "blank") return null;
  if (computed.type === "error") return `#${computed.value.code}`;
  // Preserve calculated numbers and booleans until number-format rendering.
  return computed.value;
}

export function SpreadsheetGrid({
  id,
  sheetId,
  sheet,
  revision,
  selection,
  editing,
  canEdit,
  onSelect,
  onSelectRow,
  onSelectColumn,
  onSelectAll,
  onActivate,
  onEditCommit,
  onEditCancel,
  onProjectionCells,
  editingInitial,
  onCellContextMenu,
  onRowContextMenu,
  onUnhideRows,
  revealCell,
}: SpreadsheetGridProps) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const [scroll, setScroll] = useState({ top: 0, left: 0 });
  const [viewportSize, setViewportSize] = useState({ width: 800, height: 600 });
  const [cells, setCells] = useState<ReadonlyMap<string, SpreadsheetGridCell>>(new Map());
  const [values, setValues] = useState<ReadonlyMap<string, SpreadsheetCalculatedValue>>(new Map());
  /** 条件格式命中（服务端求值）：`"row:column"` -> 规则 id 列表。 */
  const [conditionalHits, setConditionalHits] = useState<ReadonlyMap<string, string[]>>(new Map());
  /** 被筛选谓词排除的行号集合。 */
  const [filteredRows, setFilteredRows] = useState<ReadonlySet<number>>(new Set());
  const [dragging, setDragging] = useState(false);
  const pendingWindow = useRef<{ sheetId: string; revision: number; windowKey: string } | null>(null);
  const projectionScope = useRef({ id, sheetId, revision });
  projectionScope.current = { id, sheetId, revision };

  // Cell caches are sheet- and revision-scoped. A stale cell from a previous
  // sheet or a previous commit must never linger after the snapshot changes.
  useEffect(() => {
    setCells(new Map());
    setValues(new Map());
    setConditionalHits(new Map());
    setFilteredRows(new Set());
  }, [revision]);

  useEffect(() => {
    setCells(new Map());
    setValues(new Map());
    setConditionalHits(new Map());
    setFilteredRows(new Set());
    setScroll({ top: 0, left: 0 });
    const viewport = viewportRef.current;
    if (viewport) {
      viewport.scrollTop = 0;
      viewport.scrollLeft = 0;
    }
  }, [sheetId]);

  const totalRows = Math.max(DEFAULT_ROW_COUNT, sheet.metadata.rowCount ?? DEFAULT_ROW_COUNT);
  const totalColumns = Math.max(DEFAULT_COLUMN_COUNT, sheet.metadata.columnCount ?? DEFAULT_COLUMN_COUNT);
  const merged = sheet.metadata.mergedRanges;
  const rowMetrics = useMemo(() => spreadsheetRowMetrics(totalRows, sheet.cells, sheet.metadata.rowLayout), [totalRows, sheet.cells, sheet.metadata.rowLayout]);
  const bounds = useMemo(() => ({ rows: totalRows, columns: totalColumns }), [totalColumns, totalRows]);

  const range = useMemo(() => selectionRange(selection, bounds, merged), [bounds, selection, merged]);
  const active = useMemo(() => { const point = activeCell(selection); return point ? mergedAnchor(point, merged) : null; }, [selection, merged]);

  // Reveal a match or keyboard destination without stealing focus from the search panel.
  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport || !revealCell) return;
    const top = rowMetrics.top(revealCell.row), left = revealCell.column * COL_WIDTH;
    const height = viewport.clientHeight - COL_HEADER_HEIGHT;
    const width = viewport.clientWidth - ROW_HEADER_WIDTH;
    if (top < viewport.scrollTop) viewport.scrollTop = top;
    else if (top + rowMetrics.height(revealCell.row) > viewport.scrollTop + height) viewport.scrollTop = top + rowMetrics.height(revealCell.row) - height;
    if (left < viewport.scrollLeft) viewport.scrollLeft = left;
    else if (left + COL_WIDTH > viewport.scrollLeft + width) viewport.scrollLeft = left + COL_WIDTH - width;
  }, [revealCell, rowMetrics]);

  const visibleStartRow = rowMetrics.rowAt(scroll.top);
  const visibleEndRow = Math.min(totalRows, rowMetrics.rowAt(scroll.top + viewportSize.height) + 1 + BUFFER);
  const [visibleStartColumn, visibleEndColumn] = visibleColumns(totalColumns, scroll.left, viewportSize.width);

  // Measure the viewport once so the first projection is correct before any scroll.
  useEffect(() => {
    const el = viewportRef.current;
    if (!el) return;
    const resize = () => {
      setViewportSize({ width: Math.max(80, el.clientWidth), height: Math.max(80, el.clientHeight) });
    };
    resize();
    const observer = new ResizeObserver(resize);
    observer.observe(el);
    return () => observer.disconnect();
  }, []);

  const loadWindow = useCallback((startRow: number, endRow: number, startColumn: number, endColumn: number) => {
    if (endRow <= startRow || endColumn <= startColumn) return;
    // Deduplicate the same window while allowing a newer scroll window to load.
    const windowKey = `${startRow}:${endRow}:${startColumn}:${endColumn}`;
    if (pendingWindow.current?.sheetId === sheetId && pendingWindow.current.revision === revision && pendingWindow.current.windowKey === windowKey) return;
    const request = { sheetId, revision, windowKey };
    pendingWindow.current = request;
    void sdk
      .spreadsheet(id, { sheetId, startRow, endRow, startColumn, endColumn })
      .then((envelope) => {
        const current = projectionScope.current;
        if (current.id !== id || current.sheetId !== sheetId || current.revision !== revision || envelope.revision !== revision) return;
        const next = envelope.data;
        onProjectionCells?.(next.cells);
        setCells((current) => {
          const nextMap = new Map(current);
          for (const cell of next.cells) {
            nextMap.set(`${cell.address.row}:${cell.address.column}`, cell);
          }
          return nextMap;
        });
        setValues((current) => {
          const nextMap = new Map(current);
          for (const [key, value] of Object.entries(next.values)) {
            nextMap.set(key, value);
          }
          return nextMap;
        });
        // 条件格式命中与被筛行随窗口推进（服务端求值，前端只消费）。
        setConditionalHits((current) => {
          const nextMap = new Map(current);
          for (const [key, ruleIds] of Object.entries(next.conditionalStyles)) {
            nextMap.set(key, ruleIds);
          }
          return nextMap;
        });
        setFilteredRows(new Set(next.filteredOutRows));
      })
      .catch(() => undefined)
      .finally(() => {
        if (pendingWindow.current === request) pendingWindow.current = null;
      });
  }, [id, onProjectionCells, revision, sheetId]);

  // Request the window that matches the current scroll offset, debounced by scroll settle.
  const currentWindowStartRow = Math.max(0, visibleStartRow - BUFFER);
  const currentWindowStartColumn = Math.max(0, visibleStartColumn - BUFFER);
  const currentWindowEndRow = Math.min(totalRows, visibleEndRow + BUFFER);
  const currentWindowEndColumn = Math.min(totalColumns, visibleEndColumn + BUFFER);

  useEffect(() => {
    const timer = window.setTimeout(() => {
      loadWindow(currentWindowStartRow, currentWindowEndRow, currentWindowStartColumn, currentWindowEndColumn);
    }, 40);
    return () => window.clearTimeout(timer);
  }, [currentWindowEndColumn, currentWindowEndRow, currentWindowStartColumn, currentWindowStartRow, loadWindow, sheetId]);

  const handleScroll = useCallback(() => {
    const el = viewportRef.current;
    if (!el) return;
    setScroll({ top: el.scrollTop, left: el.scrollLeft });
  }, []);

  // Render buffer window rows and columns that are actually materialized.
  const renderRows = useMemo(() => {
    const rows: number[] = [];
    for (let row = visibleStartRow; row < visibleEndRow; row += 1) if (rowMetrics.height(row) > 0) rows.push(row);
    return rows;
  }, [visibleEndRow, visibleStartRow, rowMetrics]);

  const renderColumns = useMemo(() => {
    const columns: number[] = [];
    for (let column = visibleStartColumn; column < visibleEndColumn; column += 1) columns.push(column);
    return columns;
  }, [visibleEndColumn, visibleStartColumn]);

  const renderCells = useMemo(() => visibleGridCells({ startRow: visibleStartRow, endRow: visibleEndRow - 1, startColumn: visibleStartColumn, endColumn: visibleEndColumn - 1 }, merged), [merged, visibleStartRow, visibleEndRow, visibleStartColumn, visibleEndColumn]);
  const mergeAnchors = useMemo(() => new Map(merged.map((entry) => [`${entry.startRow}:${entry.startColumn}`, entry])), [merged]);
  // A merged rectangle can remain visible while its anchor is outside the
  // ordinary projection window. Fetch only those anchor cells, not the gap.
  const offscreenAnchors = renderCells.filter(({ row, column }) => row < currentWindowStartRow || column < currentWindowStartColumn)
    .map(({ row, column }) => `${row}:${column}`).join(",");
  useEffect(() => {
    if (!offscreenAnchors) return;
    let cancelled = false;
    void Promise.all(offscreenAnchors.split(",").map(async (key) => {
      const [row, column] = key.split(":").map(Number);
      const envelope = await sdk.spreadsheet(id, { sheetId, startRow: row, endRow: row + 1, startColumn: column, endColumn: column + 1 });
      if (cancelled || envelope.revision !== revision) return;
      const next = envelope.data;
      onProjectionCells?.(next.cells);
      setCells((current) => { const result = new Map(current); for (const cell of next.cells) result.set(`${cell.address.row}:${cell.address.column}`, cell); return result; });
      setValues((current) => new Map([...current, ...Object.entries(next.values)]));
      setConditionalHits((current) => new Map([...current, ...Object.entries(next.conditionalStyles)]));
    })).catch(() => undefined);
    return () => { cancelled = true; };
  }, [id, onProjectionCells, sheetId, revision, offscreenAnchors]);

  // Column header and row header strips mirror the body scroll offset.
  const columnHeaderOffset = scroll.left;
  const rowHeaderOffset = scroll.top;

  const cellStyle = useCallback((row: number, column: number) => {
    const cell = cells.get(`${row}:${column}`);
    // 条件格式命中样式优先于静态背景/字色（服务端求值的规则 id 列表）。
    const hits = conditionalHits.get(`${row}:${column}`);
    const base: React.CSSProperties = {};
    if (!cell?.style) {
      if (hits && hits.length > 0) {
        base.backgroundColor = "#fff1f0";
        base.color = "#f53f3f";
        base.fontWeight = "bold";
      }
      return base;
    }
    if (cell.style.font?.family) base.fontFamily = cell.style.font.family;
    if (cell.style.font?.size != null) base.fontSize = `${cell.style.font.size}pt`;
    if (cell.style.font?.bold) base.fontWeight = "bold";
    if (cell.style.font?.italic) base.fontStyle = "italic";
    if (cell.style.font?.color) base.color = cell.style.font.color;
    if (cell.style.font?.underline || cell.style.font?.strikethrough) {
      const lines = [
        cell.style.font?.underline ? "underline" : null,
        cell.style.font?.strikethrough ? "line-through" : null,
      ].filter(Boolean);
      base.textDecorationLine = lines.join(" ");
    }
    if (cell.style.fill?.background) base.backgroundColor = cell.style.fill.background;
    if (cell.style.alignment?.horizontal === "center") base.textAlign = "center";
    if (cell.style.alignment?.horizontal === "right") base.textAlign = "right";
    if (cell.style.alignment?.vertical === "top") base.justifyContent = "flex-start";
    if (cell.style.alignment?.vertical === "bottom") base.justifyContent = "flex-end";
    if (cell.style.alignment?.wrap) {
      base.whiteSpace = "pre-wrap";
      base.wordBreak = "break-word";
    }
    // 边框覆盖默认网格线：只渲染有样式的边，粗细映射常见词表。
    const borders = cell.style.borders;
    if (borders) {
      const widthOf = (style: string | null) =>
        style === "thick" || style === "double" ? 3 : style === "medium" ? 2 : style === "hair" ? 1 : 1;
      const lineOf = (style: string | null) =>
        style === "dashed" ? "dashed" : style === "dotted" ? "dotted" : style === "double" ? "double" : "solid";
      const edge = (side: "top" | "bottom" | "left" | "right") => {
        const value = borders[side];
        if (!value?.style && !value?.color) return undefined;
        return `${value.style ? widthOf(value.style) : 1}px ${lineOf(value.style)} ${value.color ?? "#1f2329"}`;
      };
      if (borders.top) base.borderTop = edge("top");
      if (borders.bottom) base.borderBottom = edge("bottom");
      if (borders.left) base.borderLeft = edge("left");
      if (borders.right) base.borderRight = edge("right");
    }
    if (hits && hits.length > 0) {
      base.backgroundColor = "#fff1f0";
      base.color = "#f53f3f";
      base.fontWeight = "bold";
    }
    return base;
  }, [cells, conditionalHits]);

  const pointerDown = useCallback((event: ReactPointerEvent, row: number, column: number) => {
    if (!canEdit || event.button !== 0) return;
    event.preventDefault();
    const extend = event.shiftKey;
    onSelect(row, column, extend);
    setDragging(true);
    event.currentTarget.setPointerCapture?.(event.pointerId);
    // 格子不可聚焦；把焦点放到视口上，键盘事件（打字/导航/快捷键）才能
    // 冒泡进 Studio 的 onKeyDown。
    viewportRef.current?.focus();
  }, [canEdit, onSelect]);

  const pointerMove = useCallback((event: ReactPointerEvent) => {
    if (!dragging) return;
    const target = document.elementFromPoint(event.clientX, event.clientY) as HTMLElement | null;
    const anchor = target?.closest<HTMLElement>("[data-cell]")?.dataset.cell;
    if (anchor) {
      const { row, column } = parseCellAnchor(anchor);
      if (selection) onSelect(row, column, true);
    }
  }, [dragging, onSelect, selection]);

  const pointerUp = useCallback(() => {
    setDragging(false);
  }, []);

  return (
    <div className="ss-grid">
      <div className="ss-grid__corner" role="button" aria-label="全选" onClick={onSelectAll} onPointerDown={(event) => event.stopPropagation()} />
      <div className="ss-grid__colhead" aria-hidden="true">
        {/* 内层承担平移：条带自身保持剪裁不动，transform 连剪裁框一起移动会让表头滑走。 */}
        <div className="ss-grid__headlayer" style={{ transform: `translateX(${-columnHeaderOffset}px)` }}>
          {renderColumns.map((column) => (
            <div
              key={column}
              className={`ss-grid__colhead-cell${range && range.startColumn === column && range.endColumn === column && range.startRow === 0 ? " is-highlighted" : ""}`}
              role="button"
              style={{ left: column * COL_WIDTH, width: COL_WIDTH, height: COL_HEADER_HEIGHT }}
              title={`第 ${column + 1} 列`}
              onClick={(event) => { event.stopPropagation(); onSelectColumn(column); }}
            >
              {columnToLabel(column)}
            </div>
          ))}
        </div>
      </div>
      <div className="ss-grid__rowhead">
        <div className="ss-grid__headlayer" style={{ transform: `translateY(${-rowHeaderOffset}px)` }}>
          {renderRows.map((row) => (
            <div
              key={row}
              className={`ss-grid__rowhead-cell${range && selection?.kind === "row" && range.startRow <= row && range.endRow >= row ? " is-highlighted" : ""}`}
              role="button"
              style={{ top: rowMetrics.top(row), height: rowMetrics.height(row), width: ROW_HEADER_WIDTH }}
              data-row={row}
              aria-label={`第 ${row + 1} 行`}
              tabIndex={0}
              onClick={(event) => { event.stopPropagation(); onSelectRow(row, event.shiftKey); viewportRef.current?.focus(); }}
              onContextMenu={(event) => { event.preventDefault(); onRowContextMenu?.(row, { x: event.clientX, y: event.clientY }); }}
              onKeyDown={(event) => {
                if (event.key === "Enter" || event.key === " ") { event.preventDefault(); onSelectRow(row, event.shiftKey); }
                if (event.key === "ContextMenu" || (event.shiftKey && event.key === "F10")) {
                  event.preventDefault(); const rect = event.currentTarget.getBoundingClientRect(); onRowContextMenu?.(row, { x: rect.right, y: rect.top });
                }
              }}
            >
              {row + 1}
              {row > 0 && rowMetrics.height(row - 1) === 0 && <button type="button" className="ss-grid__unhide" title="显示上方隐藏行" aria-label="显示上方隐藏行" disabled={!canEdit} onClick={(event) => {
                event.stopPropagation(); let first = row - 1; while (first > 0 && rowMetrics.height(first - 1) === 0) first--; onUnhideRows?.(first, row - 1);
              }}>↕</button>}
            </div>
          ))}
        </div>
      </div>
      <div className="ss-grid__viewport" ref={viewportRef} tabIndex={0} onScroll={handleScroll}>
        <div
          className="ss-grid__canvas"
          style={{ width: totalColumns * COL_WIDTH, height: rowMetrics.top(totalRows) }}
          onPointerUp={pointerUp}
          onPointerMove={pointerMove}
        >
          {renderCells.map(({ row, column }) => {
              if (filteredRows.has(row) || rowMetrics.height(row) === 0 && !mergeAnchors.has(`${row}:${column}`)) return null;
              const key = `${row}:${column}`;
              const cell = cells.get(key);
              const computed = values.get(key);
              const rawContent = cellContent(cell, computed);
              // 数字格式是显示层派生：canonical 值不因格式而变。
              const content = formatDisplayValue(
                rawContent,
                cell?.style?.numberFormat,
              );
              const inSelection = range
                ? row >= range.startRow && row <= range.endRow && column >= range.startColumn && column <= range.endColumn
                : false;
              const isActive = active?.row === row && active?.column === column;
              const merge = mergeAnchors.get(key);
              const isEditing = editing?.row === row && editing?.column === column;
              return (
                <div
                  key={key}
                  data-cell={`${row}:${column}`}
                  className={`ss-grid__cell${inSelection ? " is-selected" : ""}${isActive ? " is-active" : ""}`}
                  style={{
                    left: column * COL_WIDTH,
                    top: rowMetrics.top(row),
                    width: COL_WIDTH * (merge ? merge.endColumn - merge.startColumn + 1 : 1),
                    height: rowMetrics.top(merge ? merge.endRow + 1 : row + 1) - rowMetrics.top(row),
                    ...cellStyle(row, column),
                  }}
                  title={content ? `${cellRef(row, column)}  ${content}` : cellRef(row, column)}
                  onPointerDown={(event) => pointerDown(event, row, column)}
                  onDoubleClick={() => onActivate(row, column)}
                  onContextMenu={
                    onCellContextMenu
                      ? (event) => {
                          event.preventDefault();
                          onCellContextMenu(row, column, { x: event.clientX, y: event.clientY });
                        }
                      : undefined
                  }
                >
                  {isEditing
                    ? <EditOverlay
                        cellRef={cellRef(row, column)}
                        onCommit={onEditCommit}
                        onCancel={onEditCancel}
                        initial={editingInitial ?? content}
                      />
                    : <span className={`ss-grid__value${computed?.type === "error" ? " ss-grid__error" : ""}`}>{content}</span>}
                </div>
              );
            })}
          {range && <div className="ss-grid__selection" style={selectionStyle(range, rowMetrics)} />}
        </div>
      </div>
    </div>
  );
}

function parseCellAnchor(anchor: string): CellCoordinate {
  const [row, column] = anchor.split(":").map(Number);
  return { row, column };
}

function columnToLabel(column: number): string {
  let value = column;
  let letters = "";
  while (value >= 0) {
    letters = String.fromCharCode((value % 26) + 65) + letters;
    value = Math.floor(value / 26) - 1;
  }
  return letters;
}

function selectionStyle(range: { startRow: number; startColumn: number; endRow: number; endColumn: number }, rows: ReturnType<typeof spreadsheetRowMetrics>) {
  const left = range.startColumn * COL_WIDTH;
  const top = rows.top(range.startRow);
  const width = (range.endColumn - range.startColumn + 1) * COL_WIDTH;
  const height = rows.top(range.endRow + 1) - top;
  return { left, top, width, height };
}

function EditOverlay({
  cellRef,
  onCommit,
  onCancel,
  initial,
}: {
  cellRef: string;
  onCommit: (text: string) => void;
  onCancel: () => void;
  initial: string;
}) {
  const inputRef = useRef<HTMLInputElement>(null);
  useEffect(() => {
    inputRef.current?.focus();
    inputRef.current?.select();
  }, []);
  return (
    <input
      ref={inputRef}
      className="ss-grid__editor"
      defaultValue={initial}
      data-cell-ref={cellRef}
      onBlur={() => onCommit(inputRef.current?.value ?? "")}
      onKeyDown={(event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          onCommit(inputRef.current?.value ?? "");
        } else if (event.key === "Escape") {
          event.preventDefault();
          onCancel();
        }
      }}
      onPointerDown={(event) => event.stopPropagation()}
    />
  );
}

export function formatGridRange(range: { startRow: number; startColumn: number; endRow: number; endColumn: number }): string {
  return rangeRef(range);
}
