import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, type CSSProperties, type MouseEvent as ReactMouseEvent, type PointerEvent as ReactPointerEvent } from "react";
import type { DocumentBlock, DocumentCommand, RichText, TableCellStyle } from "@open-office/schema/artifact";
import { Icon } from "@open-office/ui";
import type { BlockSessionApi } from "../hooks/useBlockSession.js";
import { CodeBlockView } from "./code/CodeBlockView.js";
import { ImageBlockToolbar } from "./image/ImageBlockToolbar.js";
import { richTextFromHtml, richTextToDom } from "./richText.js";
import type { BlockRendererProps } from "./registry.js";
import { TableContextMenu } from "./table/TableContextMenu.js";
import { TableSelectionLayer } from "./table/TableSelectionLayer.js";
import { TableSelectionToolbar } from "./table/TableSelectionToolbar.js";
import {
  columnIndexForSelection,
  canMergeTableSelection,
  emptyTableGeometry,
  measureTableGeometry,
  mergedRangeForSelection,
  mergeableTableRange,
  normalizeTableSelection,
  mergedCellProjection,
  rowIndexForSelection,
  selectionIncludesCell,
  tableContextSelectionForCell,
  selectionStillExists,
  resizeTableRowHeight,
  shouldPromotePointerToTableRange,
  tableCellTextSelectionFormatState,
  tableSelectionSpansMultipleCells,
  tableSelectionFormatState,
  tableSelectionText,
  tableBoundaryCrossesMerge,
  type TableContextTarget,
  type TableGeometry,
  type TableSelection,
} from "./table/model.js";
import { createTableGridProjection } from "./table/projection.js";
import {
  readTableCellTextSelection,
  restoreTableCellTextSelection,
  type TableCellTextSelection,
} from "../utils/blockSelection.js";

export function ContentBlockRenderer({
  block,
  session,
  contentRef,
  empty,
  align,
  lineHeight,
  placeholder,
  onInput,
  onKeyDown,
}: BlockRendererProps) {
  const isTodo = block.kind.type === "todo";
  const checked = block.data.type === "todo" && block.data.data.checked;
  const composingRef = useRef(false);
  const [pastingImage, setPastingImage] = useState(false);
  return (
    <div className={isTodo ? `block-row__todo${checked ? " is-checked" : ""}` : undefined}>
      {isTodo && (
        <button
          className="block-row__todo-check"
          type="button"
          role="checkbox"
          aria-checked={checked}
          aria-label={checked ? "标记为未完成" : "标记为已完成"}
          onMouseDown={(event) => event.preventDefault()}
          onClick={() => session.setTodoChecked(block.id, !checked)}
        >
          {checked && <Icon name="check" />}
        </button>
      )}
      <div
        ref={contentRef}
        className="block-row__content"
        contentEditable
        suppressContentEditableWarning
        role="textbox"
        tabIndex={0}
        style={{ textAlign: align, lineHeight }}
        data-placeholder={empty ? placeholder : undefined}
        aria-busy={pastingImage || undefined}
        aria-label={placeholder}
        onFocus={() => session.setActiveBlock(block.id)}
        onCompositionStart={() => { composingRef.current = true; }}
        onCompositionEnd={() => {
          composingRef.current = false;
          onInput();
        }}
        onInput={() => {
          // IME emits provisional input events before compositionend. Do not
          // enqueue one semantic transaction per provisional grapheme; commit
          // the final DOM value once the composition has settled.
          if (!composingRef.current) onInput();
        }}
        onPaste={(event) => {
          const itemFile = Array.from(event.clipboardData.items)
            .find((item) => item.kind === "file" && item.type.startsWith("image/"))
            ?.getAsFile();
          const file = itemFile ?? Array.from(event.clipboardData.files).find((candidate) => candidate.type.startsWith("image/"));
          if (!file) return;
          // Native contenteditable image nodes are not part of RichText and
          // disappear on the next projection refresh. Convert the paste into
          // a canonical asset-backed Image Block instead.
          event.preventDefault();
          setPastingImage(true);
          void session.insertPastedImage(block.id, file)
            .then((imageBlockId) => {
              if (imageBlockId) session.setActiveBlock(imageBlockId);
            })
            .finally(() => setPastingImage(false));
        }}
        onKeyDown={onKeyDown}
        onBlur={() => void session.save()}
      />
      {pastingImage && <span className="block-row__paste-status" role="status">正在插入图片…</span>}
    </div>
  );
}

export function DividerBlockRenderer({ block, session }: BlockRendererProps) {
  return <button className="block-divider" type="button" onClick={() => session.setActiveBlock(block.id)} aria-label="分割线" />;
}

export function TableBlockRenderer({ block, session }: BlockRendererProps) {
  return <TableBlockView block={block} session={session} />;
}

function tableCommandSelection(selection: TableSelection): Extract<DocumentCommand, { type: "formatTableCells" }>["selection"] {
  if (selection.kind === "cell") return { kind: "cell", rowId: selection.rowId, cellId: selection.cellId };
  if (selection.kind === "row") return { kind: "row", rowId: selection.id };
  if (selection.kind === "column") return { kind: "column", columnId: selection.id };
  if (selection.kind === "range") return {
    kind: "range",
    startRowId: selection.startRowId,
    endRowId: selection.endRowId,
    startColumnId: selection.startColumnId,
    endColumnId: selection.endColumnId,
  };
  return { kind: "all" };
}

