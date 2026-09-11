import { describe, expect, it } from "vitest";
import { mindmapBranchColors, nodeThemeColors } from "../src/mindmap/appearance.js";

import type { MindmapModel, MindmapNode } from "@open-office/schema/artifact";
import {
  buildMindmapIndex,
  buildPasteCommands,
  createClipboardPayload,
  selectionRoots,
  renameMindmapContent,
} from "../src/mindmap/model.js";

const style = {
  shape: "roundedRectangle" as const,
  fillColor: null,
  borderColor: null,
  textColor: null,
  borderWidth: 1,
  textAlign: "start" as const,
  minWidth: 96,
  maxWidth: 320,
};
const node = (id: string, parentId: string | null): MindmapNode => ({
  id, parentId, content: { text: id, runs: [] }, style,
  supplement: { note: null, hyperlink: null, image: null, markers: [] },
  attrs: {}, collapsed: false,
});
const model: MindmapModel = {
  settings: { layout: "logicalRight", themeId: null, connector: { shape: "orthogonal", color: null, width: 2, dashed: false } },
  root: "root",
  nodes: [node("root", null), node("a", "root"), node("a1", "a"), node("b", "root")],
  edges: [{ id: "e", sourceId: "a", targetId: "a1", label: null, style: { shape: "curve", color: null, width: 2, dashed: true }, attrs: {} }],
  summaries: [{ id: "summary", startNodeId: "a", endNodeId: "b", content: { text: "结论", runs: [] } }],
  boundaries: [{ id: "boundary", rootNodeId: "a", label: null }],
  formulas: [{ id: "formula", nodeId: "a1", source: "x^2", display: "inline" }],
};

describe("mindmap model helpers", () => {
  it("builds children in one pass and removes redundant selected descendants", () => {
    const index = buildMindmapIndex(model);
    expect(index.children.get("root")?.map((item) => item.id)).toEqual(["a", "b"]);
    expect(selectionRoots(["a", "a1", "b"], index)).toEqual(["a", "b"]);
  });

  it("copies complete subtrees and pastes them as semantic commands", () => {
    const payload = createClipboardPayload(model, ["a"]);
    expect(payload?.nodes.map((item) => item.id)).toEqual(["a", "a1"]);
    expect(payload?.edges).toHaveLength(1);
    expect(payload?.summaries).toHaveLength(0);
    expect(payload?.boundaries).toHaveLength(1);
    expect(payload?.formulas).toHaveLength(1);
    let sequence = 0;
    let entitySequence = 0;
    const pasted = buildPasteCommands(
      payload!,
      "b",
      0,
      () => `new-${sequence++}`,
      () => `new-entity-${entitySequence++}`,
    );
    expect(pasted.rootIds).toEqual(["new-0"]);
    expect(pasted.commands[0]).toMatchObject({
      typeId: "mindmap.addNode",
      payload: { nodeId: "new-0", parentId: "b", index: 0 },
    });
    expect(pasted.commands.some((command) => command.typeId === "mindmap.addEdge")).toBe(true);
    expect(pasted.commands.some((command) => command.typeId === "mindmap.addBoundary")).toBe(true);
    expect(pasted.commands.some((command) => command.typeId === "mindmap.addFormula")).toBe(true);
  });
  it("keeps formatting when renaming a whole topic or appending Unicode text", () => {
    const bold = { bold: true, italic: false, underline: false, strikethrough: false, fontFamily: null, fontSize: null, color: null, highlight: null, verticalAlign: null };
    const content = { text: "原主题", runs: [{ start: 0, end: 3, style: bold }] };
    expect(renameMindmapContent(content, "新的主题😀")).toEqual({ text: "新的主题😀", runs: [{ start: 0, end: 5, style: bold }] });
    expect(renameMindmapContent(content, "原主题😀").runs).toEqual([{ start: 0, end: 4, style: bold }]);
    expect(renameMindmapContent(content, "").runs).toEqual([]);
  });

  it("rebases formatted suffixes without formatting plain-text gaps", () => {
    const italic = { bold: false, italic: true, underline: false, strikethrough: false, fontFamily: null, fontSize: null, color: null, highlight: null, verticalAlign: null };
    const content = { text: "abcXYZ", runs: [{ start: 3, end: 6, style: italic }] };
    expect(renameMindmapContent(content, "a😀bcXYZ").runs).toEqual([{ start: 4, end: 7, style: italic }]);
    expect(renameMindmapContent(content, "aXYZ").runs).toEqual([{ start: 1, end: 4, style: italic }]);
    expect(renameMindmapContent(content, "abcXYZ")).toEqual(content);
  });

  it("inherits branch colors by ancestry even when descendants precede their parents", () => {
    const colors = mindmapBranchColors({ ...model, nodes: [node("a1", "a"), node("root", null), node("a", "root"), node("b", "root")] });
    expect(colors.get("a1")).toBe(colors.get("a"));
    expect(colors.get("a")).not.toBe(colors.get("b"));
    expect(model.nodes[1].style.fillColor).toBeNull();
  });

  it("uses legible light, dark and high-contrast theme defaults", () => {
    expect(nodeThemeColors(0, "#3265d9", "light")).toMatchObject({ fillColor: "#3265d9", textColor: "#ffffff" });
    expect(nodeThemeColors(2, "#3265d9", "dark")).toMatchObject({ fillColor: "#22262e", textColor: "#e3e7ef" });
    expect(nodeThemeColors(1, "#3265d9", "highContrast")).toMatchObject({ fillColor: "#ffffff", textColor: "#111111" });
  });

});
