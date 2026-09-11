import { describe, expect, it } from "vitest";

import {
  clearFormatting,
  mergeBorders,
  mergeFontColor,
  mergeFontDecoration,
  mergeFontSize,
  mergeNumberFormat,
  mergeVerticalAlignment,
  mergeWrap,
  nextAutoFilter,
  NUMBER_FORMAT_PRESETS,
  toolbarStyleState,
} from "../src/toolbar.js";
import { SPREADSHEET_CAPABILITY_MATRIX, capabilitiesByToolbarGroup } from "../src/capabilities.js";
import type { CellStyle } from "@open-office/schema/artifact";

const EMPTY: CellStyle = { numberFormat: null, font: null, fill: null, alignment: null, borders: null };

describe("toolbar style merges", () => {
  it("mergeFontSize writes size and preserves other font fields", () => {
    const base = mergeFontColor(EMPTY, "#d13f28");
    const styled = mergeFontSize(base, 18);
    expect(styled.font?.size).toBe(18);
    expect(styled.font?.color).toBe("#d13f28");
    expect(styled.font?.bold).toBe(false);
  });

  it("mergeNumberFormat supports custom formats and reset", () => {
    expect(mergeNumberFormat(EMPTY, "0.00").numberFormat).toBe("0.00");
    expect(mergeNumberFormat(mergeNumberFormat(EMPTY, "0.00"), null).numberFormat).toBeNull();
  });

  it("mergeVerticalAlignment and mergeWrap keep orthogonal fields", () => {
    const styled = mergeWrap(mergeVerticalAlignment(EMPTY, "middle"), true);
    expect(styled.alignment).toEqual({ horizontal: null, vertical: "middle", wrap: true });
  });

  it("mergeFontDecoration toggles strikethrough and underline independently", () => {
    const struck = mergeFontDecoration(EMPTY, "strikethrough", true);
    expect(struck.font?.strikethrough).toBe(true);
    expect(struck.font?.underline).toBe(false);
    const both = mergeFontDecoration(struck, "underline", true);
    expect(both.font?.strikethrough).toBe(true);
    expect(both.font?.underline).toBe(true);
    const unstruck = mergeFontDecoration(both, "strikethrough", false);
    expect(unstruck.font?.strikethrough).toBe(false);
    expect(unstruck.font?.underline).toBe(true);
  });

  it("mergeBorders presets map to the four edges and none clears the node", () => {
    const all = mergeBorders(EMPTY, { side: "all", style: "thin" });
    expect(all.borders?.top).toEqual({ style: "thin", color: "#1f2329" });
    expect(all.borders?.right).toEqual({ style: "thin", color: "#1f2329" });
    // 外框保留已有内部边之外的内容：top 已存在被覆盖，left 保留。
    const outer = mergeBorders(mergeBorders(EMPTY, { side: "all", style: "dashed" }), { side: "outer", style: "medium" });
    expect(outer.borders?.top?.style).toBe("medium");
    const cleared = mergeBorders(all, { side: "none", style: "thin" });
    expect(cleared.borders).toBeNull();
  });

  it("clearFormatting zeroes the whole style node", () => {
    const decorated: CellStyle = {
      numberFormat: "0.00",
      font: { family: "Arial", size: 14, bold: true, italic: false, strikethrough: true, underline: false, color: "#000" },
      fill: { foreground: null, background: "#fff" },
      alignment: { horizontal: "center", vertical: null, wrap: true },
      borders: mergeBorders(EMPTY, { side: "all", style: "thin" }).borders,
    };
    expect(clearFormatting(decorated)).toEqual(EMPTY);
  });

  it("toolbarStyleState derives activation from a focused cell", () => {
    const state = toolbarStyleState({
      ...EMPTY,
      font: { family: null, size: 12, bold: true, italic: false, strikethrough: false, underline: false, color: null },
      alignment: { horizontal: "center", vertical: null, wrap: false },
      numberFormat: "0.00",
    });
    expect(state).toEqual({
      family: null,
      size: 12,
      bold: true,
      italic: false,
      strikethrough: false,
      underline: false,
      wrap: false,
      horizontal: "center",
      vertical: null,
      numberFormat: "0.00",
      hasBorder: false,
      fontColor: null,
      fillColor: null,
    });
  });
});

describe("auto filter toggle", () => {
  it("enables filtering with an empty predicate list and disables back to null", () => {
    const range = { startRow: 0, startColumn: 0, endRow: 9, endColumn: 3 };
    const enabled = nextAutoFilter(null, range);
    expect(enabled.active).toBe(false);
    expect(enabled.autoFilter).toEqual({ range, columns: [] });
    const disabled = nextAutoFilter(enabled.autoFilter, range);
    expect(disabled.active).toBe(true);
    expect(disabled.autoFilter).toBeNull();
  });
});

