import { describe, expect, it } from "vitest";

import {
  copyRange,
  mergedAnchor,
  visibleGridCells,
  moveCell,
  pasteTargets,
  projectClipboardRange,
  parseSpreadsheetClipboard,
  serializeSpreadsheetClipboard,
  rangeCoordinates,
  selectionRange,
  type CellSelection,
  type GridBounds,
} from "../src/selection.js";

const bounds: GridBounds = { rows: 100, columns: 26 };

describe("spreadsheet selection helpers", () => {
  it("computes the inclusive range for cells, row, column and all selections", () => {
    const cells: CellSelection = { kind: "cells", anchor: { row: 2, column: 3 }, focus: { row: 5, column: 7 } };
    expect(selectionRange(cells, bounds)).toEqual({ startRow: 2, startColumn: 3, endRow: 5, endColumn: 7 });
    expect(selectionRange({ kind: "row", row: 2 }, bounds)).toEqual({ startRow: 2, endRow: 2, startColumn: 0, endColumn: 25 });
    expect(selectionRange({ kind: "column", column: 3 }, bounds)).toEqual({ startRow: 0, endRow: 99, startColumn: 3, endColumn: 3 });
    expect(selectionRange({ kind: "all" }, bounds)).toEqual({ startRow: 0, endRow: 99, startColumn: 0, endColumn: 25 });
    expect(selectionRange(null, bounds)).toBeNull();
  });

  it("collapses a normal selection to one focused cell", () => {
    const cells: CellSelection = { kind: "cells", anchor: { row: 1, column: 1 }, focus: { row: 4, column: 4 } };
    expect(selectionRange(cells, bounds)?.endColumn).toBe(4);
  });

  it("moves the active cell with clamps and direction", () => {
    expect(moveCell({ row: 5, column: 5 }, "right", bounds)).toEqual({ row: 5, column: 6 });
    expect(moveCell({ row: 5, column: 5 }, "down", bounds)).toEqual({ row: 6, column: 5 });
    expect(moveCell({ row: 0, column: 0 }, "up", bounds)).toEqual({ row: 0, column: 0 });
    expect(moveCell({ row: 2, column: 5 }, "home", bounds)).toEqual({ row: 2, column: 0 });
    expect(moveCell({ row: 2, column: 5 }, "end", bounds)).toEqual({ row: 2, column: 25 });
    expect(moveCell({ row: 97, column: 5 }, "pageDown", bounds)).toEqual({ row: 99, column: 5 });
  });

  it("enumerates the cells inside a range", () => {
    const coords = rangeCoordinates({ startRow: 0, startColumn: 0, endRow: 1, endColumn: 1 });
    expect(coords).toEqual([{ row: 0, column: 0 }, { row: 0, column: 1 }, { row: 1, column: 0 }, { row: 1, column: 1 }]);
  });

  it("copies a sparse range into a row-major clipboard", () => {
    const cells = [
      { row: 0, column: 0, value: "a" },
      { row: 1, column: 1, value: 42 },
    ];
    const clip = copyRange(cells, { startRow: 0, startColumn: 0, endRow: 1, endColumn: 1 });
    expect(clip.values[0][0]).toBe("a");
    expect(clip.values[0][1]).toBeUndefined();
    expect(clip.values[1][1]).toBe(42);
  });

  it("computes paste destinations bounded by the grid", () => {
    const clip = { startRow: 0, startColumn: 0, values: [[1, 2], [3, 4]] };
    const targets = pasteTargets(clip, { row: 1, column: 1 }, { rows: 10, columns: 10 });
    expect(targets).toEqual([
      { row: 1, column: 1, value: 1 },
      { row: 1, column: 2, value: 2 },
      { row: 2, column: 1, value: 3 },
      { row: 2, column: 2, value: 4 },
    ]);
  });

  it("projects only selected coordinates and detaches nested clipboard data", () => {
    const cells = new Map([
      ["5:7", { row: 5, column: 7, value: { nested: [1, 2] }, formula: "=A1", attrs: { note: { text: "source" } }, style: { numberFormat: null, font: null, fill: { foreground: null, background: "#fff" }, alignment: null, borders: null } }],
    ]);
    let lookups = 0;
    const projection = projectClipboardRange(
      { startRow: 5, startColumn: 7, endRow: 6, endColumn: 8 },
      (row, column) => {
        lookups += 1;
        return cells.get(`${row}:${column}`) ?? null;
      },
    );

    expect(lookups).toBe(4);
    expect(projection).toEqual({
      rowCount: 2,
      columnCount: 2,
      cells: [{
        rowOffset: 0,
        columnOffset: 0,
        value: { nested: [1, 2] },
        formula: "=A1",
        attrs: { note: { text: "source" } },
        style: { numberFormat: null, font: null, fill: { foreground: null, background: "#fff" }, alignment: null, borders: null },
      }],
      sourceOrigin: { row: 5, column: 7 },
    });

    (cells.get("5:7")?.value as { nested: number[] }).nested.push(3);
    ((cells.get("5:7")?.attrs.note as { text: string })).text = "changed";
    if (cells.get("5:7")?.style?.fill) cells.get("5:7")!.style!.fill!.background = "#000";
    expect(projection.cells[0].value).toEqual({ nested: [1, 2] });
    expect(projection.cells[0].attrs).toEqual({ note: { text: "source" } });
    expect(projection.cells[0].style?.fill?.background).toBe("#fff");
  });

  it("round-trips quoted TSV and parses external CSV formulas", () => {
    const projection = {
      rowCount: 2,
      columnCount: 2,
      cells: [
        { rowOffset: 0, columnOffset: 0, value: "a\tb", attrs: {}, style: null },
        { rowOffset: 0, columnOffset: 1, formula: "=A1", attrs: {}, style: null },
        { rowOffset: 1, columnOffset: 0, value: "line\nbreak", attrs: {}, style: null },
      ],
      sourceOrigin: { row: 4, column: 3 },
    };
    const text = serializeSpreadsheetClipboard(projection);
    expect(text).toBe('"a\tb"\t=A1\n"line\nbreak"\t');
    expect(parseSpreadsheetClipboard(text)).toEqual({
      rowCount: 2,
      columnCount: 2,
      cells: [
        { rowOffset: 0, columnOffset: 0, value: "a\tb", attrs: {}, style: null },
        { rowOffset: 0, columnOffset: 1, formula: "=A1", attrs: {}, style: null },
        { rowOffset: 1, columnOffset: 0, value: "line\nbreak", attrs: {}, style: null },
      ],
    });
    expect(parseSpreadsheetClipboard('1,TRUE,"hello,world"')).toEqual({
      rowCount: 1,
      columnCount: 3,
      cells: [
        { rowOffset: 0, columnOffset: 0, value: 1, attrs: {}, style: null },
        { rowOffset: 0, columnOffset: 1, value: true, attrs: {}, style: null },
        { rowOffset: 0, columnOffset: 2, value: "hello,world", attrs: {}, style: null },
      ],
    });
  });

  it("copies a small range without consulting unrelated sparse cells", () => {
    const lookup = new Map<string, { row: number; column: number; value: number; attrs: Record<string, unknown> }>();
    for (let index = 0; index < 100_000; index += 1) {
      lookup.set(`${index}:0`, { row: index, column: 0, value: index, attrs: {} });
    }
    let lookups = 0;
    const projection = projectClipboardRange(
      { startRow: 50_000, startColumn: 0, endRow: 50_009, endColumn: 9 },
      (row, column) => {
        lookups += 1;
        return lookup.get(`${row}:${column}`) ?? null;
      },
    );
    expect(lookups).toBe(100);
    expect(projection.cells).toHaveLength(10);
  });

  it("clips paste targets at the right edge", () => {
    const clip = { startRow: 0, startColumn: 0, values: [[1, 2, 3]] };
    // bounds.columns = 2 means valid columns are 0 and 1; offset 1 lands on
    // column 2 which is out of bounds and is clipped.
    const targets = pasteTargets(clip, { row: 0, column: 1 }, { rows: 10, columns: 2 });
    expect(targets).toEqual([{ row: 0, column: 1, value: 1 }]);
  });
});


