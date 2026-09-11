import {
  plainPresentationRichText,
  type
  PresentationV5Node,
  ConnectorEndpoint,
  PresentationV5Deck,
  PresentationV5Layout,
  PresentationV5Master,
  PresentationV5Asset,
  PresentationV5ChartSpec,
  PresentationV5SlideBackground,
  PresentationV5SlideTransition,
  PresentationV5TimelineEntry,
  PresentationV5Transform,
} from "@open-office/schema";
import type { SemanticCommandInput } from "@open-office/sdk";
import type { PresentationMultiSelectionAction, PresentationSemanticCommand } from "@open-office/presentation-ui";

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

/**
 * Produces the one canonical batch for a multi-object arrange gesture. The
 * renderer supplies only selected ids and the immutable slide projection;
 * sibling ordering is derived here and persisted exclusively as concrete
 * `alignNodes`, `distributeNodes`, or `reorderNode` intents.
 */
export function multiNodeArrangeCommands(
  slideId: string,
  slideNodes: readonly PresentationV5Node[],
  selectedNodeIds: readonly string[],
  action: PresentationMultiSelectionAction,
): readonly PresentationSemanticCommand[] {
  const selectedIds = new Set(selectedNodeIds);
  const selected = slideNodes.filter((node) => selectedIds.has(node.id));
  if (selected.length !== selectedIds.size || selected.length < 2) return [];
  const parentId = selected[0]?.parentId ?? null;
  if (selected.some((node) => node.locked || node.parentId !== parentId || node.kind.type === "extension")) return [];

  if (action === "selection.alignLeft" || action === "selection.alignCenter" || action === "selection.alignRight"
    || action === "selection.alignTop" || action === "selection.alignMiddle" || action === "selection.alignBottom") {
    const alignment = action === "selection.alignLeft" ? "left"
      : action === "selection.alignCenter" ? "center"
        : action === "selection.alignRight" ? "right"
          : action === "selection.alignTop" ? "top"
            : action === "selection.alignMiddle" ? "middle"
              : "bottom";
    return [{ type: "alignNodes", slideId, nodeIds: [...selectedNodeIds], alignment }];
  }
  if (action === "selection.distributeHorizontal" || action === "selection.distributeVertical") {
    if (selected.length < 3) return [];
    return [{ type: "distributeNodes", slideId, nodeIds: [...selectedNodeIds], axis: action === "selection.distributeHorizontal" ? "horizontal" : "vertical" }];
  }

  const siblings = slideNodes
    .filter((node) => node.parentId === parentId)
    .sort((left, right) => left.orderKey.localeCompare(right.orderKey));
  const working = [...siblings];
  const commands: PresentationSemanticCommand[] = [];
  const initialSelected = siblings.filter((node) => selectedIds.has(node.id));
  const move = (nodeId: string, index: number) => {
    const current = working.findIndex((node) => node.id === nodeId);
    if (current < 0 || current === index) return;
    const [node] = working.splice(current, 1);
    if (!node) return;
    working.splice(index, 0, node);
    commands.push({ type: "reorderNode", slideId, nodeId, index });
  };
  if (action === "selection.bringForward") {
    for (const node of [...initialSelected].reverse()) {
      const index = working.findIndex((candidate) => candidate.id === node.id);
      if (index >= 0 && index < working.length - 1 && !selectedIds.has(working[index + 1]?.id ?? "")) move(node.id, index + 1);
    }
  } else if (action === "selection.sendBackward") {
    for (const node of initialSelected) {
      const index = working.findIndex((candidate) => candidate.id === node.id);
      if (index > 0 && !selectedIds.has(working[index - 1]?.id ?? "")) move(node.id, index - 1);
    }
  } else if (action === "selection.bringToFront") {
    for (const node of initialSelected) {
      const index = working.findIndex((candidate) => candidate.id === node.id);
      if (index >= 0 && working.slice(index + 1).some((candidate) => !selectedIds.has(candidate.id))) move(node.id, working.length - 1);
    }
  } else if (action === "selection.sendToBack") {
    for (const node of [...initialSelected].reverse()) {
      const index = working.findIndex((candidate) => candidate.id === node.id);
      if (index > 0 && working.slice(0, index).some((candidate) => !selectedIds.has(candidate.id))) move(node.id, 0);
    }
  }
  return commands;
}