function tableTextAttrsToInlinePatch(
  attrs: Record<string, unknown>,
): Extract<DocumentCommand, { type: "patchTableCellInlineRange" }>["patch"] | null {
  const patch: Extract<DocumentCommand, { type: "patchTableCellInlineRange" }>["patch"] = {};
  for (const [key, value] of Object.entries(attrs)) {
    switch (key) {
      case "bold":
      case "italic":
      case "underline":
      case "strikethrough":
        if (typeof value !== "boolean") return null;
        patch[key] = value;
        break;
      case "color":
      case "highlight":
        if (value !== null && typeof value !== "string") return null;
        patch[key] = value;
        break;
      case "fontSize":
        if (typeof value !== "number" || !Number.isFinite(value)) return null;
        patch.fontSize = value;
        break;
      default:
        return null;
    }
  }
  return Object.keys(patch).length > 0 ? patch : null;
}

export function ImageBlockRenderer({ block, session, selected }: BlockRendererProps) {
  return <ImageBlockView block={block} session={session} selected={selected} />;
}

export function CodeBlockRenderer({ block, session }: BlockRendererProps) {
  return <CodeBlockView block={block} session={session} />;
}

type ColumnResizePreview = {
  leftColumnId: string;
  rightColumnId: string;
  leftWidth: number;
  rightWidth: number;
};

type RowResizePreview = {
  rowId: string;
  height: number;
};

function resizeAdjacentDimensions(
  leading: number,
  trailing: number,
  pointerDelta: number,
  minimum: number,
): { leading: number; trailing: number } {
  const delta = Math.max(minimum - leading, Math.min(trailing - minimum, pointerDelta));
  return { leading: leading + delta, trailing: trailing - delta };
}

