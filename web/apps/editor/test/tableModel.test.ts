import { describe, expect, it } from "vitest";
import {
  columnIndexForSelection,
  canMergeTableSelection,
  mergeableTableRange,
  measureTableGeometry,
  mergedRangeForSelection,
  normalizeTableRange,
  normalizeTableSelection,
  mergedCellProjection,
  rowIndexForSelection,
  resizeTableRowHeight,
  selectionIncludesCell,
  selectionStillExists,
  shouldPromotePointerToTableRange,
  tableCellTextSelectionFormatState,
  tableContextSelectionForCell,
  tableSelectionSpansMultipleCells,
  tableSelectionFormatState,
  tableSelectionText,
  tableBoundaryCrossesMerge,
  tableRangeForSelection,
} from "../src/blocks/table/model.js";
import type { TableBlock } from "@open-office/schema/artifact";

const table: TableBlock = {
  columns: [{ id: "c-a", width: null }, { id: "c-b", width: null }],
  mergedRanges: [{ startRowId: "r-a", endRowId: "r-b", startColumnId: "c-a", endColumnId: "c-a" }],
  rows: [
    { id: "r-a", height: null, cells: [{ id: "a-1", content: { text: "A1", runs: [] } }, { id: "a-2", content: { text: "A2", runs: [] } }] },
    { id: "r-b", height: null, cells: [{ id: "b-1", content: { text: "B1", runs: [] } }, { id: "b-2", content: { text: "B2", runs: [] } }] },
  ],
};

const rect = (top: number, left: number, width: number, height: number): DOMRect => ({
  top,
  left,
  width,
  height,
  right: left + width,
  bottom: top + height,
  x: left,
  y: top,
  toJSON: () => ({}),
} as DOMRect);