export function groupNodesCommand(
  slideId: string,
  slideNodes: readonly PresentationV5Node[],
  selectedNodeIds: readonly string[],
  groupId: string,
  orderKey: string,
): PresentationSemanticCommand | null {
  const selectedIds = new Set(selectedNodeIds);
  const selected = slideNodes.filter((node) => selectedIds.has(node.id));
  if (selected.length !== selectedIds.size || selected.length < 2) return null;
  const parentId = selected[0]?.parentId ?? null;
  if (selected.some((node) => node.locked || node.parentId !== parentId || node.kind.type === "extension" || node.kind.type === "connector")) return null;
  const left = Math.min(...selected.map((node) => node.transform.x));
  const top = Math.min(...selected.map((node) => node.transform.y));
  const right = Math.max(...selected.map((node) => node.transform.x + node.transform.width));
  const bottom = Math.max(...selected.map((node) => node.transform.y + node.transform.height));
  const siblings = slideNodes.filter((node) => node.parentId === parentId).sort((a, b) => a.orderKey.localeCompare(b.orderKey));
  const index = Math.min(...selected.map((node) => siblings.findIndex((candidate) => candidate.id === node.id)));
  const group: PresentationV5Node = {
    id: groupId,
    parentId,
    orderKey,
    name: "组合",
    altText: null,
    layoutPlaceholderId: null,
    transform: { x: left, y: top, width: right - left, height: bottom - top, rotation: 0 },
    visible: true,
    locked: false,
    opacity: 1,
    kind: { type: "group", data: {} },
  };
  return { type: "groupNodes", slideId, group, childIds: [...selectedNodeIds], index };
}

export function textContentCommand(slideId: string, nodeId: string, text: string) {
  return {
    typeId: "presentation.setTextContent",
    payload: {
      type: "setTextContent",
      slideId,
      nodeId,
      body: plainPresentationRichText(text),
    },
  };
}

/** Table cells are addressed by their schema anchors, not DOM coordinates. */
export function tableCellContentCommand(slideId: string, nodeId: string, row: number, column: number, text: string): SemanticCommandInput {
  return {
    typeId: "presentation.setTableCellContent",
    payload: { type: "setTableCellContent", slideId, nodeId, row, column, content: { text, runs: [] } },
  };
}

/** Applies a complete cell style to explicit anchor identities in one atomic batch command. */
export function tableCellStyleCommand(
  slideId: string,
  nodeId: string,
  cells: { row: number; column: number }[],
  style: Extract<PresentationV5Node["kind"], { type: "table" }>["data"]["cells"][number]["style"],
): SemanticCommandInput {
  return { typeId: "presentation.setTableCellStyle", payload: { type: "setTableCellStyle", slideId, nodeId, cells, style } };
}

/** Structural table operations remain explicit protocol intents; callers never submit a table patch. */
export function tableStructureCommand(
  type: "insertTableRows" | "insertTableColumns" | "deleteTableRow" | "deleteTableColumn" | "mergeTableCells" | "splitTableCell",
  slideId: string,
  nodeId: string,
  value: { index: number; count: number } | { index: number } | { start: { row: number; column: number }; end: { row: number; column: number } } | { row: number; column: number },
): SemanticCommandInput {
  const typeId: Record<typeof type, string> = {
    insertTableRows: "presentation.insertTableRows",
    insertTableColumns: "presentation.insertTableColumns",
    deleteTableRow: "presentation.deleteTableRow",
    deleteTableColumn: "presentation.deleteTableColumn",
    mergeTableCells: "presentation.mergeTableCells",
    splitTableCell: "presentation.splitTableCell",
  };
  return { typeId: typeId[type], payload: { type, slideId, nodeId, ...value } };
}

