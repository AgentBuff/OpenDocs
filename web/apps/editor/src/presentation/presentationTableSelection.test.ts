import { describe, expect, it } from "vitest";

import { plainPresentationRichText, type PresentationV5TableCell } from "@open-office/schema";

import {
  tableAnchorAt,
  tableAnchorsInSelection,
  tableRange,
  tableSelectionCanMerge,
  type PresentationTableNode,
} from "./presentationTableSelection.js";

const cell = (row: number, column: number, rowSpan = 1, columnSpan = 1): PresentationV5TableCell => ({
  row,
  column,
  rowSpan,
  columnSpan,
  content: plainPresentationRichText(`${row}:${column}`),
  style: { fill: { type: "none" }, horizontalAlign: "left", verticalAlign: "top" },
});

const tableNode = (cells: PresentationV5TableCell[]): PresentationTableNode => ({
  id: "table-1",
  parentId: null,
  orderKey: "0001",
  name: "table",
  altText: null,
  layoutPlaceholderId: null,
  transform: { x: 0, y: 0, width: 400, height: 200, rotation: 0 },
  visible: true,
  locked: false,
  opacity: 1,
  kind: { type: "table", data: { rows: 2, columns: 2, cells } },
});

describe("presentation table selection helpers", () => {
  it("normalizes a reverse drag into a stable rectangular range", () => {
    expect(tableRange({ anchor: { row: 1, column: 1 }, focus: { row: 0, column: 0 } })).toEqual({
      start: { row: 0, column: 0 },
      end: { row: 1, column: 1 },
    });
  });

  it("resolves covered coordinates to their merged anchor", () => {
    const merged = cell(0, 0, 2, 2);
    expect(tableAnchorAt(tableNode([merged]), { row: 1, column: 1 })).toBe(merged);
  });

  it("finds intersecting anchors and rejects partial or already-merged ranges", () => {
    const simple = tableNode([cell(0, 0), cell(0, 1), cell(1, 0), cell(1, 1)]);
    const selection = { anchor: { row: 0, column: 0 }, focus: { row: 1, column: 1 } };
    expect(tableAnchorsInSelection(simple, selection)).toHaveLength(4);
    expect(tableSelectionCanMerge(simple, selection)).toBe(true);

    const merged = tableNode([cell(0, 0, 1, 2), cell(1, 0), cell(1, 1)]);
    expect(tableSelectionCanMerge(merged, selection)).toBe(false);
    expect(tableSelectionCanMerge(simple, { anchor: { row: 0, column: 0 }, focus: { row: 0, column: 0 } })).toBe(false);
  });
});
