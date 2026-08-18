import { describe, expect, it } from "vitest";
import {
  PRESENTATION_CAPABILITY_MATRIX,
  capabilitiesForSurface,
  presentationCapability,
} from "../src/index.js";

describe("Presentation control-plane capability matrix", () => {
  it("contains each implemented Presentation transaction type exactly once", () => {
    const expected = [
      "presentation.history", "presentation.setPageSpec", "presentation.setTheme",
      "presentation.createMaster", "presentation.updateMaster", "presentation.deleteMaster",
      "presentation.createLayout", "presentation.updateLayout", "presentation.deleteLayout",
      "presentation.createSlide", "presentation.deleteSlide", "presentation.moveSlide",
      "presentation.setSlideLayout", "presentation.setSlideBackground", "presentation.setSlideNotes",
      "presentation.setSlideTransition", "presentation.upsertAnimation", "presentation.deleteAnimation",
      "presentation.moveAnimation", "presentation.registerAsset", "presentation.insertNode", "presentation.deleteNode",
      "presentation.moveNode", "presentation.reorderNode", "presentation.groupNodes",
      "presentation.ungroupNodes", "presentation.setNodeTransform", "presentation.setNodeLocked", "presentation.alignNodes", "presentation.distributeNodes", "presentation.setShapeStyle", "presentation.setShapeGeometry", "presentation.setChartSpec", "presentation.setConnectorEndpoints", "presentation.setTableCellContent", "presentation.setTableCellStyle", "presentation.insertTableRows", "presentation.insertTableColumns", "presentation.deleteTableRow", "presentation.deleteTableColumn", "presentation.mergeTableCells", "presentation.splitTableCell",
      "presentation.setTextContent", "presentation.setTextFrame", "presentation.setImageConfig",
      "presentation.setMediaConfig",
    ];
    expect(PRESENTATION_CAPABILITY_MATRIX.map((item) => item.typeId)).toEqual(expected);
    expect(new Set(expected).size).toBe(expected.length);
  });

  it("filters controls by server grant, surface and selection context", () => {
    const available = new Set([
      "presentation.createSlide",
      "presentation.insertNode",
      "presentation.setImageConfig",
    ]);
    expect(capabilitiesForSurface("insert", available, "slide").map((item) => item.typeId))
      .toEqual(["presentation.insertNode"]);
    expect(capabilitiesForSurface("image", available, "single-node").map((item) => item.typeId))
      .toEqual(["presentation.setImageConfig"]);
    expect(capabilitiesForSurface("shape", available, "single-node")).toEqual([]);
  });

  it("does not let planned capabilities masquerade as executable UI", () => {
    expect(presentationCapability("presentation.extractImageText")).toBeUndefined();
  });
});