/** Inserts any strict v5 node. The node factory decides its semantic kind. */
export function insertNodeCommand(slideId: string, node: PresentationV5Node, index: number) {
  return {
    typeId: "presentation.insertNode",
    payload: { type: "insertNode", slideId, node, index },
  };
}

/** Registers the verified binary reference before a node can reference it. */
export function registerPresentationAssetCommand(asset: PresentationV5Asset): SemanticCommandInput {
  return { typeId: "presentation.registerAsset", payload: { type: "registerAsset", asset } };
}

/** A typed image node. Uploading bytes is not enough: call registerAsset in the same batch. */
export function createImageNode(id: string, orderKey: string, asset: PresentationV5Asset): PresentationV5Node {
  const aspect = asset.width && asset.height ? asset.width / asset.height : 4 / 3;
  const width = 4_000_000;
  const height = Math.max(720_000, Math.round(width / aspect));
  return {
    id,
    parentId: null,
    orderKey,
    name: "图片",
    altText: null,
    layoutPlaceholderId: null,
    transform: { x: 1_800_000, y: 1_500_000, width, height, rotation: 0 },
    visible: true,
    locked: false,
    opacity: 1,
    kind: {
      type: "image",
      data: {
        assetId: asset.assetId,
        originalAssetId: asset.originalAssetId,
        crop: { left: 0, top: 0, right: 0, bottom: 0 },
        flipH: false,
        flipV: false,
        caption: null,
      },
    },
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

export function deleteSlideCommand(slideId: string): SemanticCommandInput {
  return { typeId: "presentation.deleteSlide", payload: { type: "deleteSlide", slideId } };
}

export function moveSlideCommand(slideId: string, index: number): SemanticCommandInput {
  return { typeId: "presentation.moveSlide", payload: { type: "moveSlide", slideId, index } };
}

/**
 * Clones a slide strictly inside the canonical engine. Callers must allocate
 * every target id, so the command is deterministic and no UI copies a Deck.
 */
export function duplicateSlideCommand(
  sourceSlideId: string,
  slideId: string,
  orderKey: string,
  name: string,
  nodeIdMap: ReadonlyArray<{ sourceId: string; targetId: string }>,
  animationIdMap: ReadonlyArray<{ sourceId: string; targetId: string }>,
  index: number,
): SemanticCommandInput {
  return {
    typeId: "presentation.duplicateSlide",
    payload: {
      type: "duplicateSlide",
      sourceSlideId,
      slideId,
      orderKey,
      name,
      nodeIdMap,
      animationIdMap,
      index,
    },
  };
}

/** Slide properties have their own narrow commands; no UI surface writes a slide projection back wholesale. */
export function slideNotesCommand(slideId: string, notes: string | null): SemanticCommandInput {
  return { typeId: "presentation.setSlideNotes", payload: { type: "setSlideNotes", slideId, notes } };
}

export function slideBackgroundCommand(slideId: string, background: PresentationV5SlideBackground): SemanticCommandInput {
  return { typeId: "presentation.setSlideBackground", payload: { type: "setSlideBackground", slideId, background } };
}

export function slideTransitionCommand(slideId: string, transition: PresentationV5SlideTransition | null): SemanticCommandInput {
  return { typeId: "presentation.setSlideTransition", payload: { type: "setSlideTransition", slideId, transition } };
}

/** Attaches one slide to an existing canonical layout, without copying master data into the UI. */
export function slideLayoutCommand(slideId: string, layoutId: string | null): SemanticCommandInput {
  return { typeId: "presentation.setSlideLayout", payload: { type: "setSlideLayout", slideId, layoutId } };
}

/** Timeline entries are independently addressable semantic records, not a slide patch. */
export function upsertAnimationCommand(slideId: string, animation: PresentationV5TimelineEntry): SemanticCommandInput {
  return { typeId: "presentation.upsertAnimation", payload: { type: "upsertAnimation", slideId, animation } };
}

export function deleteAnimationCommand(slideId: string, animationId: string): SemanticCommandInput {
  return { typeId: "presentation.deleteAnimation", payload: { type: "deleteAnimation", slideId, animationId } };
}

/**
 * Changes the order of one stable animation entry.  The engine owns order-key
 * normalization; the Studio only expresses the intended list index.
 */
export function moveAnimationCommand(slideId: string, animationId: string, index: number): SemanticCommandInput {
  return { typeId: "presentation.moveAnimation", payload: { type: "moveAnimation", slideId, animationId, index } };
}

export function deckPageSpecCommand(pageSpec: PresentationV5Deck["pageSpec"]): SemanticCommandInput {
  return { typeId: "presentation.setPageSpec", payload: { type: "setPageSpec", pageSpec } };
}

/** Master and layout entities are complete typed records, never a generic property patch. */
export function createMasterCommand(master: PresentationV5Master): SemanticCommandInput {
  return { typeId: "presentation.createMaster", payload: { type: "createMaster", master } };
}
export function updateMasterCommand(master: PresentationV5Master): SemanticCommandInput {
  return { typeId: "presentation.updateMaster", payload: { type: "updateMaster", master } };
}
export function deleteMasterCommand(masterId: string): SemanticCommandInput {
  return { typeId: "presentation.deleteMaster", payload: { type: "deleteMaster", masterId } };
}
export function createLayoutCommand(layout: PresentationV5Layout): SemanticCommandInput {
  return { typeId: "presentation.createLayout", payload: { type: "createLayout", layout } };
}
export function updateLayoutCommand(layout: PresentationV5Layout): SemanticCommandInput {
  return { typeId: "presentation.updateLayout", payload: { type: "updateLayout", layout } };
}
export function deleteLayoutCommand(layoutId: string): SemanticCommandInput {
  return { typeId: "presentation.deleteLayout", payload: { type: "deleteLayout", layoutId } };
}

export function deckThemeCommand(theme: PresentationV5Deck["theme"]): SemanticCommandInput {
  return { typeId: "presentation.setTheme", payload: { type: "setTheme", theme } };
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
          body: plainPresentationRichText("双击输入文本"),
          verticalAlign: "middle",
          padding: { top: 48_000, right: 48_000, bottom: 48_000, left: 48_000 },
          autoFit: "shrinkText",
        },
      },
    },
  };
}

