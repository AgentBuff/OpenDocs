import { describe, expect, it } from "vitest";
import { resolveMultiSelectionControls, selectionRequirement, selectionSurfaces } from "../src/index.js";
import { plainPresentationRichText, type PresentationV5Node } from "@open-office/schema";

const node = (type: PresentationV5Node["kind"]["type"]): PresentationV5Node => ({
  id: `node-${type}`, parentId: null, orderKey: "a", name: null, altText: null, layoutPlaceholderId: null,
  transform: { x: 0, y: 0, width: 1, height: 1, rotation: 0 }, visible: true, locked: false, opacity: 1,
  kind: type === "text"
    ? { type, data: { frame: { body: plainPresentationRichText(""), verticalAlign: "top", padding: { top: 0, right: 0, bottom: 0, left: 0 }, autoFit: "none" } } }
    : type === "shape"
      ? { type, data: { geometry: "rectangle", style: { fill: { type: "none" }, stroke: null } } }
      : { type: "image", data: { assetId: "asset", originalAssetId: null, crop: { top: 0, right: 0, bottom: 0, left: 0 }, flipH: false, flipV: false, caption: null } },
});

describe("Presentation selection derivation", () => {
  it("keeps slide controls separate from object controls", () => {
    const snapshot = { slideId: "slide-1", selectedNodes: [], textEditingNodeId: null } as const;
    expect(selectionRequirement(snapshot)).toBe("slide");
    expect(selectionSurfaces(snapshot)).toEqual(["global", "insert", "slide", "timeline"]);
  });

  it("uses an object-specific surface for single selection and node-only surface for multi-select", () => {
    const image = node("image");
    expect(selectionSurfaces({ slideId: "slide-1", selectedNodes: [image], textEditingNodeId: null }))
      .toEqual(["global", "node", "image"]);
    expect(selectionSurfaces({ slideId: "slide-1", selectedNodes: [image, node("shape")], textEditingNodeId: null }))
      .toEqual(["global", "node"]);
  });

  it("gives text editing precedence over ordinary node selection", () => {
    const text = node("text");
    const snapshot = { slideId: "slide-1", selectedNodes: [text], textEditingNodeId: text.id };
    expect(selectionRequirement(snapshot)).toBe("text-editing");
    expect(selectionSurfaces(snapshot)).toEqual(["global", "text"]);
  });

  it("only exposes multi-object controls after the server grants each semantic capability", () => {
    expect(resolveMultiSelectionControls(new Set(["presentation.groupNodes"]), 2).map((control) => control.action))
      .toEqual(["selection.group"]);
    const alignOnly = resolveMultiSelectionControls(new Set(["presentation.alignNodes"]), 2);
    expect(alignOnly.map((control) => control.action)).toEqual([
      "selection.alignLeft", "selection.alignCenter", "selection.alignRight",
      "selection.alignTop", "selection.alignMiddle", "selection.alignBottom",
    ]);
    const all = resolveMultiSelectionControls(new Set(["presentation.alignNodes", "presentation.distributeNodes"]), 3);
    expect(all).toHaveLength(8);
  });

  it("adds layer ordering only for a compatible, unlocked sibling selection", () => {
    const first = node("shape");
    const second = { ...node("image"), id: "node-image", parentId: null };
    const available = new Set(["presentation.groupNodes", "presentation.alignNodes", "presentation.distributeNodes", "presentation.reorderNode"]);
    expect(resolveMultiSelectionControls(available, [first, second]).map((control) => control.action)).toEqual([
      "selection.group",
      "selection.alignLeft", "selection.alignCenter", "selection.alignRight",
      "selection.alignTop", "selection.alignMiddle", "selection.alignBottom",
      "selection.bringForward", "selection.sendBackward", "selection.bringToFront", "selection.sendToBack",
    ]);
    expect(resolveMultiSelectionControls(available, [{ ...first, locked: true }, second])).toEqual([]);
    expect(resolveMultiSelectionControls(available, [first, { ...second, parentId: "group-1" }])).toEqual([]);
  });
});
