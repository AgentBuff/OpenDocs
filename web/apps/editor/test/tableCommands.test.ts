import { describe, expect, it } from "vitest";

import { tableCommandSelection } from "../src/blocks/table/commands.js";
import type { TableSelection } from "../src/blocks/table/model.js";

/**
 * Contract under test: table selections travel as stable domain IDs only.
 * A renderer re-render may rebuild every DOM node, so the projected command
 * shape must be derived exclusively from IDs and must be idempotent across
 * repeated projections (I01 acceptance, unit evidence).
 */
describe("table command selection projection", () => {
  const cases: Array<["cell" | "row" | "column" | "range" | "all", TableSelection]> = [
    ["cell", { kind: "cell", rowId: "row-7", cellId: "cell-3" } as TableSelection],
    ["row", { kind: "row", id: "row-2" } as TableSelection],
    ["column", { kind: "column", id: "col-5" } as TableSelection],
    ["range", {
      kind: "range",
      startRowId: "row-1",
      endRowId: "row-4",
      startColumnId: "col-2",
      endColumnId: "col-9",
    } as TableSelection],
    ["all", { kind: "all" } as TableSelection],
  ];

  for (const [kind, selection] of cases) {
    it(`projects a ${kind} selection to its stable-id command shape`, () => {
      const projected = tableCommandSelection(selection);
      expect(projected.kind).toBe(kind);
      expect(JSON.stringify(projected)).not.toContain("Index");
      // Re-render simulation: the same logical selection projects identically
      // no matter how many times the view asks for the command shape.
      expect(tableCommandSelection(selection)).toEqual(projected);
    });
  }

  it("keeps range corners as explicit stable ids, not positional pairs", () => {
    const projected = tableCommandSelection({
      kind: "range",
      startRowId: "row-a",
      endRowId: "row-b",
      startColumnId: "col-a",
      endColumnId: "col-b",
    } as TableSelection);
    expect(projected).toEqual({
      kind: "range",
      startRowId: "row-a",
      endRowId: "row-b",
      startColumnId: "col-a",
      endColumnId: "col-b",
    });
  });
});
