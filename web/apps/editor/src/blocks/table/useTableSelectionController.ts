import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type PointerEvent,
  type RefObject,
} from "react";

import type { TableBlock } from "@open-office/schema/artifact";

import { readTableCellTextSelection, type TableCellTextSelection } from "../../utils/blockSelection.js";
import {
  normalizeTableSelection,
  selectionStillExists,
  shouldPromotePointerToTableRange,
  type TableSelection,
} from "./model.js";

interface CellAddress {
  rowId: string;
  columnId: string;
  cellId: string;
}

interface CellDrag {
  anchorRowId: string;
  anchorColumnId: string;
  pointerId: number;
  startX: number;
  startY: number;
  canStartRange: boolean;
  active: boolean;
}

/**
 * Owns the ephemeral, stable-id selection state of one table. The document
 * model never receives browser coordinates: pointer and keyboard gestures are
 * reduced here to row/column ids before the view publishes them to the shared
 * interaction store.
 */
export function useTableSelectionController({
  table,
  rootRef,
  onSelectionChange,
}: {
  table: TableBlock | null;
  rootRef: RefObject<HTMLDivElement | null>;
  onSelectionChange?: (selection: TableSelection | null) => void;
}) {
  const [selection, setSelection] = useState<TableSelection | null>(null);
  const [cellTextSelection, setCellTextSelection] = useState<TableCellTextSelection | null>(null);
  const [isCellSelecting, setIsCellSelecting] = useState(false);
  const selectionRef = useRef<TableSelection | null>(null);
  const pointerSelectionRef = useRef<TableSelection | null>(null);
  const cellDragRef = useRef<CellDrag | null>(null);

  const commitSelection = useCallback((next: TableSelection | null): TableSelection | null => {
    const normalized = next && table ? normalizeTableSelection(table, next) : next;
    selectionRef.current = normalized;
    setSelection(normalized);
    return normalized;
  }, [table]);

  const selectSelection = useCallback((next: TableSelection) => {
    const normalized = commitSelection(next);
    pointerSelectionRef.current = normalized;
    return normalized;
  }, [commitSelection]);

  const selectCell = useCallback((address: CellAddress, extend: boolean) => {
    const current = selectionRef.current;
    if (!table || !extend || !current || (current.kind !== "cell" && current.kind !== "range")) {
      return selectSelection({ kind: "cell", rowId: address.rowId, cellId: address.cellId });
    }
    let startRowId = address.rowId;
    let startColumnId = address.columnId;
    if (current.kind === "range") {
      startRowId = current.startRowId;
      startColumnId = current.startColumnId;
    } else {
      const selectedRow = table.rows.find((row) => row.id === current.rowId);
      const selectedCellIndex = selectedRow?.cells.findIndex((cell) => cell.id === current.cellId) ?? -1;
      const selectedColumn = table.columns[selectedCellIndex];
      if (!selectedRow || !selectedColumn) return selectSelection({ kind: "cell", rowId: address.rowId, cellId: address.cellId });
      startRowId = selectedRow.id;
      startColumnId = selectedColumn.id;
    }
    return selectSelection({
      kind: "range",
      startRowId,
      endRowId: address.rowId,
      startColumnId,
      endColumnId: address.columnId,
    });
  }, [selectSelection, table]);

  const finishCellDrag = useCallback(() => {
    cellDragRef.current = null;
    setIsCellSelecting(false);
  }, []);

  useEffect(() => {
    const extendCellDrag = (event: globalThis.PointerEvent) => {
      const drag = cellDragRef.current;
      const root = rootRef.current;
      if (!drag || drag.pointerId !== event.pointerId || !root || !table) return;
      const target = document.elementFromPoint(event.clientX, event.clientY);
      const cell = target instanceof Element ? target.closest<HTMLElement>("[data-table-cell-id]") : null;
      if (!cell || !root.contains(cell)) return;
      const rowId = cell.dataset.tableRowId;
      const columnId = cell.dataset.tableColumnId;
      if (!rowId || !columnId) return;
      const sameCell = rowId === drag.anchorRowId && columnId === drag.anchorColumnId;
      if (!drag.active && !shouldPromotePointerToTableRange({
        canStartRange: drag.canStartRange, sameCell, startX: drag.startX, startY: drag.startY, currentX: event.clientX, currentY: event.clientY,
      })) return;
      if (!drag.active) {
        drag.active = true;
        setIsCellSelecting(true);
        window.getSelection()?.removeAllRanges();
      }
      event.preventDefault();
      selectSelection({
        kind: "range",
        startRowId: drag.anchorRowId,
        endRowId: rowId,
        startColumnId: drag.anchorColumnId,
        endColumnId: columnId,
      });
    };
    window.addEventListener("pointermove", extendCellDrag);
    window.addEventListener("pointerup", finishCellDrag);
    window.addEventListener("pointercancel", finishCellDrag);
    return () => {
      window.removeEventListener("pointermove", extendCellDrag);
      window.removeEventListener("pointerup", finishCellDrag);
      window.removeEventListener("pointercancel", finishCellDrag);
    };
  }, [finishCellDrag, rootRef, selectSelection, table]);

  const onCellPointerDown = useCallback((event: PointerEvent<HTMLTableCellElement>, address: CellAddress) => {
    if (event.button !== 0 || !table) return;
    const startedOnCellSurface = event.target === event.currentTarget;
    let anchorRowId = address.rowId;
    let anchorColumnId = address.columnId;
    const current = selectionRef.current;
    if (event.shiftKey && current) {
      if (current.kind === "range") {
        anchorRowId = current.startRowId;
        anchorColumnId = current.startColumnId;
      } else if (current.kind === "cell") {
        anchorRowId = current.rowId;
        const anchorRow = table.rows.find((row) => row.id === current.rowId);
        const anchorCellIndex = anchorRow?.cells.findIndex((cell) => cell.id === current.cellId) ?? -1;
        anchorColumnId = anchorCellIndex >= 0 ? table.columns[anchorCellIndex]?.id ?? address.columnId : address.columnId;
      }
    }
    if (!event.shiftKey) {
      cellDragRef.current = {
        anchorRowId: address.rowId, anchorColumnId: address.columnId, pointerId: event.pointerId,
        startX: event.clientX, startY: event.clientY, canStartRange: startedOnCellSurface, active: false,
      };
      setIsCellSelecting(false);
      selectCell(address, false);
      return;
    }
    event.preventDefault();
    event.stopPropagation();
    const isRange = anchorRowId !== address.rowId || anchorColumnId !== address.columnId;
    cellDragRef.current = {
      anchorRowId, anchorColumnId, pointerId: event.pointerId, startX: event.clientX, startY: event.clientY,
      canStartRange: true, active: isRange,
    };
    if (isRange) {
      selectCell(address, true);
      setIsCellSelecting(true);
    } else {
      selectCell(address, false);
    }
  }, [selectCell, table]);

  const onCellPointerEnter = useCallback((event: PointerEvent<HTMLTableCellElement>, address: CellAddress) => {
    const drag = cellDragRef.current;
    if (!drag || drag.pointerId !== event.pointerId) return;
    const sameCell = address.rowId === drag.anchorRowId && address.columnId === drag.anchorColumnId;
    if (!drag.active && !shouldPromotePointerToTableRange({
      canStartRange: drag.canStartRange, sameCell, startX: drag.startX, startY: drag.startY, currentX: event.clientX, currentY: event.clientY,
    })) return;
    if (!drag.active) {
      drag.active = true;
      setIsCellSelecting(true);
      window.getSelection()?.removeAllRanges();
    }
    event.preventDefault();
    selectSelection({
      kind: "range",
      startRowId: drag.anchorRowId,
      endRowId: address.rowId,
      startColumnId: drag.anchorColumnId,
      endColumnId: address.columnId,
    });
  }, [selectSelection]);

  const onCellFocus = useCallback((address: CellAddress) => {
    const pointerSelection = pointerSelectionRef.current;
    pointerSelectionRef.current = null;
    const pointerCommittedThisCell = pointerSelection?.kind === "cell"
      && pointerSelection.rowId === address.rowId && pointerSelection.cellId === address.cellId;
    if (!pointerCommittedThisCell && pointerSelection?.kind !== "range" && !selectionRef.current) {
      commitSelection({ kind: "cell", rowId: address.rowId, cellId: address.cellId });
    }
  }, [commitSelection]);

  const onCellKeyDown = useCallback((event: KeyboardEvent<HTMLTableCellElement>, address: CellAddress) => {
    if (!table) return;
    if (event.key === "Tab") {
      const cells = Array.from(rootRef.current?.querySelectorAll<HTMLTableCellElement>("[data-table-cell-id]") ?? []);
      const index = cells.indexOf(event.currentTarget);
      const next = cells[index + (event.shiftKey ? -1 : 1)];
      if (!next) return;
      const rowId = next.dataset.tableRowId;
      const cellId = next.dataset.tableCellId;
      if (!rowId || !cellId) return;
      event.preventDefault();
      commitSelection({ kind: "cell", rowId, cellId });
      next.focus({ preventScroll: true });
      return;
    }
    if (!event.shiftKey || !["ArrowUp", "ArrowDown", "ArrowLeft", "ArrowRight"].includes(event.key)) return;
    const rowIndex = table.rows.findIndex((row) => row.id === address.rowId);
    const row = table.rows[rowIndex];
    const cellIndex = row?.cells.findIndex((cell) => cell.id === address.cellId) ?? -1;
    if (rowIndex < 0 || cellIndex < 0) return;
    const rowDelta = event.key === "ArrowUp" ? -1 : event.key === "ArrowDown" ? 1 : 0;
    const columnDelta = event.key === "ArrowLeft" ? -1 : event.key === "ArrowRight" ? 1 : 0;
    const nextRowIndex = Math.max(0, Math.min(table.rows.length - 1, rowIndex + rowDelta));
    const nextColumnIndex = Math.max(0, Math.min(table.columns.length - 1, cellIndex + columnDelta));
    if (nextRowIndex === rowIndex && nextColumnIndex === cellIndex) return;
    const current = selectionRef.current;
    const anchor = current?.kind === "range"
      ? current
      : { startRowId: address.rowId, startColumnId: table.columns[cellIndex].id };
    const nextRow = table.rows[nextRowIndex];
    const nextColumn = table.columns[nextColumnIndex];
    selectSelection({
      kind: "range",
      startRowId: anchor.startRowId,
      endRowId: nextRow.id,
      startColumnId: anchor.startColumnId,
      endColumnId: nextColumn.id,
    });
    event.preventDefault();
    const nextCell = nextRow.cells[nextColumnIndex];
    const nextElement = Array.from(rootRef.current?.querySelectorAll<HTMLTableCellElement>("[data-table-cell-id]") ?? [])
      .find((element) => element.dataset.tableCellId === nextCell.id);
    nextElement?.focus({ preventScroll: true });
  }, [commitSelection, rootRef, selectSelection, table]);

  useEffect(() => {
    if (table && selection && !selectionStillExists(table, selection)) commitSelection(null);
  }, [commitSelection, selection, table]);

  useEffect(() => {
    onSelectionChange?.(selection);
  }, [onSelectionChange, selection]);

  useEffect(() => {
    const updateCellTextSelection = () => {
      const root = rootRef.current;
      if (!root || !selection || selection.kind !== "cell") {
        setCellTextSelection(null);
        return;
      }
      const next = readTableCellTextSelection(root);
      setCellTextSelection(next?.cellId === selection.cellId ? next : null);
    };
    updateCellTextSelection();
    document.addEventListener("selectionchange", updateCellTextSelection);
    document.addEventListener("keyup", updateCellTextSelection);
    return () => {
      document.removeEventListener("selectionchange", updateCellTextSelection);
      document.removeEventListener("keyup", updateCellTextSelection);
    };
  }, [rootRef, selection]);

  return {
    selection,
    cellTextSelection,
    isCellSelecting,
    commitSelection,
    selectSelection,
    onCellFocus,
    onCellPointerDown,
    onCellPointerEnter,
    onCellKeyDown,
  };
}
