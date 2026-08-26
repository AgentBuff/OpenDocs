import { useRef, useState } from "react";

import type { BlockSessionApi } from "../../hooks/useBlockSession.js";
import { resizeTableRowHeight } from "./model.js";

export interface ColumnResizePreview {
  leftColumnId: string;
  rightColumnId: string;
  leftWidth: number;
  rightWidth: number;
}

export interface RowResizePreview {
  rowId: string;
  height: number;
}

function resizeAdjacentDimensions(
  leading: number,
  trailing: number,
  pointerDelta: number,
  minimum: number,
): { leading: number; trailing: number } {
  const delta = Math.max(minimum - leading, Math.min(trailing - minimum, pointerDelta));
  return { leading: leading + delta, trailing: trailing - delta };
}

/**
 * Owns only transient adjacent-boundary resize previews. Persisted dimensions
 * are committed through BlockSessionApi after pointer release.
 */
export function useTableResize(blockId: string, session: BlockSessionApi) {
  const [columnPreview, setColumnPreview] = useState<ColumnResizePreview | null>(null);
  const [rowPreview, setRowPreview] = useState<RowResizePreview | null>(null);
  const columnPreviewRef = useRef<ColumnResizePreview | null>(null);
  const columnResizeRef = useRef<{
    leftColumnId: string;
    rightColumnId: string;
    startX: number;
    startLeftWidth: number;
    startRightWidth: number;
  } | null>(null);
  const rowPreviewRef = useRef<RowResizePreview | null>(null);
  const rowResizeRef = useRef<{ rowId: string; startY: number; startHeight: number } | null>(null);

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
    columnResizeRef.current = { leftColumnId, rightColumnId, startX: event.clientX, startLeftWidth, startRightWidth };
    columnPreviewRef.current = { leftColumnId, rightColumnId, leftWidth: startLeftWidth, rightWidth: startRightWidth };
    setColumnPreview(columnPreviewRef.current);
    const onMove = (moveEvent: PointerEvent) => {
      const current = columnResizeRef.current;
      if (!current) return;
      const next = resizeAdjacentDimensions(current.startLeftWidth, current.startRightWidth, moveEvent.clientX - current.startX, 32);
      columnPreviewRef.current = { leftColumnId: current.leftColumnId, rightColumnId: current.rightColumnId, leftWidth: next.leading, rightWidth: next.trailing };
      setColumnPreview(columnPreviewRef.current);
    };
    const onEnd = () => {
      const current = columnResizeRef.current;
      columnResizeRef.current = null;
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onEnd);
      window.removeEventListener("pointercancel", onEnd);
      const preview = columnPreviewRef.current;
      if (current && preview?.leftColumnId === current.leftColumnId && preview.rightColumnId === current.rightColumnId) {
        session.setTableColumnWidths(blockId, current.leftColumnId, preview.leftWidth, current.rightColumnId, preview.rightWidth);
      }
      columnPreviewRef.current = null;
      setColumnPreview(null);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onEnd, { once: true });
    window.addEventListener("pointercancel", onEnd, { once: true });
  };

  const beginRowResize = (event: React.PointerEvent<HTMLButtonElement>, rowId: string, rowHeight: number) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    const startHeight = Math.max(34, rowHeight);
    rowResizeRef.current = { rowId, startY: event.clientY, startHeight };
    rowPreviewRef.current = { rowId, height: startHeight };
    setRowPreview(rowPreviewRef.current);
    const onMove = (moveEvent: PointerEvent) => {
      const current = rowResizeRef.current;
      if (!current) return;
      rowPreviewRef.current = { rowId: current.rowId, height: resizeTableRowHeight(current.startHeight, moveEvent.clientY - current.startY) };
      setRowPreview(rowPreviewRef.current);
    };
    const onEnd = () => {
      const current = rowResizeRef.current;
      rowResizeRef.current = null;
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onEnd);
      window.removeEventListener("pointercancel", onEnd);
      const preview = rowPreviewRef.current;
      if (current && preview?.rowId === current.rowId) session.setTableRowHeight(blockId, current.rowId, preview.height);
      rowPreviewRef.current = null;
      setRowPreview(null);
    };
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onEnd, { once: true });
    window.addEventListener("pointercancel", onEnd, { once: true });
  };

  return { columnPreview, rowPreview, beginColumnResize, beginRowResize };
}
