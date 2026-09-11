import { describe, expect, it } from "vitest";

import {
  clearCellCommand,
  createSheetCommand,
  deleteSheetCommand,
  deleteColumnsCommand,
  deleteRowsCommand,
  formatRangeCommands,
  insertColumnsCommand,
  insertRowsCommand,
  mergeAlignment,
  mergeFill,
  mergeFontStyle,
  pasteRangeCommand,
  fillRangeCommand,
  renameSheetCommand,
  setCellCommand,
  setCellFormulaCommand,
  setCellStyleCommand,
  setSheetMetadataCommand,
} from "../src/commands.js";
import type { CellStyle } from "@open-office/schema/artifact";

const EMPTY: CellStyle = { numberFormat: null, font: null, fill: null, alignment: null, borders: null };

describe("spreadsheet semantic commands", () => {
  it("maps setCell to the canonical typeId and camelCase payload", () => {
    const command = setCellCommand("s1", 4, 2, "hello");
    expect(command.typeId).toBe("spreadsheet.setCell");
    expect(command.payload).toEqual({
      type: "setCell",
      sheetId: "s1",
      row: 4,
      column: 2,
      value: "hello",
      formula: null,
      attrs: {},
    });
  });

  it("maps a formula edit to setCell with a formula and null value", () => {
    const command = setCellFormulaCommand("s1", 0, 0, "=A1+B1");
    expect(command.typeId).toBe("spreadsheet.setCell");
    expect(command.payload).toEqual({
      type: "setCell",
      sheetId: "s1",
      row: 0,
      column: 0,
      value: null,
      formula: "=A1+B1",
      attrs: {},
    });
  });

  it("maps clearCell to a typed clearCell command", () => {
    expect(clearCellCommand("s1", 1, 1)).toEqual({
      typeId: "spreadsheet.clearCell",
      payload: { type: "clearCell", sheetId: "s1", row: 1, column: 1 },
    });
  });

  it("maps formatting and metadata to their own commands", () => {
    const style: CellStyle = { numberFormat: "0.00", font: { family: null, size: null, bold: true, italic: false, strikethrough: false, underline: false, color: null }, fill: null, alignment: null, borders: null };
    expect(setCellStyleCommand("s1", 0, 0, style).typeId).toBe("spreadsheet.setCellStyle");
    expect(createSheetCommand("s2", "Sheet 2")).toEqual({
      typeId: "spreadsheet.createSheet",
      payload: { type: "createSheet", id: "s2", name: "Sheet 2" },
    });
    expect(renameSheetCommand("s1", "Rename")).toEqual({
      typeId: "spreadsheet.renameSheet",
      payload: { type: "renameSheet", sheetId: "s1", name: "Rename" },
    });
    expect(deleteSheetCommand("s1").typeId).toBe("spreadsheet.deleteSheet");
    expect(setSheetMetadataCommand("s1", { visibility: "visible", rowCount: null, columnCount: null, freeze: { rows: 1, columns: 0 }, autoFilter: null, sort: null, conditionalFormats: [], dataValidations: [], mergedRanges: [], media: [] }).payload.type).toBe("setSheetMetadata");
  });

  it("maps row/column structural edits to their own commands", () => {
    expect(insertRowsCommand("s1", 3)).toEqual({ typeId: "spreadsheet.insertRows", payload: { type: "insertRows", sheetId: "s1", at: 3, count: 1 } });
    expect(deleteRowsCommand("s1", 3, 2)).toEqual({ typeId: "spreadsheet.deleteRows", payload: { type: "deleteRows", sheetId: "s1", at: 3, count: 2 } });
    expect(insertColumnsCommand("s1", 2)).toEqual({ typeId: "spreadsheet.insertColumns", payload: { type: "insertColumns", sheetId: "s1", at: 2, count: 1 } });
    expect(deleteColumnsCommand("s1", 2)).toEqual({ typeId: "spreadsheet.deleteColumns", payload: { type: "deleteColumns", sheetId: "s1", at: 2, count: 1 } });
  });

  it("builds one command per cell inside a bounded range", () => {
    const commands = formatRangeCommands("s1", { startRow: 0, startColumn: 0, endRow: 1, endColumn: 1 }, (base) => ({ ...base, font: mergeFontStyle(base.font, { bold: true }) }), () => EMPTY);
    expect(commands).toHaveLength(4);
    expect(commands.every((command) => command.typeId === "spreadsheet.setCellStyle")).toBe(true);
  });

  it("merges style patches without leaving required fields undefined", () => {
    const bold = mergeFontStyle(EMPTY.font, { bold: true });
    expect(bold).toEqual({ family: null, size: null, bold: true, italic: false, strikethrough: false, underline: false, color: null });
    expect(mergeFill(EMPTY.fill, { background: "#fff" })).toEqual({ foreground: null, background: "#fff" });
    expect(mergeAlignment(EMPTY.alignment, { horizontal: "center" })).toEqual({ horizontal: "center", vertical: null, wrap: false });
  });

  it("builds one paste or fill command for a whole range", () => {
    const paste = pasteRangeCommand("s1", 10, 5, {
      rowCount: 100,
      columnCount: 100,
      cells: [{ rowOffset: 0, columnOffset: 0, formula: "=A1", attrs: {} }],
      sourceOrigin: { row: 0, column: 0 },
    });
    expect(paste.typeId).toBe("spreadsheet.pasteRange");
    expect(paste.payload.cells).toHaveLength(1);
    expect(paste.payload.rowCount).toBe(100);
    expect(fillRangeCommand(
      "s1",
      { startRow: 0, startColumn: 0, endRow: 0, endColumn: 0 },
      "s2",
      { startRow: 1, startColumn: 0, endRow: 10_000, endColumn: 0 },
    ).typeId).toBe("spreadsheet.fillRange");
  });
});