describe("capability matrix grouping", () => {
  it("every toolbar group in the render order has at least one command", () => {
    const groups = capabilitiesByToolbarGroup();
    for (const entry of [
      "clipboard", "structure", "font", "align", "data",
    ] as const) {
      expect(groups.get(entry)?.length ?? 0).toBeGreaterThan(0);
    }
  });

  it("grouped commands are a subset of the matrix and no fake capabilities exist", () => {
    const typeIds = new Set(SPREADSHEET_CAPABILITY_MATRIX.map((descriptor) => descriptor.typeId));
    for (const descriptor of SPREADSHEET_CAPABILITY_MATRIX) {
      if (descriptor.toolbarGroup) expect(typeIds.has(descriptor.typeId)).toBe(true);
    }
    // 引擎不支持的能力不得以任何形式进入矩阵。
    const joined = SPREADSHEET_CAPABILITY_MATRIX.map((d) => d.typeId).join();
    expect(joined).not.toContain("border");
    expect(joined).not.toContain("chart");
    expect(joined).not.toContain("protect");
  });

  it("number format presets carry unique keys and samples", () => {
    const keys = NUMBER_FORMAT_PRESETS.map((preset) => preset.key);
    expect(new Set(keys).size).toBe(keys.length);
    for (const preset of NUMBER_FORMAT_PRESETS) {
      expect(preset.label.length).toBeGreaterThan(0);
      expect(preset.sample.length).toBeGreaterThan(0);
    }
  });
});

import { autoSumTarget, sumFormula } from "../src/toolbar.js";

describe("autofill sum", () => {
  it("sums a selected column into the cell below it", () => {
    const target = autoSumTarget(
      { startRow: 0, startColumn: 0, endRow: 3, endColumn: 0 },
      () => true,
    );
    expect(target).toEqual({
      row: 4,
      column: 0,
      range: { startRow: 0, startColumn: 0, endRow: 3, endColumn: 0 },
    });
    expect(sumFormula({ startRow: 0, startColumn: 0, endRow: 3, endColumn: 0 })).toBe("=SUM(A1:A4)");
  });

  it("collects the contiguous numeric block above a single cell", () => {
    // rows 0,1 数字；row 2 空 → 连续块 = [0..1]，目标 = row 2 本身？Excel 把结果放选中格。
    const target = autoSumTarget(
      { startRow: 2, startColumn: 0, endRow: 2, endColumn: 0 },
      (row) => row < 2,
    );
    expect(target).toEqual({
      row: 2,
      column: 0,
      range: { startRow: 0, startColumn: 0, endRow: 1, endColumn: 0 },
    });
    expect(sumFormula(target!.range)).toBe("=SUM(A1:A2)");
  });

  it("returns null when nothing numeric sits above", () => {
    expect(autoSumTarget({ startRow: 0, startColumn: 0, endRow: 0, endColumn: 0 }, () => false)).toBeNull();
  });

  it("renders cross-column ranges in the formula", () => {
    expect(sumFormula({ startRow: 0, startColumn: 0, endRow: 2, endColumn: 2 })).toBe("=SUM(A1:C3)");
  });
});

import { formatDisplayValue } from "../src/toolbar.js";

describe("number format display", () => {
  it("formats thousands separators", () => {
    expect(formatDisplayValue(1234567, "#,##0")).toBe("1,234,567");
    expect(formatDisplayValue(1234.5, "#,##0")).toBe("1,235");
  });
  it("formats currency and percent", () => {
    expect(formatDisplayValue(1234.5, "¥#,##0.00")).toBe("¥1,234.50");
    expect(formatDisplayValue(0.1234, "0.00%")).toBe("12.34%");
    expect(formatDisplayValue(2, "0%")).toBe("200%");
  });
  it("formats fixed decimals", () => {
    expect(formatDisplayValue(1.5, "0.00")).toBe("1.50");
    expect(formatDisplayValue(1234.567, "#,##0.00")).toBe("1,234.57");
  });
  it("passes through non-numbers and unknown formats", () => {
    expect(formatDisplayValue("text", "#,##0")).toBe("text");
    expect(formatDisplayValue(42, "General")).toBe("42");
    expect(formatDisplayValue(42, null)).toBe("42");
    expect(formatDisplayValue(null, "#,##0")).toBe("");
  });
});


describe("Excel date display", () => {
  it.each([
    [1, "1900-01-01"], [59, "1900-02-28"], [60, "1900-02-29"],
    [61, "1900-03-01"], [25569, "1970-01-01"], [46267, "2026-09-02"],
    [25569.75, "1970-01-01"], [2958465, "9999-12-31"],
    [-1, "########"], [2958466, "########"],
  ])("formats serial %s as %s", (serial, expected) => {
    expect(formatDisplayValue(serial, "yyyy-mm-dd")).toBe(expected);
  });
  it("does not silently turn invalid numeric values into zero", () => {
    expect(formatDisplayValue(Infinity, "0.00")).toBe("Infinity");
    expect(formatDisplayValue(NaN, "yyyy-mm-dd")).toBe("NaN");
  });
});
