import { describe, expect, it } from "vitest";
import type { TableBlock } from "@open-office/schema/artifact";
import { TableGridProjection } from "../src/blocks/table/projection.js";

const table: TableBlock = {
  columns: [{ id: "column-a", width: 120 }, { id: "column-b", width: 180 }],
  rows: [
    { id: "row-a", height: 34, cells: [{ id: "cell-a1", content: { text: "A1", runs: [] } }, { id: "cell-a2", content: { text: "A2", runs: [] } }] },
    { id: "row-b", height: null, cells: [{ id: "cell-b1", content: { text: "B1", runs: [] } }, { id: "cell-b2", content: { text: "B2", runs: [] } }] },
  ],
  mergedRanges: [],
};

describe("TableGridProjection", () => {
  it("indexes stable row, column and cell ids without cloning the table", () => {
    const projection = new TableGridProjection(table);
    expect(projection.table).toBe(table);
    expect(projection.rows).toBe(table.rows);
    expect(projection.rowIndex("row-b")).toBe(1);
    expect(projection.columnIndex("column-a")).toBe(0);
    expect(projection.cell("cell-b2")).toMatchObject({ rowIndex: 1, columnIndex: 1, row: table.rows[1], column: table.columns[1] });
    expect(projection.cell("missing")).toBeNull();
  });

  it("resolves coordinates at command/render time and keeps invalid ids explicit", () => {
    const projection = new TableGridProjection(table);
    expect(projection.cellAt("row-a", "column-b")?.cell.id).toBe("cell-a2");
    expect(projection.cellAt("missing", "column-b")).toBeNull();
    expect(projection.row("row-a")).toBe(table.rows[0]);
    expect(projection.column("column-b")).toBe(table.columns[1]);
    expect(projection.hasRow("missing")).toBe(false);
    expect(projection.cellsInRow("missing")).toEqual([]);
  });
});