/** A deliberately useful default chart. Its data remains a schema-validated
 * ChartSpec and is never inferred from presentation renderer state. */
export function createChartNode(id: string, orderKey: string): PresentationV5Node {
  const spec: PresentationV5ChartSpec = {
    chartType: "column",
    title: "图表标题",
    categories: ["类别 1", "类别 2", "类别 3"],
    series: [{ name: "系列 1", values: [42, 68, 54], color: null }],
  };
  return {
    id,
    parentId: null,
    orderKey,
    name: "图表",
    altText: null,
    layoutPlaceholderId: null,
    transform: { x: 1_500_000, y: 1_400_000, width: 4_800_000, height: 3_000_000, rotation: 0 },
    visible: true,
    locked: false,
    opacity: 1,
    kind: { type: "chart", data: { spec } },
  };
}

/** A typed built-in shape; UI never serializes anonymous SceneElement attrs. */
export type BuiltinShapeGeometry = Extract<PresentationV5Node["kind"], { type: "shape" }>["data"]["geometry"];

/** Creates a built-in schema geometry; the UI never sends arbitrary SVG paths. */
export function createShapeNode(id: string, orderKey: string, geometry: BuiltinShapeGeometry): PresentationV5Node {
  const isLine = geometry === "line" || geometry === "arrow";
  return {
    id,
    parentId: null,
    orderKey,
    name: shapeDisplayName(geometry),
    altText: null,
    layoutPlaceholderId: null,
    transform: { x: 1_650_000, y: 1_650_000, width: 3_000_000, height: isLine ? 360_000 : 1_500_000, rotation: 0 },
    visible: true,
    locked: false,
    opacity: 1,
    kind: {
      type: "shape",
      data: {
        geometry,
        style: {
          fill: isLine ? { type: "none" } : { type: "solid", value: { type: "theme", value: "accent1" } },
          stroke: isLine ? { color: { type: "theme", value: "accent1" }, width: 2 } : null,
        },
      },
    },
  };
}

