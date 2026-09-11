import { expect, test } from "vitest";
import type { CellModel } from "@open-office/schema/artifact";
import { spreadsheetRowMetrics } from "../src/spreadsheet/row-metrics.js";

test("large fonts grow one row and scrolling resolves the correct row boundaries", () => {
  const cells = [{ row: 1, column: 0, style: { font: { size: 24 } } }, { row: 3, column: 0, style: { font: { size: 48 } } }] as CellModel[];
  const rows = spreadsheetRowMetrics(1_000_000, cells);
  expect(rows.height(0)).toBe(28);
  expect(rows.height(1)).toBe(50);
  expect(rows.height(3)).toBe(92);
  expect(rows.top(2)).toBe(78);
  expect(rows.top(4)).toBe(198);
  expect(rows.rowAt(77)).toBe(1);
  expect(rows.rowAt(78)).toBe(2);
  expect(rows.rowAt(rows.top(999_999))).toBe(999_999);
  expect(spreadsheetRowMetrics(1200, []).top(1200)).toBe(33600);
});


test("explicit point heights and hidden rows share consistent sparse geometry", () => {
  const rows = spreadsheetRowMetrics(1200, [], [{ row: 0, height: 42, hidden: false }, { row: 1, height: null, hidden: true }, { row: 2, height: 21, hidden: true }]);
  expect(rows.height(0)).toBe(56);
  expect(rows.height(1)).toBe(0);
  expect(rows.top(3)).toBe(56);
  expect(rows.rowAt(56)).toBe(3);
  expect(rows.rowAt(55)).toBe(0);
});
