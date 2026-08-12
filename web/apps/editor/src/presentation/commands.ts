import type { PresentationV5Node, PresentationV5Transform } from "@open-office/schema";
import type { SemanticCommandInput } from "@open-office/sdk";
import type { PresentationSemanticCommand } from "@open-office/presentation-ui";

export type PresentationHistoryAction = "undo" | "redo";

/**
 * Presentation command payloads are intentionally constructed here instead of
 * exposing DOM gestures or a writable Deck to the UI. Keep this module small:
 * every payload maps 1:1 to a Rust PresentationCommand variant.
 */
export function nodeTransformCommand(
  slideId: string,
  nodeId: string,
  transform: PresentationV5Transform,
) {
  return {
    typeId: "presentation.setNodeTransform",
    payload: { type: "setNodeTransform", slideId, nodeId, transform },
  };
}

export function textContentCommand(slideId: string, nodeId: string, text: string) {
  return {
    typeId: "presentation.setTextContent",
    payload: {
      type: "setTextContent",
      slideId,
      nodeId,
      body: { text, runs: [] },
    },
  };
}

export function insertTextNodeCommand(slideId: string, node: PresentationV5Node, index: number) {
  return {
    typeId: "presentation.insertNode",
    payload: { type: "insertNode", slideId, node, index },
  };
}

export function createSlideCommand(slideId: string, orderKey: string, index: number) {
  return {
    typeId: "presentation.createSlide",
    payload: {
      type: "createSlide",
      slide: { id: slideId, orderKey, name: "未命名幻灯片" },
      index,
    },
  };
}

/**
 * History execution is intentionally represented as one semantic command.
 * The caller provides the matching transaction origin (`undo` or `redo`).
 */
export function presentationHistoryCommand(action: PresentationHistoryAction): SemanticCommandInput {
  return {
    typeId: "presentation.history",
    payload: { action },
  };
}

/** The semantic command and transaction origin are an inseparable history intent. */
export function presentationHistoryTransaction(action: PresentationHistoryAction) {
  return {
    origin: action,
    commands: [presentationHistoryCommand(action)],
  } as const;
}

export function createTextNode(id: string, orderKey: string): PresentationV5Node {
  return {
    id,
    parentId: null,
    orderKey,
    name: "文本",
    altText: null,
    layoutPlaceholderId: null,
    transform: { x: 1_200_000, y: 1_200_000, width: 4_800_000, height: 720_000, rotation: 0 },
    visible: true,
    locked: false,
    opacity: 1,
    kind: {
      type: "text",
      data: {
        frame: {
          body: { text: "双击输入文本", runs: [] },
          verticalAlign: "middle",
          padding: { top: 48_000, right: 48_000, bottom: 48_000, left: 48_000 },
          autoFit: "shrinkText",
        },
      },
    },
  };
}

/**
 * The node registry emits an editor-neutral semantic command.  This adapter is
 * the only place that turns it into the transaction record accepted by the
 * server; React components never serialize a Deck mutation by themselves.
 */
export function presentationSemanticInputs(
  commands: readonly PresentationSemanticCommand[],
): SemanticCommandInput[] {
  return commands.map((command) => ({
    typeId: presentationCommandTypeId(command.type),
    payload: command,
  }));
}

function presentationCommandTypeId(type: PresentationSemanticCommand["type"]): string {
  switch (type) {
    case "deleteNode": return "presentation.deleteNode";
    case "ungroupNodes": return "presentation.ungroupNodes";
    case "setNodeTransform": return "presentation.setNodeTransform";
    case "setShapeStyle": return "presentation.setShapeStyle";
    case "setTextContent": return "presentation.setTextContent";
    case "setTextFrame": return "presentation.setTextFrame";
    case "setImageConfig": return "presentation.setImageConfig";
    case "setMediaConfig": return "presentation.setMediaConfig";
  }
}