describe("table interaction model", () => {
  it("keeps a normal cell press in editing mode until it intentionally crosses a cell boundary", () => {
    expect(shouldPromotePointerToTableRange({
      canStartRange: true,
      sameCell: true,
      startX: 10,
      startY: 10,
      currentX: 40,
      currentY: 10,
    })).toBe(false);
    expect(shouldPromotePointerToTableRange({
      canStartRange: false,
      sameCell: false,
      startX: 10,
      startY: 10,
      currentX: 40,
      currentY: 10,
    })).toBe(false);
    expect(shouldPromotePointerToTableRange({
      canStartRange: true,
      sameCell: false,
      startX: 10,
      startY: 10,
      currentX: 12,
      currentY: 11,
    })).toBe(false);
    expect(shouldPromotePointerToTableRange({
      canStartRange: true,
      sameCell: false,
      startX: 10,
      startY: 10,
      currentX: 20,
      currentY: 10,
    })).toBe(true);
  });

  it("resizes only the row above a horizontal boundary", () => {
    expect(resizeTableRowHeight(40, 18)).toBe(58);
    expect(resizeTableRowHeight(40, -3)).toBe(37);
    expect(resizeTableRowHeight(40, -20)).toBe(34);
  });

  it("resolves semantic row/column ids instead of persisting indexes", () => {
    expect(rowIndexForSelection(table, { kind: "row", id: "r-b" })).toBe(1);
    expect(columnIndexForSelection(table, { kind: "column", id: "c-a" })).toBe(0);
    expect(selectionIncludesCell(table, { kind: "row", id: "r-b" }, "r-b", "c-a")).toBe(true);
    expect(selectionIncludesCell(table, { kind: "row", id: "r-b" }, "r-a", "c-a")).toBe(false);
    expect(selectionStillExists(table, { kind: "column", id: "c-b" })).toBe(true);
    expect(selectionStillExists(table, { kind: "column", id: "missing" })).toBe(false);
  });

  it("serializes selected ranges without coupling clipboard to the renderer", () => {
    expect(tableSelectionText(table, { kind: "row", id: "r-a" })).toBe("A1\tA2");
    expect(tableSelectionText(table, { kind: "column", id: "c-b" })).toBe("A2\nB2");
    expect(tableSelectionText(table, { kind: "all" })).toBe("A1\tA2\nB1\tB2");
    const range = { kind: "range" as const, startRowId: "r-a", endRowId: "r-b", startColumnId: "c-b", endColumnId: "c-b" };
    expect(selectionIncludesCell(table, range, "r-b", "c-b", "b-2")).toBe(true);
    expect(selectionIncludesCell(table, range, "r-a", "c-a", "a-1")).toBe(false);
    expect(tableSelectionText(table, range)).toBe("A2\nB2");
  });

  it("keeps an existing semantic selection when its context menu opens from an included cell", () => {
    const range = { kind: "range" as const, startRowId: "r-a", endRowId: "r-b", startColumnId: "c-b", endColumnId: "c-b" };
    expect(tableContextSelectionForCell(table, range, "r-b", "c-b", "b-2")).toBe(range);

    const row = { kind: "row" as const, id: "r-b" };
    expect(tableContextSelectionForCell(table, row, "r-b", "c-a", "b-1")).toBe(row);

    const all = { kind: "all" as const };
    expect(tableContextSelectionForCell(table, all, "r-a", "c-a", "a-1")).toBe(all);

    expect(tableContextSelectionForCell(table, range, "r-a", "c-a", "a-1")).toEqual({
      kind: "cell",
      rowId: "r-a",
      cellId: "a-1",
    });
  });

  it("projects merge anchors into DOM spans and hides covered cells", () => {
    expect(mergedCellProjection(table, "r-a", "c-a")).toEqual({ rowSpan: 2, colSpan: 1 });
    expect(mergedCellProjection(table, "r-b", "c-a")).toBeNull();
    expect(mergedCellProjection(table, "r-b", "c-b")).toBeUndefined();
  });

  it("does not expose resize splitters inside merged row or column spans", () => {
    expect(tableBoundaryCrossesMerge(table, "row", 1)).toBe(true);
    expect(tableBoundaryCrossesMerge(table, "column", 1)).toBe(false);
    const horizontallyMerged = {
      ...table,
      mergedRanges: [{ startRowId: "r-a", endRowId: "r-a", startColumnId: "c-a", endColumnId: "c-b" }],
    };
    expect(tableBoundaryCrossesMerge(horizontallyMerged, "column", 1)).toBe(true);
    expect(tableBoundaryCrossesMerge(horizontallyMerged, "row", 1)).toBe(false);
    expect(tableBoundaryCrossesMerge(table, "row", 0)).toBe(false);
    expect(tableBoundaryCrossesMerge(table, "row", table.rows.length)).toBe(false);
  });

  it("resolves split actions to the stable persisted merge range", () => {
    expect(mergedRangeForSelection(table, { kind: "cell", rowId: "r-a", cellId: "a-1" })).toEqual(table.mergedRanges[0]);
    expect(mergedRangeForSelection(table, {
      kind: "range",
      startRowId: "r-b",
      endRowId: "r-a",
      startColumnId: "c-a",
      endColumnId: "c-a",
    })).toEqual(table.mergedRanges[0]);
    expect(mergedRangeForSelection(table, { kind: "cell", rowId: "r-b", cellId: "b-1" })).toBeNull();
    expect(mergedRangeForSelection(table, { kind: "column", id: "c-a" })).toEqual(table.mergedRanges[0]);
    expect(mergedRangeForSelection({ ...table, mergedRanges: [{ startRowId: "r-a", endRowId: "r-a", startColumnId: "c-a", endColumnId: "c-b" }] }, { kind: "row", id: "r-a" })).toEqual({
      startRowId: "r-a",
      endRowId: "r-a",
      startColumnId: "c-a",
      endColumnId: "c-b",
    });
  });

  it("enables merge for a multi-cell range that fully contains an existing merged cell", () => {
    expect(canMergeTableSelection(table, {
      kind: "range",
      startRowId: "r-a",
      endRowId: "r-b",
      startColumnId: "c-b",
      endColumnId: "c-b",
    })).toBe(true);
    expect(canMergeTableSelection(table, {
      kind: "range",
      startRowId: "r-a",
      endRowId: "r-b",
      startColumnId: "c-a",
      endColumnId: "c-b",
    })).toBe(true);
    expect(canMergeTableSelection(table, {
      kind: "range",
      startRowId: "r-a",
      endRowId: "r-b",
      startColumnId: "c-a",
      endColumnId: "c-a",
    })).toBe(false);
  });

  it("expands a pointer range to whole merged cells before highlighting or merging", () => {
    const touchedMerge = {
      kind: "range" as const,
      startRowId: "r-a",
      endRowId: "r-a",
      startColumnId: "c-a",
      endColumnId: "c-b",
    };
    const expectedRange = {
      startRowId: "r-a",
      endRowId: "r-b",
      startColumnId: "c-a",
      endColumnId: "c-b",
    };

    expect(normalizeTableRange(table, touchedMerge)).toEqual(expectedRange);
    expect(normalizeTableSelection(table, touchedMerge)).toEqual({ kind: "range", ...expectedRange });
    expect(mergeableTableRange(table, touchedMerge)).toEqual(expectedRange);
    expect(canMergeTableSelection(table, touchedMerge)).toBe(true);
  });

  it("projects row, column, and all selections to mergeable stable ranges", () => {
    const unmergedTable = { ...table, mergedRanges: [] };
    expect(tableRangeForSelection(table, { kind: "row", id: "r-a" })).toEqual({
      startRowId: "r-a",
      endRowId: "r-a",
      startColumnId: "c-a",
      endColumnId: "c-b",
    });
    expect(tableRangeForSelection(table, { kind: "column", id: "c-b" })).toEqual({
      startRowId: "r-a",
      endRowId: "r-b",
      startColumnId: "c-b",
      endColumnId: "c-b",
    });
    expect(tableRangeForSelection(table, { kind: "all" })).toEqual({
      startRowId: "r-a",
      endRowId: "r-b",
      startColumnId: "c-a",
      endColumnId: "c-b",
    });
    expect(canMergeTableSelection(unmergedTable, { kind: "row", id: "r-a" })).toBe(true);
    expect(canMergeTableSelection(unmergedTable, { kind: "row", id: "r-b" })).toBe(true);
    expect(canMergeTableSelection(unmergedTable, { kind: "column", id: "c-b" })).toBe(true);
  });

  it("normalizes a reversed range before dispatching merge", () => {
    expect(tableRangeForSelection(table, {
      kind: "range",
      startRowId: "r-b",
      endRowId: "r-a",
      startColumnId: "c-b",
      endColumnId: "c-a",
    })).toEqual({
      startRowId: "r-a",
      endRowId: "r-b",
      startColumnId: "c-a",
      endColumnId: "c-b",
    });
  });

  it("only promotes a semantic selection to the table toolbar when it spans multiple cells", () => {
    expect(tableSelectionSpansMultipleCells(table, { kind: "cell", rowId: "r-a", cellId: "a-1" })).toBe(false);
    expect(tableSelectionSpansMultipleCells(table, {
      kind: "range",
      startRowId: "r-a",
      endRowId: "r-a",
      startColumnId: "c-a",
      endColumnId: "c-a",
    })).toBe(false);
    expect(tableSelectionSpansMultipleCells(table, {
      kind: "range",
      startRowId: "r-a",
      endRowId: "r-a",
      startColumnId: "c-a",
      endColumnId: "c-b",
    })).toBe(true);
    expect(tableSelectionSpansMultipleCells(table, { kind: "row", id: "r-a" })).toBe(true);
    expect(tableSelectionSpansMultipleCells(table, { kind: "column", id: "c-a" })).toBe(true);
    expect(tableSelectionSpansMultipleCells(table, { kind: "all" })).toBe(true);
  });

  it("derives controlled format state for one cell and a batch range", () => {
    const formattedTable: TableBlock = {
      ...table,
      rows: [
        {
          ...table.rows[0],
          cells: [
            {
              ...table.rows[0].cells[0],
              content: { text: "A1", runs: [{ start: 0, end: 2, style: { ...plainStyle(), bold: true, color: "#f53f3f", fontSize: 16 } }] },
              style: { fillColor: "#d9e7ff", horizontalAlign: "center", verticalAlign: "middle" },
            },
            {
              ...table.rows[0].cells[1],
              content: { text: "A2", runs: [{ start: 0, end: 2, style: { ...plainStyle(), bold: true, color: "#f53f3f", fontSize: 16 } }] },
              style: { fillColor: "#d9e7ff", horizontalAlign: "center", verticalAlign: "middle" },
            },
          ],
        },
      ],
    };
    expect(tableSelectionFormatState(formattedTable, { kind: "cell", rowId: "r-a", cellId: "a-1" })).toMatchObject({
      bold: true,
      fontSize: 16,
      textColor: "#f53f3f",
      fillColor: "#d9e7ff",
      horizontalAlign: "center",
      verticalAlign: "middle",
    });
    expect(tableSelectionFormatState(formattedTable, { kind: "range", startRowId: "r-a", endRowId: "r-a", startColumnId: "c-a", endColumnId: "c-b" })).toMatchObject({
      bold: true,
      textColor: "#f53f3f",
      fillColor: "#d9e7ff",
    });
  });

  it("derives inline state from only the selected text runs in one cell", () => {
    const formattedTable: TableBlock = {
      ...table,
      rows: [{
        ...table.rows[0],
        cells: [{
          ...table.rows[0].cells[0],
          content: {
            text: "A1",
            runs: [
              { start: 0, end: 1, style: { ...plainStyle(), bold: true, color: "#165dff" } },
              { start: 1, end: 2, style: { ...plainStyle(), italic: true, color: "#f53f3f" } },
            ],
          },
          style: { fillColor: "#fff2cc", horizontalAlign: "right", verticalAlign: "bottom" },
        }, table.rows[0].cells[1]],
      }],
    };
    expect(tableCellTextSelectionFormatState(formattedTable, { rowId: "r-a", cellId: "a-1", start: 0, end: 1 })).toMatchObject({
      bold: true,
      italic: false,
      textColor: "#165dff",
      fillColor: "#fff2cc",
      horizontalAlign: "right",
      verticalAlign: "bottom",
    });
    expect(tableCellTextSelectionFormatState(formattedTable, { rowId: "r-a", cellId: "a-1", start: 0, end: 2 })).toMatchObject({
      bold: false,
      italic: false,
      textColor: null,
    });
  });

  it("projects DOM rectangles into keyed gutter geometry", () => {
    const geometry = measureTableGeometry(
      rect(100, 200, 420, 70),
      rect(100, 200, 420, 70),
      [rect(100, 200, 420, 35), rect(135, 200, 420, 35)],
      [rect(100, 200, 210, 70), rect(100, 410, 210, 70)],
      ["r-a", "r-b"],
      ["c-a", "c-b"],
    );
    expect(geometry.rows).toEqual([
      { id: "r-a", top: 0, height: 35 },
      { id: "r-b", top: 35, height: 35 },
    ]);
    expect(geometry.columns).toEqual([
      { id: "c-a", left: 0, width: 210 },
      { id: "c-b", left: 210, width: 210 },
    ]);
    expect(geometry.rowBoundaries).toEqual([0, 35, 70]);
    expect(geometry.columnBoundaries).toEqual([0, 210, 420]);
  });
});

function plainStyle() {
  return { bold: false, italic: false, underline: false, strikethrough: false, fontFamily: null, fontSize: null, color: null, highlight: null, verticalAlign: null };
}