export function TableBlockView({ block, session }: { block: DocumentBlock; session: BlockSessionApi }) {
  const tablePayload = block.data.type === "table" ? block.data : null;
  const wrapRef = useRef<HTMLDivElement>(null);
  const [selection, setSelection] = useState<TableSelection | null>(null);
  const [isCellSelecting, setIsCellSelecting] = useState(false);
  const [cellTextSelection, setCellTextSelection] = useState<TableCellTextSelection | null>(null);
  const [contextMenu, setContextMenu] = useState<TableContextTarget | null>(null);
  const [geometry, setGeometry] = useState<TableGeometry>(emptyTableGeometry);
  const [resizePreview, setResizePreview] = useState<ColumnResizePreview | null>(null);
  const [rowResizePreview, setRowResizePreview] = useState<RowResizePreview | null>(null);
  const resizePreviewRef = useRef<ColumnResizePreview | null>(null);
  const resizingRef = useRef<{
    leftColumnId: string;
    rightColumnId: string;
    startX: number;
    startLeftWidth: number;
    startRightWidth: number;
  } | null>(null);
  const rowResizePreviewRef = useRef<RowResizePreview | null>(null);
  const rowResizingRef = useRef<{
    rowId: string;
    startY: number;
    startHeight: number;
  } | null>(null);
  // A pointer selection is committed on mousedown, while contentEditable
  // focus arrives immediately afterwards. Keep the two phases from letting
  // the focus handler overwrite a newly-created range with a single cell.
  const pointerSelectionRef = useRef<TableSelection | null>(null);
  const cellDragRef = useRef<{
    anchorRowId: string;
    anchorColumnId: string;
    pointerId: number;
    startX: number;
    startY: number;
    canStartRange: boolean;
    active: boolean;
  } | null>(null);

  const measureCurrentTable = useCallback(() => {
    const wrap = wrapRef.current;
    const table = wrap?.querySelector<HTMLTableElement>(".block-table");
    if (!wrap || !table || !tablePayload) return;
    const wrapRect = wrap.getBoundingClientRect();
    const tableRect = table.getBoundingClientRect();
    const rows = Array.from(table.tBodies[0]?.rows ?? []);
    // Measure the colgroup, not a body row. A merged anchor changes the
    // number of visible cells in a row, while each <col> remains a stable
    // one-to-one projection of the persisted column IDs.
    const columns = Array.from(table.querySelectorAll<HTMLTableColElement>("col"));
    setGeometry(measureTableGeometry(
      wrapRect,
      tableRect,
      rows.map((row) => row.getBoundingClientRect()),
      columns.map((column) => column.getBoundingClientRect()),
      tablePayload.data.rows.map((row) => row.id),
      tablePayload.data.columns.map((column) => column.id),
    ));
  }, [tablePayload]);

  useLayoutEffect(() => {
    const wrap = wrapRef.current;
    if (!wrap || !tablePayload) return;
    let frame = 0;
    const measure = () => {
      frame = 0;
      measureCurrentTable();
    };
    const scheduleMeasure = () => {
      if (!frame) frame = requestAnimationFrame(measure);
    };
    measure();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(scheduleMeasure);
    observer?.observe(wrap);
    window.addEventListener("resize", scheduleMeasure);
    return () => {
      if (frame) cancelAnimationFrame(frame);
      observer?.disconnect();
      window.removeEventListener("resize", scheduleMeasure);
    };
  }, [measureCurrentTable, tablePayload]);

  // The preview changes the DOM width/height before the semantic command is
  // committed. Re-measure after the browser applies that preview so the
  // hover rule follows the actual boundary rather than its old coordinate.
  useLayoutEffect(() => {
    if (!resizePreview && !rowResizePreview) return;
    const frame = requestAnimationFrame(measureCurrentTable);
    return () => cancelAnimationFrame(frame);
  }, [measureCurrentTable, resizePreview, rowResizePreview]);

  useEffect(() => {
    const finishCellDrag = () => {
      cellDragRef.current = null;
      setIsCellSelecting(false);
    };
    const extendCellDrag = (event: PointerEvent) => {
      const drag = cellDragRef.current;
      const root = wrapRef.current;
      if (!drag || drag.pointerId !== event.pointerId || !root || !tablePayload) return;
      const target = document.elementFromPoint(event.clientX, event.clientY);
      const cell = target instanceof Element ? target.closest<HTMLElement>("[data-table-cell-id]") : null;
      if (!cell || !root.contains(cell)) return;
      const rowId = cell.dataset.tableRowId;
      const columnId = cell.dataset.tableColumnId;
      if (!rowId || !columnId) return;
      const sameCell = rowId === drag.anchorRowId && columnId === drag.anchorColumnId;
      if (!drag.active && !shouldPromotePointerToTableRange({
        canStartRange: drag.canStartRange,
        sameCell,
        startX: drag.startX,
        startY: drag.startY,
        currentX: event.clientX,
        currentY: event.clientY,
      })) return;
      if (!drag.active) {
        drag.active = true;
        setIsCellSelecting(true);
        window.getSelection()?.removeAllRanges();
      }
      event.preventDefault();
      const range: TableSelection = {
        kind: "range",
        startRowId: drag.anchorRowId,
        endRowId: rowId,
        startColumnId: drag.anchorColumnId,
        endColumnId: columnId,
      };
      const normalizedRange = normalizeTableSelection(tablePayload.data, range);
      pointerSelectionRef.current = normalizedRange;
      setSelection(normalizedRange);
      setContextMenu(null);
    };
    window.addEventListener("pointermove", extendCellDrag);
    window.addEventListener("pointerup", finishCellDrag);
    window.addEventListener("pointercancel", finishCellDrag);
    return () => {
      window.removeEventListener("pointermove", extendCellDrag);
      window.removeEventListener("pointerup", finishCellDrag);
      window.removeEventListener("pointercancel", finishCellDrag);
    };
  }, [tablePayload]);

  useEffect(() => {
    if (tablePayload && selection && !selectionStillExists(tablePayload.data, selection)) setSelection(null);
  }, [tablePayload, selection]);

  // A single cell is an editor surface, not a table range. Only promote it to
  // the table toolbar when the native DOM selection contains actual text in
  // that same cell. This keeps a click into an empty cell quiet while still
  // matching office editors when text inside the cell is selected.
  useEffect(() => {
    const updateCellTextSelection = () => {
      const root = wrapRef.current;
      const currentSelection = selection;
      if (!root || !currentSelection || currentSelection.kind !== "cell") {
        setCellTextSelection(null);
        return;
      }
      const nextSelection = readTableCellTextSelection(root);
      setCellTextSelection(nextSelection?.cellId === currentSelection.cellId ? nextSelection : null);
    };

    updateCellTextSelection();
    document.addEventListener("selectionchange", updateCellTextSelection);
    document.addEventListener("keyup", updateCellTextSelection);
    return () => {
      document.removeEventListener("selectionchange", updateCellTextSelection);
      document.removeEventListener("keyup", updateCellTextSelection);
    };
  }, [selection]);

  useEffect(() => {
    if (!contextMenu) return;
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Element)) {
        setContextMenu(null);
        return;
      }
      if (wrapRef.current?.contains(target) || target.closest(".block-table__context-submenu")) return;
      setContextMenu(null);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setContextMenu(null);
    };
    window.addEventListener("pointerdown", onPointerDown);
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("pointerdown", onPointerDown);
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [contextMenu]);

  // TableGridProjection is the read-only runtime index for this render. It
  // keeps stable-id lookups out of cell JSX while retaining the canonical
  // TableBlock reference; commands still go through BlockSessionApi below.
  const grid = useMemo(() => tablePayload ? createTableGridProjection(tablePayload.data) : null, [tablePayload?.data]);
  if (!tablePayload || !grid) return null;

  const openContextMenu = (event: ReactMouseEvent, nextSelection: TableSelection) => {
    event.preventDefault();
    event.stopPropagation();
    const normalizedSelection = normalizeTableSelection(tablePayload.data, nextSelection);
    setSelection(normalizedSelection);
    // Context menus live in the global overlay portal, so their coordinates
    // stay in viewport space rather than the table's stacking context.
    setContextMenu({ selection: normalizedSelection, x: event.clientX, y: event.clientY });
  };

  const selectSelection = (nextSelection: TableSelection) => {
    const normalizedSelection = normalizeTableSelection(tablePayload.data, nextSelection);
    pointerSelectionRef.current = normalizedSelection;
    setSelection(normalizedSelection);
    setContextMenu(null);
  };

  const selectCell = (rowId: string, columnId: string, cellId: string, extend: boolean) => {
    if (!extend || !selection || (selection.kind !== "cell" && selection.kind !== "range")) {
      selectSelection({ kind: "cell", rowId, cellId });
      return;
    }
    let startRowId = rowId;
    let startColumnId = columnId;
    if (selection.kind === "range") {
      startRowId = selection.startRowId;
      startColumnId = selection.startColumnId;
    } else {
      const selectedRow = tablePayload.data.rows.find((row) => row.id === selection.rowId);
      const selectedCellIndex = selectedRow?.cells.findIndex((cell) => cell.id === selection.cellId) ?? -1;
      const selectedColumn = tablePayload.data.columns[selectedCellIndex];
      if (!selectedRow || !selectedColumn) {
        selectSelection({ kind: "cell", rowId, cellId });
        return;
      }
      startRowId = selectedRow.id;
      startColumnId = selectedColumn.id;
    }
    const nextSelection: TableSelection = { kind: "range", startRowId, endRowId: rowId, startColumnId, endColumnId: columnId };
    selectSelection(nextSelection);
  };

  const selectCellFromPointer = (
    event: ReactPointerEvent<HTMLTableCellElement>,
    rowId: string,
    columnId: string,
    cellId: string,
  ) => {
    if (event.button !== 0) return;
    // A normal press always starts as native content editing. A surface drag
    // may later be promoted to a rectangular range after it crosses a cell
    // boundary; a text drag remains a native DOM selection.
    const startedOnCellSurface = event.target === event.currentTarget;
    let anchorRowId = rowId;
    let anchorColumnId = columnId;
    if (event.shiftKey && selection) {
      if (selection.kind === "range") {
        anchorRowId = selection.startRowId;
        anchorColumnId = selection.startColumnId;
      } else if (selection.kind === "cell") {
        anchorRowId = selection.rowId;
        const anchorRow = tablePayload.data.rows.find((row) => row.id === selection.rowId);
        const anchorCellIndex = anchorRow?.cells.findIndex((cell) => cell.id === selection.cellId) ?? -1;
        anchorColumnId = anchorCellIndex >= 0 ? tablePayload.data.columns[anchorCellIndex]?.id ?? columnId : columnId;
      }
    }

    if (!event.shiftKey) {
      cellDragRef.current = {
        anchorRowId: rowId,
        anchorColumnId: columnId,
        pointerId: event.pointerId,
        startX: event.clientX,
        startY: event.clientY,
        canStartRange: startedOnCellSurface,
        active: false,
      };
      setIsCellSelecting(false);
      selectCell(rowId, columnId, cellId, false);
      return;
    }

    event.preventDefault();
    event.stopPropagation();
    cellDragRef.current = {
      anchorRowId,
      anchorColumnId,
      pointerId: event.pointerId,
      startX: event.clientX,
      startY: event.clientY,
      canStartRange: true,
      active: anchorRowId !== rowId || anchorColumnId !== columnId,
    };
    if (anchorRowId !== rowId || anchorColumnId !== columnId) {
      const range: TableSelection = { kind: "range", startRowId: anchorRowId, endRowId: rowId, startColumnId: anchorColumnId, endColumnId: columnId };
      selectSelection(range);
      setIsCellSelecting(true);
    } else {
      selectCell(rowId, columnId, cellId, false);
    }
  };

  const extendCellPointerSelection = (event: ReactPointerEvent<HTMLTableCellElement>, rowId: string, columnId: string) => {
    const drag = cellDragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    const sameCell = rowId === drag.anchorRowId && columnId === drag.anchorColumnId;
    if (!drag.active && !shouldPromotePointerToTableRange({
      canStartRange: drag.canStartRange,
      sameCell,
      startX: drag.startX,
      startY: drag.startY,
      currentX: event.clientX,
      currentY: event.clientY,
    })) return;
    if (!drag.active) {
      drag.active = true;
      setIsCellSelecting(true);
      window.getSelection()?.removeAllRanges();
    }
    event.preventDefault();
    const range: TableSelection = {
      kind: "range",
      startRowId: drag.anchorRowId,
      endRowId: rowId,
      startColumnId: drag.anchorColumnId,
      endColumnId: columnId,
    };
    selectSelection(range);
  };

  const insertAtSelection = (direction: "before" | "after") => {
    if (!selection || (selection.kind !== "row" && selection.kind !== "column")) return;
    const index = selection.kind === "row"
      ? rowIndexForSelection(tablePayload.data, selection)
      : columnIndexForSelection(tablePayload.data, selection);
    if (index === null) return;
    const boundaryIndex = index + (direction === "after" ? 1 : 0);
    if (selection.kind === "row") session.insertTableRow(block.id, boundaryIndex);
    else session.insertTableColumn(block.id, boundaryIndex);
    setContextMenu(null);
  };

  const deleteSelection = () => {
    if (!selection || (selection.kind !== "row" && selection.kind !== "column")) return;
    const index = selection.kind === "row"
      ? rowIndexForSelection(tablePayload.data, selection)
      : columnIndexForSelection(tablePayload.data, selection);
    if (index === null) return;
    if (selection.kind === "row") session.deleteTableRow(block.id, index);
    else session.deleteTableColumn(block.id, index);
    setSelection(null);
    setContextMenu(null);
  };

  const closeContextOnContentPointerDown = (event: ReactMouseEvent<HTMLDivElement>) => {
    const target = event.target as Element;
    if (target.closest(".block-table__context-menu") || target.closest(".block-table__context-submenu") || target.closest(".block-table__row-selector") || target.closest(".block-table__column-selector") || target.closest(".block-table__corner-selector") || target.closest(".block-table__selection-toolbar")) return;
    setContextMenu(null);
  };

  const copySelection = async () => {
    if (!selection) return;
    try {
      await navigator.clipboard?.writeText(tableSelectionText(tablePayload.data, selection));
    } catch {
      // Clipboard permissions are optional; the selection remains intact.
    }
    setContextMenu(null);
  };

  const cutSelection = () => {
    void copySelection();
    deleteSelection();
  };

  const splitRange = mergedRangeForSelection(tablePayload.data, selection);
  const canMerge = canMergeTableSelection(tablePayload.data, selection);
  const canSplit = splitRange !== null;
  const formatState = tableCellTextSelectionFormatState(tablePayload.data, cellTextSelection)
    ?? tableSelectionFormatState(tablePayload.data, selection);

  const mergeSelection = () => {
    const range = mergeableTableRange(tablePayload.data, selection);
    if (!canMerge || !range) return;
    session.mergeTableCells(block.id, range);
    // Keep the newly-created persisted range selected so the next toolbar
    // action is immediately Split, matching office table semantics.
    pointerSelectionRef.current = { kind: "range", ...range };
    setSelection({ kind: "range", ...range });
    setContextMenu(null);
  };

  const splitSelection = () => {
    if (!splitRange) return;
    session.splitTableCells(block.id, splitRange);
    setContextMenu(null);
  };

  const handleCellKeyDown = (event: React.KeyboardEvent<HTMLTableCellElement>, rowId: string, cellId: string) => {
    if (!event.shiftKey || !["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(event.key)) return;
    const rowIndex = tablePayload.data.rows.findIndex((row) => row.id === rowId);
    const row = tablePayload.data.rows[rowIndex];
    const cellIndex = row?.cells.findIndex((cell) => cell.id === cellId) ?? -1;
    if (rowIndex < 0 || cellIndex < 0) return;
    const columnCount = tablePayload.data.columns.length;
    const columnIndex = cellIndex;
    const rowDelta = event.key === "ArrowUp" ? -1 : event.key === "ArrowDown" ? 1 : 0;
    const columnDelta = event.key === "ArrowLeft" ? -1 : event.key === "ArrowRight" ? 1 : 0;
    const nextRowIndex = Math.max(0, Math.min(tablePayload.data.rows.length - 1, rowIndex + rowDelta));
    const nextColumnIndex = Math.max(0, Math.min(columnCount - 1, columnIndex + columnDelta));
    if (nextRowIndex === rowIndex && nextColumnIndex === columnIndex) return;
    const anchor = selection?.kind === "range"
      ? selection
      : { kind: "range" as const, startRowId: rowId, endRowId: rowId, startColumnId: tablePayload.data.columns[columnIndex].id, endColumnId: tablePayload.data.columns[columnIndex].id };
    const nextRowId = tablePayload.data.rows[nextRowIndex].id;
    const nextColumnId = tablePayload.data.columns[nextColumnIndex].id;
    selectSelection({
      kind: "range",
      startRowId: anchor.startRowId,
      endRowId: nextRowId,
      startColumnId: anchor.startColumnId,
      endColumnId: nextColumnId,
    });
    event.preventDefault();
    const nextCell = tablePayload.data.rows[nextRowIndex].cells[nextColumnIndex];
    const nextElement = Array.from(wrapRef.current?.querySelectorAll<HTMLTableCellElement>("[data-table-cell-id]") ?? [])
      .find((element) => element.dataset.tableCellId === nextCell.id);
    nextElement?.focus({ preventScroll: true });
  };

  const applyTableBorderPreset = (preset: Parameters<BlockSessionApi["applyTableBorderPreset"]>[2], border?: Parameters<BlockSessionApi["applyTableBorderPreset"]>[3]) => {
    if (!selection) return;
    session.applyTableBorderPreset(block.id, tableCommandSelection(selection), preset, border);
    setContextMenu(null);
  };

  const beginColumnResize = (
    event: React.PointerEvent<HTMLButtonElement>,
    leftColumnId: string,
    rightColumnId: string,
    leftWidth: number,
    rightWidth: number,
  ) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    const startLeftWidth = Math.max(32, leftWidth);
    const startRightWidth = Math.max(32, rightWidth);
    resizingRef.current = { leftColumnId, rightColumnId, startX: event.clientX, startLeftWidth, startRightWidth };
    resizePreviewRef.current = { leftColumnId, rightColumnId, leftWidth: startLeftWidth, rightWidth: startRightWidth };
    setResizePreview(resizePreviewRef.current);
    const onMove = (moveEvent: PointerEvent) => {
      const current = resizingRef.current;
      if (!current) return;
      const next = resizeAdjacentDimensions(
        current.startLeftWidth,
        current.startRightWidth,
        moveEvent.clientX - current.startX,
        32,
      );
      resizePreviewRef.current = {
        leftColumnId: current.leftColumnId,
        rightColumnId: current.rightColumnId,
        leftWidth: next.leading,
        rightWidth: next.trailing,
      };
      setResizePreview(resizePreviewRef.current);
    };
    const onUp = () => {
      const current = resizingRef.current;
      resizingRef.current = null;
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      if (current) {
        const preview = resizePreviewRef.current;
        if (preview?.leftColumnId === current.leftColumnId && preview.rightColumnId === current.rightColumnId) {
          session.setTableColumnWidths(block.id, current.leftColumnId, preview.leftWidth, current.rightColumnId, preview.rightWidth);
        }
      }
      resizePreviewRef.current = null;
      setResizePreview(null);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp, { once: true });
    window.addEventListener("pointercancel", onUp, { once: true });
  };

  const beginRowResize = (
    event: React.PointerEvent<HTMLButtonElement>,
    rowId: string,
    rowHeight: number,
  ) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    const startHeight = Math.max(34, rowHeight);
    rowResizingRef.current = { rowId, startY: event.clientY, startHeight };
    rowResizePreviewRef.current = { rowId, height: startHeight };
    setRowResizePreview(rowResizePreviewRef.current);
    const onMove = (moveEvent: PointerEvent) => {
      const current = rowResizingRef.current;
      if (!current) return;
      const height = resizeTableRowHeight(current.startHeight, moveEvent.clientY - current.startY);
      rowResizePreviewRef.current = {
        rowId: current.rowId,
        height,
      };
      setRowResizePreview(rowResizePreviewRef.current);
    };
    const onUp = () => {
      const current = rowResizingRef.current;
      rowResizingRef.current = null;
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      window.removeEventListener("pointercancel", onUp);
      if (current) {
        const preview = rowResizePreviewRef.current;
        if (preview?.rowId === current.rowId) {
          session.setTableRowHeight(block.id, current.rowId, preview.height);
        }
      }
      rowResizePreviewRef.current = null;
      setRowResizePreview(null);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp, { once: true });
    window.addEventListener("pointercancel", onUp, { once: true });
  };

  return (
    <div
      ref={wrapRef}
      className={`block-table-wrap${selection ? " is-selection-active" : ""}${isCellSelecting ? " is-cell-selecting" : ""}`}
      onPointerDown={closeContextOnContentPointerDown}
    >
      <table className="block-table" aria-label="表格块" data-table-grid="document">
        <colgroup>
          {grid.columns.map((column) => {
            const previewWidth = resizePreview?.leftColumnId === column.id
              ? resizePreview.leftWidth
              : resizePreview?.rightColumnId === column.id
                ? resizePreview.rightWidth
                : null;
            return <col key={column.id} style={previewWidth !== null
              ? { width: `${previewWidth}px` }
              : column.width ? { width: `${column.width}px` } : undefined} />;
          })}
        </colgroup>
        <tbody>
          {grid.rows.map((row) => (
            <tr key={row.id} style={rowResizePreview?.rowId === row.id
              ? { height: `${rowResizePreview.height}px` }
              : row.height ? { height: `${row.height}px` } : undefined}>
              {grid.cellsInRow(row.id).map((cell) => {
                const columnIndex = row.cells.indexOf(cell);
                const column = columnIndex >= 0 ? grid.columns[columnIndex] : undefined;
                if (columnIndex < 0 || !column) return null;
                if (!grid.cellAt(row.id, column.id)) return null;
                const projection = column ? mergedCellProjection(tablePayload.data, row.id, column.id) : undefined;
                if (projection === null) return null;
                return <TableCellView
                  key={cell.id}
                  blockId={block.id}
                  rowId={row.id}
                  columnId={column?.id ?? ""}
                  cellId={cell.id}
                  content={cell.content}
                  session={session}
                  selected={selection?.kind !== "cell" && selectionIncludesCell(tablePayload.data, selection, row.id, tablePayload.data.columns[columnIndex]?.id ?? "", cell.id)}
                  style={cell.style}
                  onFocus={() => {
                    session.setActiveBlock(block.id);
                    const pointerSelection = pointerSelectionRef.current;
                    pointerSelectionRef.current = null;
                    const pointerCommittedThisCell = pointerSelection?.kind === "cell"
                      && pointerSelection.rowId === row.id
                      && pointerSelection.cellId === cell.id;
                    const pointerCommittedRange = pointerSelection?.kind === "range";
                    if (!pointerCommittedThisCell && !pointerCommittedRange && !selection) {
                      setSelection({ kind: "cell", rowId: row.id, cellId: cell.id });
                    }
                    setContextMenu(null);
                  }}
                  onPointerDown={(event) => selectCellFromPointer(event, row.id, tablePayload.data.columns[columnIndex]?.id ?? "", cell.id)}
                  onPointerEnter={(event) => extendCellPointerSelection(event, row.id, tablePayload.data.columns[columnIndex]?.id ?? "")}
                  onKeyDown={(event) => handleCellKeyDown(event, row.id, cell.id)}
                  onContextMenu={(event) => openContextMenu(
                    event,
                    tableContextSelectionForCell(
                      tablePayload.data,
                      selection,
                      row.id,
                      tablePayload.data.columns[columnIndex]?.id ?? "",
                      cell.id,
                    ),
                  )}
                  rowSpan={projection?.rowSpan}
                  colSpan={projection?.colSpan}
                />;
              })}
            </tr>
          ))}
        </tbody>
      </table>
      <div className="block-table__controls" aria-label="表格操作">
        {geometry.columnBoundaries.map((left, boundaryIndex) => {
          // Only internal boundaries are resizable. Each boundary owns the
          // two adjacent columns, so the grid total stays fixed and no other
          // column is redistributed by the browser.
          if (boundaryIndex <= 0 || boundaryIndex >= geometry.columns.length) return null;
          if (tableBoundaryCrossesMerge(tablePayload.data, "column", boundaryIndex)) return null;
          const leftColumn = geometry.columns[boundaryIndex - 1];
          const rightColumn = geometry.columns[boundaryIndex];
          const tableHeight = (geometry.rowBoundaries.at(-1) ?? geometry.tableTop) - geometry.tableTop;
          return (
            <button
              key={`resize-column-${leftColumn.id}-${rightColumn.id}`}
              className={`block-table__resize-handle${resizePreview?.leftColumnId === leftColumn.id && resizePreview.rightColumnId === rightColumn.id ? " is-resizing" : ""}`}
              style={{ left: `${left}px`, top: `${geometry.tableTop}px`, height: `${Math.max(0, tableHeight)}px` }}
              type="button"
              data-table-resize="column"
              data-table-column-id={leftColumn.id}
              data-table-column-next-id={rightColumn.id}
              aria-label={`调整第 ${boundaryIndex} 与第 ${boundaryIndex + 1} 列宽度`}
              title="拖动调整列宽"
              onPointerDown={(event) => beginColumnResize(event, leftColumn.id, rightColumn.id, leftColumn.width, rightColumn.width)}
            />
          );
        })}
        {geometry.rowBoundaries.map((top, boundaryIndex) => {
          // Only internal boundaries are resizable. Each boundary owns the
          // two adjacent rows, preserving the table's total height.
          if (boundaryIndex <= 0 || boundaryIndex >= geometry.rows.length) return null;
          if (tableBoundaryCrossesMerge(tablePayload.data, "row", boundaryIndex)) return null;
          const topRow = geometry.rows[boundaryIndex - 1];
          const bottomRow = geometry.rows[boundaryIndex];
          const tableWidth = geometry.tableWidth;
          return (
            <button
              key={`resize-row-${topRow.id}-${bottomRow.id}`}
              className={`block-table__row-resize-handle${rowResizePreview?.rowId === topRow.id ? " is-resizing" : ""}`}
              style={{ left: `${geometry.tableLeft}px`, top: `${top}px`, width: `${Math.max(0, tableWidth)}px` }}
              type="button"
              data-table-resize="row"
              data-table-row-id={topRow.id}
              data-table-row-next-id={bottomRow.id}
              aria-label={`调整第 ${boundaryIndex} 与第 ${boundaryIndex + 1} 行高度`}
              title="拖动调整行高"
              onPointerDown={(event) => beginRowResize(event, topRow.id, topRow.height)}
            />
          );
        })}
        {geometry.rowBoundaries.map((top, boundaryIndex) => (
          boundaryIndex === 0 ? null : (
          <button
            key={`row-${boundaryIndex}`}
            className="block-table__affordance block-table__row-affordance"
            style={{ left: `${geometry.tableLeft - 9}px`, top: `${top - 9}px` }}
            type="button"
            aria-label={boundaryIndex === geometry.rowBoundaries.length - 1 ? "在表格末尾添加行" : `在第 ${boundaryIndex + 1} 行前添加行`}
            title={boundaryIndex === geometry.rowBoundaries.length - 1 ? "添加行" : "在此处添加行"}
            onClick={() => session.insertTableRow(block.id, boundaryIndex)}
          >
            <Icon name="plus" />
            <span className="sr-only">添加行</span>
          </button>
          )
        ))}
        {geometry.columnBoundaries.map((left, boundaryIndex) => (
          boundaryIndex === 0 ? null : (
          <button
            key={`column-${boundaryIndex}`}
            className="block-table__affordance block-table__column-affordance"
            style={{ left: `${left - 9}px`, top: `${geometry.tableTop - 9}px` }}
            type="button"
            aria-label={boundaryIndex === geometry.columnBoundaries.length - 1 ? "在表格末尾添加列" : `在第 ${boundaryIndex + 1} 列前添加列`}
            title={boundaryIndex === geometry.columnBoundaries.length - 1 ? "添加列" : "在此处添加列"}
            onClick={() => session.insertTableColumn(block.id, boundaryIndex)}
          >
            <Icon name="plus" />
            <span className="sr-only">添加列</span>
          </button>
          )
        ))}
      </div>
      <TableSelectionLayer
        table={tablePayload.data}
        geometry={geometry}
        selection={selection}
        onSelect={selectSelection}
        onContextMenu={openContextMenu}
      />
      {selection && (tableSelectionSpansMultipleCells(tablePayload.data, selection) || cellTextSelection !== null) && (
        <TableSelectionToolbar
          selection={selection}
          geometry={geometry}
          onInsert={insertAtSelection}
          onMerge={mergeSelection}
          onSplit={splitSelection}
          onApplyBorderPreset={applyTableBorderPreset}
          canMerge={canMerge}
          canSplit={canSplit}
          formatState={formatState}
          onFormat={(patch) => {
            const inlinePatch = cellTextSelection && patch.textAttrs
              ? tableTextAttrsToInlinePatch(patch.textAttrs)
              : null;
            if (cellTextSelection && inlinePatch) {
              session.patchTableCellInlineRange(
                block.id,
                cellTextSelection.rowId,
                cellTextSelection.cellId,
                { start: cellTextSelection.start, end: cellTextSelection.end },
                inlinePatch,
              );
              requestAnimationFrame(() => {
                if (wrapRef.current) restoreTableCellTextSelection(wrapRef.current, cellTextSelection);
              });
            } else {
              session.formatTableCells(block.id, tableCommandSelection(selection), patch);
            }
            setContextMenu(null);
          }}
        />
      )}
      {contextMenu && (
        <TableContextMenu
          target={contextMenu}
          onCut={cutSelection}
          onCopy={() => void copySelection()}
          onInsert={insertAtSelection}
          onDelete={deleteSelection}
          onMerge={mergeSelection}
          onSplit={splitSelection}
          onApplyBorderPreset={applyTableBorderPreset}
          canMerge={canMerge}
          canSplit={canSplit}
        />
      )}
    </div>
  );
}

