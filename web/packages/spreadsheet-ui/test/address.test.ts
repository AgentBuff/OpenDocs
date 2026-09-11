import { describe, expect, it } from "vitest";

import {
  cellRef,
  clampCell,
  columnToLetters,
  lettersToColumn,
  parseCellRef,
  parseRangeRef,
  rangeRef,
} from "../src/address.js";

describe("spreadsheet address helpers", () => {
  it("converts 0-based columns to column letters", () => {
    expect(columnToLetters(0)).toBe("A");
    expect(columnToLetters(25)).toBe("Z");
    expect(columnToLetters(26)).toBe("AA");
    expect(columnToLetters(27)).toBe("AB");
    expect(columnToLetters(52)).toBe("BA");
    expect(columnToLetters(701)).toBe("ZZ");
    expect(columnToLetters(702)).toBe("AAA");
  });

  it("rejects invalid columns", () => {
    expect(() => columnToLetters(-1)).toThrow();
    expect(() => columnToLetters(1.5)).toThrow();
  });

  it("converts column letters back to 0-based columns", () => {
    expect(lettersToColumn("A")).toBe(0);
    expect(lettersToColumn("Z")).toBe(25);
    expect(lettersToColumn("AA")).toBe(26);
    expect(lettersToColumn("aaa")).toBe(702);
    expect(lettersToColumn("ZZ")).toBe(701);
  });

  it("parses A1 cell references into 0-based coordinates", () => {
    expect(parseCellRef("A1")).toEqual({ row: 0, column: 0 });
    expect(parseCellRef("B2")).toEqual({ row: 1, column: 1 });
    expect(parseCellRef("AA10")).toEqual({ row: 9, column: 26 });
  });

  it("formats 0-based coordinates into A1 references", () => {
    expect(cellRef(0, 0)).toBe("A1");
    expect(cellRef(9, 26)).toBe("AA10");
  });

  it("round-trips cell references", () => {
    for (const ref of ["A1", "B2", "Z100", "AA10", "XFD1048576"]) {
      expect(cellRef(parseCellRef(ref).row, parseCellRef(ref).column)).toBe(ref);
    }
  });

  it("formats and parses ranges", () => {
    expect(rangeRef({ startRow: 0, startColumn: 0, endRow: 0, endColumn: 0 })).toBe("A1");
    expect(rangeRef({ startRow: 0, startColumn: 0, endRow: 1, endColumn: 1 })).toBe("A1:B2");
    expect(parseRangeRef("A1:B2")).toEqual({ startRow: 0, startColumn: 0, endRow: 1, endColumn: 1 });
    expect(parseRangeRef("B2")).toEqual({ startRow: 1, startColumn: 1, endRow: 1, endColumn: 1 });
  });

  it("clamps coordinates into a bounded range", () => {
    expect(clampCell(-1, 5, 10, 10)).toEqual({ row: 0, column: 5 });
    expect(clampCell(11, 5, 10, 10)).toEqual({ row: 10, column: 5 });
    expect(clampCell(5, 5, 10, 10)).toEqual({ row: 5, column: 5 });
  });
});