describe("merged cell geometry and navigation", () => {
  const merge = { startRow: 0, endRow: 1, startColumn: 0, endColumn: 1 };
  it("skips the merged rectangle on navigation and resolves incoming cells to its anchor", () => {
    expect(moveCell({ row: 0, column: 0 }, "right", bounds, 20, [merge])).toEqual({ row: 0, column: 2 });
    expect(moveCell({ row: 0, column: 1 }, "down", bounds, 20, [merge])).toEqual({ row: 2, column: 0 });
    expect(moveCell({ row: 2, column: 1 }, "up", bounds, 20, [merge])).toEqual({ row: 0, column: 0 });
    expect(mergedAnchor({ row: 1, column: 1 }, [merge])).toEqual({ row: 0, column: 0 });
  });
  it("renders a merged anchor once when only the lower part intersects the viewport", () => {
    expect(visibleGridCells({ startRow: 5, endRow: 6, startColumn: 0, endColumn: 2 }, [{ ...merge, endRow: 9 }])).toEqual([
      { row: 0, column: 0 }, { row: 5, column: 2 }, { row: 6, column: 2 },
    ]);
  });
  it("expands chained intersections without modifying the view selection", () => {
    const selection: CellSelection = { kind: "cells", anchor: { row: 1, column: 1 }, focus: { row: 1, column: 3 } };
    expect(selectionRange(selection, bounds, [
      { startRow: 2, endRow: 4, startColumn: 0, endColumn: 1 },
      { startRow: 1, endRow: 2, startColumn: 3, endColumn: 3 }, merge,
    ])).toEqual({ startRow: 0, endRow: 4, startColumn: 0, endColumn: 3 });
    expect(selection.anchor).toEqual({ row: 1, column: 1 });
  });
});
