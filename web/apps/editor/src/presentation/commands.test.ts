import { describe, expect, it } from "vitest";

import { createSlideCommand, createTextNode, insertTextNodeCommand, nodeTransformCommand, presentationHistoryCommand, presentationHistoryTransaction, presentationSemanticInputs, textContentCommand } from "./commands.js";

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
    expect(insertTextNodeCommand("slide-1", node, 0).typeId).toBe("presentation.insertNode");
    expect(textContentCommand("slide-1", "node-1", "hello").payload).toMatchObject({
      type: "setTextContent",
      body: { text: "hello", runs: [] },
    });
  });

  it("creates the first slide through the engine command rather than local view state", () => {
    expect(createSlideCommand("slide-1", "0001", 0)).toMatchObject({
      typeId: "presentation.createSlide",
      payload: { type: "createSlide", slide: { id: "slide-1", orderKey: "0001" }, index: 0 },
    });
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
      { type: "ungroupNodes", slideId: "slide-1", groupId: "group-1" },
    ])).toEqual([
      { typeId: "presentation.deleteNode", payload: { type: "deleteNode", slideId: "slide-1", nodeId: "node-1" } },
      { typeId: "presentation.ungroupNodes", payload: { type: "ungroupNodes", slideId: "slide-1", groupId: "group-1" } },
    ]);
  });
});
