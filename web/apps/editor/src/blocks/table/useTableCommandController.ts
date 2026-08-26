import { useCallback, useMemo, type RefObject } from "react";

import type { TableBlock } from "@open-office/schema/artifact";

import type { BlockSessionApi } from "../../hooks/useBlockSession.js";
import { restoreTableCellTextSelection, type TableCellTextSelection } from "../../utils/blockSelection.js";
import { tableCommandSelection, tableTextAttrsToInlinePatch } from "./commands.js";
import {
  canMergeTableSelection,
  columnIndexForSelection,
  mergedRangeForSelection,
  mergeableTableRange,
  rowIndexForSelection,
  tableCellTextSelectionFormatState,
  tableSelectionFormatState,
  tableSelectionText,
  type TableFormatState,
  type TableSelection,
} from "./model.js";

const EMPTY_TABLE_FORMAT_STATE: TableFormatState = {
  bold: false,
  italic: false,
  underline: false,
  strikethrough: false,
  fontSize: null,
  textColor: null,
  highlightColor: null,
  fillColor: null,
  horizontalAlign: null,
  verticalAlign: null,
};

/**
 * The sole command coordinator for a selected table. It converts the
 * transient stable-id selection into semantic session commands, allowing the
 * context menu and floating toolbar to share exactly the same mutation path.
 */
export function useTableCommandController({
  blockId,
  table,
  selection,
  cellTextSelection,
  session,
  rootRef,
  commitSelection,
  onDismiss,
}: {
  blockId: string;
  table: TableBlock | null;
  selection: TableSelection | null;
  cellTextSelection: TableCellTextSelection | null;
  session: BlockSessionApi;
  rootRef: RefObject<HTMLDivElement | null>;
  commitSelection: (selection: TableSelection | null) => TableSelection | null;
  onDismiss: () => void;
}) {
  const splitRange = useMemo(() => table ? mergedRangeForSelection(table, selection) : null, [selection, table]);
  const canMerge = useMemo(() => table ? canMergeTableSelection(table, selection) : false, [selection, table]);
  const formatState = useMemo(() => table
    ? tableCellTextSelectionFormatState(table, cellTextSelection) ?? tableSelectionFormatState(table, selection)
    : EMPTY_TABLE_FORMAT_STATE, [cellTextSelection, selection, table]);

  const insertAtSelection = useCallback((direction: "before" | "after") => {
    if (!table || !selection || (selection.kind !== "row" && selection.kind !== "column")) return;
    const index = selection.kind === "row"
      ? rowIndexForSelection(table, selection)
      : columnIndexForSelection(table, selection);
    if (index === null) return;
    const boundaryIndex = index + (direction === "after" ? 1 : 0);
    if (selection.kind === "row") session.insertTableRow(blockId, boundaryIndex);
    else session.insertTableColumn(blockId, boundaryIndex);
    onDismiss();
  }, [blockId, onDismiss, selection, session, table]);

  const deleteSelection = useCallback(() => {
    if (!table || !selection || (selection.kind !== "row" && selection.kind !== "column")) return;
    const index = selection.kind === "row"
      ? rowIndexForSelection(table, selection)
      : columnIndexForSelection(table, selection);
    if (index === null) return;
    if (selection.kind === "row") session.deleteTableRow(blockId, index);
    else session.deleteTableColumn(blockId, index);
    commitSelection(null);
    onDismiss();
  }, [blockId, commitSelection, onDismiss, selection, session, table]);

  const copySelection = useCallback(async () => {
    if (!table || !selection) return;
    try {
      await navigator.clipboard?.writeText(tableSelectionText(table, selection));
    } catch {
      // Clipboard permission is optional. A denied write must not invalidate
      // the table selection or block the follow-up command.
    }
    onDismiss();
  }, [onDismiss, selection, table]);

  const cutSelection = useCallback(() => {
    void copySelection();
    deleteSelection();
  }, [copySelection, deleteSelection]);

  const mergeSelection = useCallback(() => {
    if (!table) return;
    const range = mergeableTableRange(table, selection);
    if (!canMerge || !range) return;
    session.mergeTableCells(blockId, range);
    // Keep the persisted merge range selected so Split is immediately the
    // available follow-up action.
    commitSelection({ kind: "range", ...range });
    onDismiss();
  }, [blockId, canMerge, commitSelection, onDismiss, selection, session, table]);

  const splitSelection = useCallback(() => {
    if (!splitRange) return;
    session.splitTableCells(blockId, splitRange);
    onDismiss();
  }, [blockId, onDismiss, session, splitRange]);

  const applyBorderPreset = useCallback((
    preset: Parameters<BlockSessionApi["applyTableBorderPreset"]>[2],
    border?: Parameters<BlockSessionApi["applyTableBorderPreset"]>[3],
  ) => {
    if (!selection) return;
    session.applyTableBorderPreset(blockId, tableCommandSelection(selection), preset, border);
    onDismiss();
  }, [blockId, onDismiss, selection, session]);

  const formatSelection = useCallback((patch: {
    textAttrs?: Record<string, unknown>;
    fillColor?: string | null;
    horizontalAlign?: "left" | "center" | "right";
    verticalAlign?: "top" | "middle" | "bottom";
  }) => {
    if (!selection) return;
    const inlinePatch = cellTextSelection && patch.textAttrs
      ? tableTextAttrsToInlinePatch(patch.textAttrs)
      : null;
    if (cellTextSelection && inlinePatch) {
      session.patchTableCellInlineRange(
        blockId,
        cellTextSelection.rowId,
        cellTextSelection.cellId,
        { start: cellTextSelection.start, end: cellTextSelection.end },
        inlinePatch,
      );
      requestAnimationFrame(() => {
        if (rootRef.current) restoreTableCellTextSelection(rootRef.current, cellTextSelection);
      });
    } else {
      session.formatTableCells(blockId, tableCommandSelection(selection), patch);
    }
    onDismiss();
  }, [blockId, cellTextSelection, onDismiss, rootRef, selection, session]);

  return {
    canMerge,
    canSplit: splitRange !== null,
    formatState,
    insertAtSelection,
    deleteSelection,
    copySelection,
    cutSelection,
    mergeSelection,
    splitSelection,
    applyBorderPreset,
    formatSelection,
  };
}