export function createRectangleNode(id: string, orderKey: string): PresentationV5Node {
  return createShapeNode(id, orderKey, "rectangle");
}

/**
 * Connectors keep their endpoints in slide coordinates.  Their transform is
 * only a conservative hit-test bound for older projection consumers; the
 * canvas resolves the actual line directly from `start` and `end`.
 */
export function createConnectorNode(id: string, orderKey: string): PresentationV5Node {
  const start: ConnectorEndpoint = { type: "free", value: { x: 2_000_000, y: 2_000_000 } };
  const end: ConnectorEndpoint = { type: "free", value: { x: 5_000_000, y: 3_200_000 } };
  return {
    id,
    parentId: null,
    orderKey,
    name: "连接线",
    altText: null,
    layoutPlaceholderId: null,
    transform: {
      x: Math.min(start.value.x, end.value.x),
      y: Math.min(start.value.y, end.value.y),
      width: Math.abs(end.value.x - start.value.x),
      height: Math.abs(end.value.y - start.value.y),
      rotation: 0,
    },
    visible: true,
    locked: false,
    opacity: 1,
    kind: { type: "connector", data: { start, end } },
  };
}

function shapeDisplayName(geometry: BuiltinShapeGeometry): string {
  switch (geometry) {
    case "rectangle": return "矩形";
    case "ellipse": return "圆形";
    case "line": return "直线";
    case "arrow": return "箭头";
  }
}

/**
 * A duplicate receives a new canonical identity and a small positional offset
 * so the original stays discoverable. The returned value stays strictly typed
 * and is submitted through `presentation.insertNode`.
 */
export function duplicatePresentationNode(node: PresentationV5Node, id: string, orderKey: string): PresentationV5Node {
  return {
    ...node,
    id,
    orderKey,
    name: node.name ? `${node.name} 副本` : null,
    parentId: null,
    transform: { ...node.transform, x: node.transform.x + 180_000, y: node.transform.y + 180_000 },
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
    case "insertNode": return "presentation.insertNode";
    case "deleteNode": return "presentation.deleteNode";
    case "groupNodes": return "presentation.groupNodes";
    case "ungroupNodes": return "presentation.ungroupNodes";
    case "reorderNode": return "presentation.reorderNode";
    case "setNodeTransform": return "presentation.setNodeTransform";
    case "setNodeLocked": return "presentation.setNodeLocked";
    case "alignNodes": return "presentation.alignNodes";
    case "distributeNodes": return "presentation.distributeNodes";
    case "setShapeStyle": return "presentation.setShapeStyle";
    case "setShapeGeometry": return "presentation.setShapeGeometry";
    case "setChartSpec": return "presentation.setChartSpec";
    case "setConnectorEndpoints": return "presentation.setConnectorEndpoints";
    case "setTableCellContent": return "presentation.setTableCellContent";
    case "setTableCellStyle": return "presentation.setTableCellStyle";
    case "insertTableRows": return "presentation.insertTableRows";
    case "insertTableColumns": return "presentation.insertTableColumns";
    case "deleteTableRow": return "presentation.deleteTableRow";
    case "deleteTableColumn": return "presentation.deleteTableColumn";
    case "mergeTableCells": return "presentation.mergeTableCells";
    case "splitTableCell": return "presentation.splitTableCell";
    case "setTextContent": return "presentation.setTextContent";
    case "setTextFrame": return "presentation.setTextFrame";
    case "setImageConfig": return "presentation.setImageConfig";
    case "setMediaConfig": return "presentation.setMediaConfig";
  }
}