export function TableCellView({
  blockId,
  rowId,
  columnId,
  rowSpan,
  colSpan,
  cellId,
  content,
  style,
  session,
  selected = false,
  onFocus,
  onPointerDown,
  onPointerEnter,
  onContextMenu,
  onKeyDown,
}: {
  blockId: string;
  rowId: string;
  columnId: string;
  rowSpan?: number;
  colSpan?: number;
  cellId: string;
  content: RichText;
  style?: TableCellStyle;
  session: BlockSessionApi;
  selected?: boolean;
  onFocus?: () => void;
  onPointerDown?: (event: ReactPointerEvent<HTMLTableCellElement>) => void;
  onPointerEnter?: (event: ReactPointerEvent<HTMLTableCellElement>) => void;
  onContextMenu?: (event: ReactMouseEvent<HTMLTableCellElement>) => void;
  onKeyDown?: (event: React.KeyboardEvent<HTMLTableCellElement>) => void;
}) {
  const ref = useRef<HTMLTableCellElement>(null);
  useEffect(() => {
    const element = ref.current;
    if (!element || document.activeElement === element) return;
    element.replaceChildren(richTextToDom(content));
  }, [content]);
  const borderStyle = (edge: "top" | "right" | "bottom" | "left"): string | undefined => {
    const border = style?.borders?.[edge];
    return border ? `${border.width}px ${border.style} ${border.color}` : undefined;
  };
  const diagonal = (edge: "diagonalDown" | "diagonalUp") => {
    const border = style?.borders?.[edge];
    if (!border) return undefined;
    const direction = edge === "diagonalDown" ? "to bottom right" : "to top right";
    const half = Math.max(0.5, border.width / 2);
    return `linear-gradient(${direction}, transparent calc(50% - ${half}px), ${border.color} 50%, transparent calc(50% + ${half}px))`;
  };
  return (
    <td
      ref={ref}
      className={`block-table__cell${selected ? " block-table__cell--selected" : ""}`}
      contentEditable
      suppressContentEditableWarning
      data-table-cell-id={cellId}
      data-table-column-id={columnId}
      rowSpan={rowSpan}
      colSpan={colSpan}
      data-table-row-id={rowId}
      style={{
        backgroundColor: style?.fillColor,
        textAlign: style?.horizontalAlign,
        verticalAlign: style?.verticalAlign,
        borderTop: borderStyle("top"),
        borderRight: borderStyle("right"),
        borderBottom: borderStyle("bottom"),
        borderLeft: borderStyle("left"),
        "--oo-table-diagonal-down": diagonal("diagonalDown") ?? "none",
        "--oo-table-diagonal-up": diagonal("diagonalUp") ?? "none",
      } as CSSProperties}
      onPointerDown={(event) => onPointerDown?.(event)}
      onPointerEnter={(event) => onPointerEnter?.(event)}
      onFocus={() => { session.setActiveBlock(blockId); onFocus?.(); }}
      onContextMenu={onContextMenu}
      onInput={() => {
        if (ref.current) session.updateTableCell(blockId, rowId, cellId, richTextFromHtml(ref.current));
      }}
      onKeyDown={(event) => {
        onKeyDown?.(event);
        if (event.defaultPrevented) return;
        if (event.key !== "Enter") return;
        event.preventDefault();
        const selection = window.getSelection();
        const range = selection?.rangeCount ? selection.getRangeAt(0) : null;
        if (!range || !ref.current?.contains(range.commonAncestorContainer)) return;
        range.deleteContents();
        const newline = document.createTextNode("\n");
        range.insertNode(newline);
        range.setStartAfter(newline);
        range.collapse(true);
        selection?.removeAllRanges();
        selection?.addRange(range);
        session.updateTableCell(blockId, rowId, cellId, richTextFromHtml(ref.current));
      }}
      onBlur={() => void session.save()}
    />
  );
}

