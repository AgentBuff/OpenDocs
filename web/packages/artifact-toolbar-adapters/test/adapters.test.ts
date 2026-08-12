import { describe, expect, it } from "vitest";
import {
  createMindmapToolbarAdapter,
  createPresentationToolbarAdapter,
  createSpreadsheetToolbarAdapter,
  createWhiteboardToolbarAdapter,
  type SpreadsheetToolbarDescriptor,
} from "../src/index.js";

describe("artifact toolbar adapters", () => {
  it("start with no capabilities instead of exposing placeholder actions", () => {
    expect(createSpreadsheetToolbarAdapter().resolve({
      artifactId: "sheet-1",
      revision: 0,
      availableCapabilities: new Set(),
      selection: null,
    })).toEqual([]);
    expect(createPresentationToolbarAdapter().toolbar).toEqual([]);
    expect(createMindmapToolbarAdapter().toolbar).toEqual([]);
    expect(createWhiteboardToolbarAdapter().toolbar).toEqual([]);
  });

  it("resolves only capabilities advertised by the artifact registry", () => {
    const descriptors: readonly SpreadsheetToolbarDescriptor<"freeze" | "format">[] = [
      { id: "freeze", capability: "spreadsheet.freeze", group: "view", kind: "button", action: "freeze", label: "冻结窗格" },
      { id: "format", capability: "spreadsheet.format", group: "format", kind: "button", action: "format", label: "单元格格式" },
    ];
    const adapter = createSpreadsheetToolbarAdapter(descriptors);
    const resolved = adapter.resolve({
      artifactId: "sheet-1",
      revision: 3,
      availableCapabilities: new Set(["spreadsheet.freeze"]),
      selection: { kind: "cell", sheetId: "s1", row: 2, column: 3 },
    });
    expect(resolved.map((item) => item.id)).toEqual(["freeze"]);
  });

  it("rejects a descriptor from another artifact namespace", () => {
    expect(() => createWhiteboardToolbarAdapter([{
      id: "wrong",
      capability: "presentation.align",
      group: "shape",
      kind: "button",
      action: "wrong",
      label: "错误能力",
    }])).toThrow("whiteboard. namespace");
  });

  it("keeps nested menu capabilities independently visible", () => {
    const adapter = createPresentationToolbarAdapter([{
      id: "shape-menu",
      capability: "presentation.shape-menu",
      group: "shape",
      kind: "menu",
      label: "形状",
      children: [{
        id: "align",
        capability: "presentation.align",
        group: "shape",
        kind: "button",
        action: "align",
        label: "对齐",
      }],
    }]);
    const context = {
      artifactId: "deck-1",
      revision: 1,
      availableCapabilities: new Set(["presentation.shape-menu", "presentation.align"]),
      selection: { kind: "element" as const, slideId: "slide-1", elementIds: ["shape-1"] },
    };
    expect(adapter.resolve(context)[0]?.children?.[0]?.id).toBe("align");
    expect(adapter.resolve({ ...context, availableCapabilities: new Set(["presentation.shape-menu"]) })[0]?.children).toEqual([]);
  });
});
