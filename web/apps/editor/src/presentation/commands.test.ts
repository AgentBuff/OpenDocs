import { describe, expect, it } from "vitest";

import { createChartNode, createConnectorNode, createImageNode, createLayoutCommand, createMasterCommand, createRectangleNode, createShapeNode, createSlideCommand, createTextNode, deckPageSpecCommand, deckThemeCommand, deleteAnimationCommand, deleteLayoutCommand, deleteMasterCommand, deleteSlideCommand, duplicatePresentationNode, duplicateSlideCommand, groupNodesCommand, insertNodeCommand, moveAnimationCommand, moveSlideCommand, multiNodeArrangeCommands, nodeTransformCommand, presentationHistoryCommand, presentationHistoryTransaction, presentationSemanticInputs, registerPresentationAssetCommand, slideBackgroundCommand, slideLayoutCommand, slideNotesCommand, slideTransitionCommand, tableStructureCommand, textContentCommand, updateLayoutCommand, updateMasterCommand, upsertAnimationCommand } from "./commands.js";

describe("Presentation semantic UI commands", () => {
  it("does not serialize a DOM gesture or a writable deck when moving a node", () => {
    expect(nodeTransformCommand("slide-1", "node-1", { x: 1, y: 2, width: 3, height: 4, rotation: 0 })).toEqual({
      typeId: "presentation.setNodeTransform",
      payload: {
        type: "setNodeTransform",
        slideId: "slide-1",
        nodeId: "node-1",
        transform: { x: 1, y: 2, width: 3, height: 4, rotation: 0 },
      },
    });
  });

  it("creates a typed text node and submits text through the semantic command", () => {
    const node = createTextNode("node-1", "0001");
    expect(node.kind.type).toBe("text");
    expect(insertNodeCommand("slide-1", node, 0).typeId).toBe("presentation.insertNode");
    expect(insertNodeCommand("slide-1", createRectangleNode("shape-1", "0002"), 1).payload).toMatchObject({
      type: "insertNode",
      node: { kind: { type: "shape", data: { geometry: "rectangle" } } },
    });
    expect(textContentCommand("slide-1", "node-1", "hello").payload).toMatchObject({
      type: "setTextContent",
      body: { text: "hello", runs: [], paragraphs: [{ start: 0, end: 5 }] },
    });
  });

  it("creates a connector with strict paired free endpoints", () => {
    const connector = createConnectorNode("connector-1", "0003");
    expect(connector).toMatchObject({
      kind: {
        type: "connector",
        data: {
          start: { type: "free", value: { x: 2_000_000, y: 2_000_000 } },
          end: { type: "free", value: { x: 5_000_000, y: 3_200_000 } },
        },
      },
    });
    expect(insertNodeCommand("slide-1", connector, 0).payload).toMatchObject({ type: "insertNode", node: { kind: { type: "connector" } } });
  });

  it("registers a verified asset in the same semantic batch as an image node", () => {
    const asset = { assetId: "asset-1", digest: "sha256:abc", mimeType: "image/png", width: 400, height: 200, originalAssetId: null };
    expect(registerPresentationAssetCommand(asset)).toEqual({
      typeId: "presentation.registerAsset",
      payload: { type: "registerAsset", asset },
    });
    expect(createImageNode("image-1", "0003", asset)).toMatchObject({
      kind: { type: "image", data: { assetId: "asset-1" } },
      transform: { width: 4_000_000, height: 2_000_000 },
    });
  });

  it("creates only schema-defined shape geometries with line-safe defaults", () => {
    const arrow = createShapeNode("arrow-1", "0004", "arrow");
    expect(arrow.kind).toMatchObject({ type: "shape", data: { geometry: "arrow", style: { fill: { type: "none" }, stroke: { width: 2 } } } });
    expect(arrow.transform.height).toBeGreaterThan(0);
    expect(createShapeNode("ellipse-1", "0005", "ellipse")).toMatchObject({ name: "圆形", kind: { data: { geometry: "ellipse" } } });
  });

  it("serializes shape primitive changes as an explicit command", () => {
    expect(presentationSemanticInputs([{ type: "setShapeGeometry", slideId: "slide-1", nodeId: "shape-1", geometry: "ellipse" }])).toEqual([
      { typeId: "presentation.setShapeGeometry", payload: { type: "setShapeGeometry", slideId: "slide-1", nodeId: "shape-1", geometry: "ellipse" } },
    ]);
  });

  it("serializes one complete ChartSpec instead of a chart JSON patch", () => {
    const spec = { chartType: "line" as const, title: "预测", categories: ["Q1", "Q2"], series: [{ name: "营收", values: [10, 20], color: null }] };
    expect(presentationSemanticInputs([{ type: "setChartSpec", slideId: "slide-1", nodeId: "chart-1", spec }])).toEqual([
      { typeId: "presentation.setChartSpec", payload: { type: "setChartSpec", slideId: "slide-1", nodeId: "chart-1", spec } },
    ]);
  });

  it("creates a schema-defined chart node with complete initial data", () => {
    const chart = createChartNode("chart-1", "0005");
    expect(chart).toMatchObject({
      kind: { type: "chart", data: { spec: { chartType: "column", categories: ["类别 1", "类别 2", "类别 3"], series: [{ values: [42, 68, 54] }] } } },
    });
    expect(insertNodeCommand("slide-1", chart, 0).typeId).toBe("presentation.insertNode");
  });

  it("serializes both connector endpoints as one atomic command", () => {
    const start = { type: "node" as const, value: { nodeId: "shape-1", anchor: "right" as const } };
    const end = { type: "free" as const, value: { x: 320, y: 180 } };
    expect(presentationSemanticInputs([{ type: "setConnectorEndpoints", slideId: "slide-1", nodeId: "connector-1", start, end }])).toEqual([
      { typeId: "presentation.setConnectorEndpoints", payload: { type: "setConnectorEndpoints", slideId: "slide-1", nodeId: "connector-1", start, end } },
    ]);
  });

  it("serializes table structure as concrete protocol commands rather than a table patch", () => {
    expect(tableStructureCommand("mergeTableCells", "slide-1", "table-1", { start: { row: 0, column: 0 }, end: { row: 1, column: 1 } })).toEqual({
      typeId: "presentation.mergeTableCells",
      payload: { type: "mergeTableCells", slideId: "slide-1", nodeId: "table-1", start: { row: 0, column: 0 }, end: { row: 1, column: 1 } },
    });
  });

  it("derives multi-object layer commands from immutable sibling order", () => {
    const nodes = [
      { ...createTextNode("a", "0001"), orderKey: "0001" },
      { ...createTextNode("b", "0002"), orderKey: "0002" },
      { ...createTextNode("c", "0003"), orderKey: "0003" },
      { ...createTextNode("d", "0004"), orderKey: "0004" },
    ];
    expect(multiNodeArrangeCommands("slide-1", nodes, ["a", "b"], "selection.bringForward")).toEqual([
      { type: "reorderNode", slideId: "slide-1", nodeId: "b", index: 2 },
      { type: "reorderNode", slideId: "slide-1", nodeId: "a", index: 1 },
    ]);
    expect(multiNodeArrangeCommands("slide-1", nodes, ["c", "d"], "selection.sendToBack")).toEqual([
      { type: "reorderNode", slideId: "slide-1", nodeId: "d", index: 0 },
      { type: "reorderNode", slideId: "slide-1", nodeId: "c", index: 0 },
    ]);
    expect(multiNodeArrangeCommands("slide-1", nodes, ["a", "b"], "selection.alignLeft")).toEqual([
      { type: "alignNodes", slideId: "slide-1", nodeIds: ["a", "b"], alignment: "left" },
    ]);
    expect(multiNodeArrangeCommands("slide-1", nodes, ["a", "b"], "selection.distributeHorizontal")).toEqual([]);
    expect(multiNodeArrangeCommands("slide-1", [{ ...nodes[0], locked: true }, ...nodes.slice(1)], ["a", "b"], "selection.bringToFront")).toEqual([]);
  });

  it("builds a group from compatible sibling bounds and stable ids", () => {
    const first = { ...createTextNode("a", "0001"), transform: { x: 10, y: 20, width: 30, height: 40, rotation: 0 } };
    const second = { ...createTextNode("b", "0002"), transform: { x: 60, y: 50, width: 20, height: 10, rotation: 0 } };
    expect(groupNodesCommand("slide-1", [first, second], ["a", "b"], "group-1", "0003")).toEqual({
      type: "groupNodes",
      slideId: "slide-1",
      childIds: ["a", "b"],
      index: 0,
      group: expect.objectContaining({
        id: "group-1",
        parentId: null,
        kind: { type: "group", data: {} },
        transform: { x: 10, y: 20, width: 70, height: 40, rotation: 0 },
      }),
    });
    expect(groupNodesCommand("slide-1", [{ ...first, locked: true }, second], ["a", "b"], "group-1", "0003")).toBeNull();
    expect(groupNodesCommand("slide-1", [{ ...first, kind: { type: "connector", data: { start: { type: "free", value: { x: 0, y: 0 } }, end: { type: "free", value: { x: 1, y: 1 } } } } }, second], ["a", "b"], "group-1", "0003")).toBeNull();
  });

  it("creates the first slide through the engine command rather than local view state", () => {
    expect(createSlideCommand("slide-1", "0001", 0)).toMatchObject({
      typeId: "presentation.createSlide",
      payload: { type: "createSlide", slide: { id: "slide-1", orderKey: "0001" }, index: 0 },
    });
    expect(moveSlideCommand("slide-1", 2)).toEqual({ typeId: "presentation.moveSlide", payload: { type: "moveSlide", slideId: "slide-1", index: 2 } });
    expect(duplicateSlideCommand("slide-1", "slide-2", "0002", "副本", [{ sourceId: "node-1", targetId: "node-2" }], [{ sourceId: "animation-1", targetId: "animation-2" }], 1)).toEqual({
      typeId: "presentation.duplicateSlide",
      payload: {
        type: "duplicateSlide", sourceSlideId: "slide-1", slideId: "slide-2", orderKey: "0002", name: "副本",
        nodeIdMap: [{ sourceId: "node-1", targetId: "node-2" }], animationIdMap: [{ sourceId: "animation-1", targetId: "animation-2" }], index: 1,
      },
    });
    expect(deleteSlideCommand("slide-1")).toEqual({ typeId: "presentation.deleteSlide", payload: { type: "deleteSlide", slideId: "slide-1" } });
  });

  it("keeps slide properties as narrow semantic commands", () => {
    expect(slideNotesCommand("slide-1", "speaker note")).toEqual({
      typeId: "presentation.setSlideNotes",
      payload: { type: "setSlideNotes", slideId: "slide-1", notes: "speaker note" },
    });
    expect(slideBackgroundCommand("slide-1", { type: "none" })).toEqual({
      typeId: "presentation.setSlideBackground",
      payload: { type: "setSlideBackground", slideId: "slide-1", background: { type: "none" } },
    });
    expect(slideTransitionCommand("slide-1", { kind: "fade", durationMs: 300 })).toEqual({
      typeId: "presentation.setSlideTransition",
      payload: { type: "setSlideTransition", slideId: "slide-1", transition: { kind: "fade", durationMs: 300 } },
    });
    expect(slideLayoutCommand("slide-1", "layout-title")).toEqual({
      typeId: "presentation.setSlideLayout",
      payload: { type: "setSlideLayout", slideId: "slide-1", layoutId: "layout-title" },
    });
    expect(slideLayoutCommand("slide-1", null).payload).toMatchObject({ layoutId: null });
  });

  it("addresses one timeline entry without replacing the whole slide timeline", () => {
    const animation = { id: "fade-1", targetNodeId: "node-1", trigger: "onClick" as const, preset: "fade" as const, durationMs: 300, delayMs: 0, orderKey: "0001" };
    expect(upsertAnimationCommand("slide-1", animation)).toEqual({ typeId: "presentation.upsertAnimation", payload: { type: "upsertAnimation", slideId: "slide-1", animation } });
    expect(deleteAnimationCommand("slide-1", "fade-1")).toEqual({ typeId: "presentation.deleteAnimation", payload: { type: "deleteAnimation", slideId: "slide-1", animationId: "fade-1" } });
    expect(moveAnimationCommand("slide-1", "fade-1", 2)).toEqual({ typeId: "presentation.moveAnimation", payload: { type: "moveAnimation", slideId: "slide-1", animationId: "fade-1", index: 2 } });
  });

  it("keeps deck design settings independent from slide projections", () => {
    expect(deckPageSpecCommand({ width: 12_192_000, height: 6_858_000, unit: "emu", safeArea: null })).toMatchObject({
      typeId: "presentation.setPageSpec",
      payload: { type: "setPageSpec", pageSpec: { unit: "emu" } },
    });
    expect(deckThemeCommand({ id: "open-docs", name: "OpenDocs" })).toEqual({
      typeId: "presentation.setTheme",
      payload: { type: "setTheme", theme: { id: "open-docs", name: "OpenDocs" } },
    });
  });

  it("serializes complete master and layout entities without a generic patch", () => {
    const master = { id: "master-1", name: "默认", background: { type: "none" as const }, placeholders: [] };
    const layout = { id: "layout-1", masterId: "master-1", name: "标题页", placeholders: [] };
    expect(createMasterCommand(master)).toEqual({ typeId: "presentation.createMaster", payload: { type: "createMaster", master } });
    expect(updateMasterCommand(master)).toEqual({ typeId: "presentation.updateMaster", payload: { type: "updateMaster", master } });
    expect(deleteMasterCommand("master-1")).toEqual({ typeId: "presentation.deleteMaster", payload: { type: "deleteMaster", masterId: "master-1" } });
    expect(createLayoutCommand(layout)).toEqual({ typeId: "presentation.createLayout", payload: { type: "createLayout", layout } });
    expect(updateLayoutCommand(layout)).toEqual({ typeId: "presentation.updateLayout", payload: { type: "updateLayout", layout } });
    expect(deleteLayoutCommand("layout-1")).toEqual({ typeId: "presentation.deleteLayout", payload: { type: "deleteLayout", layoutId: "layout-1" } });
  });

  it("duplicates into a new canonical node identity without mutating its source", () => {
    const original = createTextNode("node-1", "0001");
    const duplicate = duplicatePresentationNode(original, "node-2", "0002");
    expect(duplicate).toMatchObject({ id: "node-2", orderKey: "0002", parentId: null, name: "文本 副本" });
    expect(duplicate.transform).toMatchObject({ x: original.transform.x + 180_000, y: original.transform.y + 180_000 });
    expect(original.id).toBe("node-1");
  });

  it("represents undo and redo as the single presentation history intent", () => {
    expect(presentationHistoryCommand("undo")).toEqual({
      typeId: "presentation.history",
      payload: { action: "undo" },
    });
    expect(presentationHistoryCommand("redo")).toEqual({
      typeId: "presentation.history",
      payload: { action: "redo" },
    });
    expect(presentationHistoryTransaction("undo")).toEqual({
      origin: "undo",
      commands: [{ typeId: "presentation.history", payload: { action: "undo" } }],
    });
    expect(presentationHistoryTransaction("redo")).toMatchObject({ origin: "redo" });
  });

  it("adapts node-registry intents to real server command type ids", () => {
    expect(presentationSemanticInputs([
      { type: "deleteNode", slideId: "slide-1", nodeId: "node-1" },
      { type: "groupNodes", slideId: "slide-1", group: createTextNode("group-1", "0002"), childIds: ["node-1", "node-2"], index: 0 },
      { type: "ungroupNodes", slideId: "slide-1", groupId: "group-1" },
    ])).toEqual([
      { typeId: "presentation.deleteNode", payload: { type: "deleteNode", slideId: "slide-1", nodeId: "node-1" } },
      { typeId: "presentation.groupNodes", payload: { type: "groupNodes", slideId: "slide-1", group: createTextNode("group-1", "0002"), childIds: ["node-1", "node-2"], index: 0 } },
      { typeId: "presentation.ungroupNodes", payload: { type: "ungroupNodes", slideId: "slide-1", groupId: "group-1" } },
    ]);
  });
});
