import { describe, expect, it } from "vitest";

import { parseSpreadsheetGridProjection, parseSpreadsheetStructureProjection } from "../src/api.js";

const cell = {
  address: { sheetId: "s1", row: 0, column: 0 },
  value: "hello",
  formula: null,
  style: null,
};

function validProjection(overrides: Record<string, unknown> = {}) {
  return {
    sheetId: "s1",
    startRow: 0,
    endRow: 10,
    startColumn: 0,
    endColumn: 5,
    cells: [cell],
    cellCount: 1,
    sparse: true,
    values: { "0:1": { type: "number", value: 5 } },
    ...overrides,
  };
}

describe("spreadsheet grid projection parser", () => {
  it("parses structure separately and rejects leaked sparse cells", () => {
    const structure = {
      metadata: { activeSheetId: "s1", calculationMode: "automatic", dateSystem: "excel1900" },
      sheets: [{
        id: "s1",
        name: "Sheet 1",
        cells: [],
        metadata: { visibility: "visible", rowCount: 100_000, columnCount: 100, freeze: { rows: 0, columns: 0 }, autoFilter: null, sort: null, conditionalFormats: [], dataValidations: [], mergedRanges: [], media: [] },
      }],
    };
    expect(parseSpreadsheetStructureProjection(structure).sheets[0].cells).toEqual([]);
    structure.sheets[0].cells = [{ row: 99_999, column: 99, value: "far", attrs: {} }] as never;
    expect(() => parseSpreadsheetStructureProjection(structure)).toThrow("不得包含 cell payload");
  });

  it("parses a valid spreadsheet grid window", () => {
    const projection = parseSpreadsheetGridProjection(validProjection());
    expect(projection.sheetId).toBe("s1");
    expect(projection.cells).toHaveLength(1);
    expect(projection.cells[0].address).toEqual({ sheetId: "s1", row: 0, column: 0 });
    expect(projection.cells[0].value).toBe("hello");
    expect(projection.values["0:1"]).toEqual({ type: "number", value: 5 });
    expect(projection.sparse).toBe(true);
  });

  it("rejects an inverted or empty window", () => {
    expect(() => parseSpreadsheetGridProjection(validProjection({ startRow: 10, endRow: 5 }))).toThrow();
    expect(() => parseSpreadsheetGridProjection(validProjection({ startColumn: 5, endColumn: 5 }))).toThrow();
  });

  it("rejects a cell whose sheetId does not match the projection", () => {
    const projection = validProjection();
    projection.cells = [{ ...cell, address: { sheetId: "other", row: 0, column: 0 } }];
    expect(() => parseSpreadsheetGridProjection(projection)).toThrow();
  });

  it("rejects a mismatched cellCount", () => {
    expect(() => parseSpreadsheetGridProjection(validProjection({ cellCount: 3 }))).toThrow();
  });

  it("parses all derived value shapes", () => {
    const projection = parseSpreadsheetGridProjection(validProjection({
      values: {
        "0:0": { type: "blank" },
        "0:1": { type: "number", value: 5 },
        "0:2": { type: "text", value: "x" },
        "0:3": { type: "bool", value: true },
        "0:4": { type: "error", value: { code: "divisionByZero", message: "boom" } },
      },
    }));
    expect(projection.values["0:0"]).toEqual({ type: "blank" });
    expect(projection.values["0:4"]).toEqual({ type: "error", value: { code: "divisionByZero", message: "boom" } });
  });
});