export function ImageBlockView({ block, session, selected }: { block: DocumentBlock; session: BlockSessionApi; selected: boolean }) {
  const [failed, setFailed] = useState(false);
  const rootRef = useRef<HTMLElement>(null);
  const imageData = block.data.type === "image" ? block.data.data : null;
  const alt = imageData?.alt || "图片块";
  const assetUrl = imageData ? session.assetUrl(imageData.assetId) : "";
  const imageTransform = imageData?.transform;
  const crop = imageTransform?.crop;
  const imageStyle: CSSProperties | undefined = crop && imageTransform ? {
    clipPath: `inset(${crop.top * 100}% ${crop.right * 100}% ${crop.bottom * 100}% ${crop.left * 100}%)`,
    transform: `scaleX(${imageTransform.flipHorizontal ? -1 : 1}) scaleY(${imageTransform.flipVertical ? -1 : 1})`,
  } : undefined;

  useEffect(() => {
    if (!selected || !imageData) return;
    const dismissOnOutsidePointer = (event: PointerEvent) => {
      const target = event.target;
      if (target instanceof Node && rootRef.current?.contains(target)) return;
      session.setActiveBlock(null);
    };
    window.addEventListener("pointerdown", dismissOnOutsidePointer, true);
    return () => window.removeEventListener("pointerdown", dismissOnOutsidePointer, true);
  }, [imageData, selected, session]);

  if (!imageData) return null;

  return (
    <figure
      ref={rootRef}
      className={`block-image${selected ? " is-selected" : ""}${failed ? " is-load-failed" : ""}`}
      aria-label={alt}
      aria-selected={selected}
      role="group"
      tabIndex={0}
      onPointerDown={() => session.setActiveBlock(block.id)}
      onFocus={() => session.setActiveBlock(block.id)}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          session.setActiveBlock(block.id);
        }
      }}
    >
      {selected && <ImageBlockToolbar blockId={block.id} image={imageData} assetUrl={assetUrl} session={session} />}
      <div className="block-image__frame">
        {failed ? (
          <div className="block-image__placeholder">图片加载失败</div>
        ) : (
          <img
            className="block-image__content"
            src={assetUrl}
            alt={alt}
            draggable={false}
            style={imageStyle}
            onError={() => setFailed(true)}
          />
        )}
      </div>
      {imageData.caption && <figcaption className="block-image__caption">{imageData.caption}</figcaption>}
    </figure>
  );
}
