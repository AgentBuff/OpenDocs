import type { PresentationV5ChartSpec, PresentationV5Node, PresentationV5NodeKind, PresentationV5RichText, PresentationV5Transform } from "@open-office/schema";
import { PresentationExtensionRegistry } from "./extension.js";
import { PresentationNodeRegistry } from "./registry.js";
import type {
  PresentationActionInvocation,
  PresentationNodeAdornment,
  PresentationNodeContext,
  PresentationNodeRegistration,
  PresentationNodeToolbarDescriptor,
} from "./types.js";

export type BuiltinPresentationNodeAction =
  | "node.duplicate"
  | "node.delete"
  | "node.transform"
  | "node.lock"
  | "node.bringForward"
  | "node.sendBackward"
  | "node.bringToFront"
  | "node.sendToBack"
  | "shape.style"
  | "shape.geometry"
  | "chart.spec"
  | "connector.endpoints"
  | "table.cellContent"
  | "table.cellStyle"
  | "table.insertRows"
  | "table.insertColumns"
  | "table.deleteRow"
  | "table.deleteColumn"
  | "table.mergeCells"
  | "table.splitCell"
  | "text.content"
  | "text.frame"
  | "image.config"
  | "media.config"
  | "group.ungroup";

export function createBuiltinPresentationNodeRegistry(options: { extensions?: PresentationExtensionRegistry } = {}): PresentationNodeRegistry<BuiltinPresentationNodeAction> {
  const extensions = options.extensions ?? new PresentationExtensionRegistry();
  return new PresentationNodeRegistry([
    createShapeRegistration(),
    createChartRegistration(),
    createConnectorRegistration(),
    createTableRegistration(),
    createTextRegistration(),
    createImageRegistration(),
    createMediaRegistration("video"),
    createMediaRegistration("audio"),
    createGroupRegistration(),
    createExtensionRegistration(extensions),
  ]);
}

const COMMAND_CAPABILITIES: Record<BuiltinPresentationNodeAction, string> = {
  "node.duplicate": "presentation.insertNode",
  "node.delete": "presentation.deleteNode",
  "node.transform": "presentation.setNodeTransform",
  "node.lock": "presentation.setNodeLocked",
  "node.bringForward": "presentation.reorderNode",
  "node.sendBackward": "presentation.reorderNode",
  "node.bringToFront": "presentation.reorderNode",
  "node.sendToBack": "presentation.reorderNode",
  "shape.style": "presentation.setShapeStyle",
  "shape.geometry": "presentation.setShapeGeometry",
  "chart.spec": "presentation.setChartSpec",
  "connector.endpoints": "presentation.setConnectorEndpoints",
  "table.cellContent": "presentation.setTableCellContent",
  "table.cellStyle": "presentation.setTableCellStyle",
  "table.insertRows": "presentation.insertTableRows",
  "table.insertColumns": "presentation.insertTableColumns",
  "table.deleteRow": "presentation.deleteTableRow",
  "table.deleteColumn": "presentation.deleteTableColumn",
  "table.mergeCells": "presentation.mergeTableCells",
  "table.splitCell": "presentation.splitTableCell",
  "text.content": "presentation.setTextContent",
  "text.frame": "presentation.setTextFrame",
  "image.config": "presentation.setImageConfig",
  "media.config": "presentation.setMediaConfig",
  "group.ungroup": "presentation.ungroupNodes",
};

function baseToolbar<ActionId extends BuiltinPresentationNodeAction>(action: ActionId, label: string, icon: string): PresentationNodeToolbarDescriptor<ActionId> {
  return {
    id: `presentation.node.${action}`,
    // Capabilities are server-advertised command type ids.  Keep this mapping
    // explicit: UI action labels are intentionally decoupled from transport
    // ids, while a missing server command must remove the affordance entirely.
    capability: COMMAND_CAPABILITIES[action],
    group: "node",
    kind: "button",
    action,
    label,
    ariaLabel: label,
    icon,
  };
}

function selectionOutline(context: PresentationNodeContext): readonly PresentationNodeAdornment[] {
  if (!context.selection.refs.some((ref) => ref.slideId === context.slideId && ref.nodeId === context.node.id)) return [];
  return [{
    kind: "outline",
    node: { slideId: context.slideId, nodeId: context.node.id },
    bounds: context.node.transform,
    // The current semantic command supports a single proportional-free resize
    // gesture from the south-east handle.  Do not advertise handles that the
    // UI has not yet mapped to a real command.
    handles: context.node.locked || context.node.kind.type === "extension" ? [] : ["southEast"],
  }];
}

function requireValue<T>(invocation: PresentationActionInvocation, label: string): T {
  if (invocation.value === undefined) throw new Error(`presentation node action requires ${label}`);
  return invocation.value as T;
}

function shapeData(context: PresentationNodeContext): Extract<PresentationV5NodeKind, { type: "shape" }>["data"] {
  if (context.node.kind.type !== "shape") throw new Error(`presentation node renderer requires shape, received ${context.node.kind.type}`);
  return context.node.kind.data;
}

function connectorData(context: PresentationNodeContext): Extract<PresentationV5NodeKind, { type: "connector" }>["data"] {
  if (context.node.kind.type !== "connector") throw new Error(`presentation node renderer requires connector, received ${context.node.kind.type}`);
  return context.node.kind.data;
}

function textData(context: PresentationNodeContext): Extract<PresentationV5NodeKind, { type: "text" }>["data"] {
  if (context.node.kind.type !== "text") throw new Error(`presentation node renderer requires text, received ${context.node.kind.type}`);
  return context.node.kind.data;
}

function imageData(context: PresentationNodeContext): Extract<PresentationV5NodeKind, { type: "image" }>["data"] {
  if (context.node.kind.type !== "image") throw new Error(`presentation node renderer requires image, received ${context.node.kind.type}`);
  return context.node.kind.data;
}

function tableData(context: PresentationNodeContext): Extract<PresentationV5NodeKind, { type: "table" }>["data"] {
  if (context.node.kind.type !== "table") throw new Error(`presentation node renderer requires table, received ${context.node.kind.type}`);
  return context.node.kind.data;
}

function extensionData(context: PresentationNodeContext): Extract<PresentationV5NodeKind, { type: "extension" }>["data"] {
  if (context.node.kind.type !== "extension") throw new Error(`presentation node renderer requires extension, received ${context.node.kind.type}`);
  return context.node.kind.data;
}

function createShapeRegistration(): PresentationNodeRegistration<BuiltinPresentationNodeAction> {
  return {
    type: "shape",
    renderer: (context) => {
      const shape = shapeData(context);
      return { kind: "shape", geometry: shape.geometry };
    },
    selectionAdornment: selectionOutline,
    toolbar: [
      baseToolbar("shape.geometry", "切换形状", "shape"),
      ...nodeToolbar("shape.style", "形状样式", "format-painter"),
    ],
    inspector: { id: "presentation.shape", title: "形状", fields: [
      { id: "geometry", label: "形状", kind: "select", action: "shape.geometry" },
      { id: "style", label: "填充与轮廓", kind: "color", action: "shape.style" },
      { id: "transform", label: "位置与大小", kind: "number", action: "node.transform" },
    ] },
    mapAction: (action, invocation) => {
      const node = invocation.context.node;
      if (action === "shape.geometry") return [{ type: "setShapeGeometry", slideId: invocation.context.slideId, nodeId: node.id, geometry: requireValue<Extract<PresentationV5NodeKind, { type: "shape" }>["data"]["geometry"]>(invocation, "shape geometry") }];
      if (action === "shape.style") return [{ type: "setShapeStyle", slideId: invocation.context.slideId, nodeId: node.id, style: requireValue<Extract<PresentationV5NodeKind, { type: "shape" }>["data"]["style"]>(invocation, "shape style") }];
      return mapCommonAction(action, invocation);
    },
  };
}

/**
 * Chart data is intentionally one complete `ChartSpec` mutation.  The
 * inspector may keep an uncommitted form draft, but a chart never becomes a
 * collection of renderer-owned category or series patches.
 */
function createChartRegistration(): PresentationNodeRegistration<BuiltinPresentationNodeAction> {
  return {
    type: "chart",
    renderer: (context) => {
      if (context.node.kind.type !== "chart") throw new Error(`presentation node renderer requires chart, received ${context.node.kind.type}`);
      return { kind: "chart", spec: context.node.kind.data.spec };
    },
    selectionAdornment: selectionOutline,
    toolbar: [...nodeToolbar("chart.spec", "编辑图表", "chart")],
    inspector: { id: "presentation.chart", title: "图表", fields: [
      { id: "spec", label: "图表数据", kind: "text", action: "chart.spec" },
      { id: "transform", label: "位置与大小", kind: "number", action: "node.transform" },
    ] },
    mapAction: (action, invocation) => {
      if (action === "chart.spec") return [{
        type: "setChartSpec",
        slideId: invocation.context.slideId,
        nodeId: invocation.context.node.id,
        spec: requireValue<PresentationV5ChartSpec>(invocation, "chart spec"),
      }];
      return mapCommonAction(action, invocation);
    },
  };
}

/**
 * A connector owns its two endpoint references as one semantic value.  The
 * registry deliberately exposes no renderer-side "start" / "end" patches.
 */
function createConnectorRegistration(): PresentationNodeRegistration<BuiltinPresentationNodeAction> {
  return {
    type: "connector",
    renderer: (context) => {
      const connector = connectorData(context);
      return { kind: "connector", start: connector.start, end: connector.end };
    },
    selectionAdornment: selectionOutline,
    toolbar: [baseToolbar("connector.endpoints", "连接线端点", "connector"), ...nodeCommonToolbar()],
    inspector: { id: "presentation.connector", title: "连接线", fields: [
      { id: "endpoints", label: "起点与终点", kind: "select", action: "connector.endpoints" },
      { id: "transform", label: "位置与大小", kind: "number", action: "node.transform" },
    ] },
    mapAction: (action, invocation) => {
      const node = invocation.context.node;
      if (action === "connector.endpoints") {
        const value = requireValue<Extract<PresentationV5NodeKind, { type: "connector" }>["data"]>(invocation, "connector endpoints");
        return [{ type: "setConnectorEndpoints", slideId: invocation.context.slideId, nodeId: node.id, start: value.start, end: value.end }];
      }
      return mapCommonAction(action, invocation);
    },
  };
}

/**
 * A table cell is identified by its canonical top-left anchor. Grid resizing,
 * merge/split and border topology remain separate semantic domains rather
 * than leaking through a generic table-node patch.
 */
function createTableRegistration(): PresentationNodeRegistration<BuiltinPresentationNodeAction> {
  return {
    type: "table",
    renderer: (context) => {
      const table = tableData(context);
      return { kind: "table", rows: table.rows, columns: table.columns };
    },
    selectionAdornment: selectionOutline,
    toolbar: [
      baseToolbar("table.cellContent", "编辑单元格", "text"),
      baseToolbar("table.cellStyle", "单元格样式", "table"),
      baseToolbar("table.insertRows", "插入行", "insert-row-column"),
      baseToolbar("table.insertColumns", "插入列", "insert-row-column"),
      baseToolbar("table.deleteRow", "删除行", "delete"),
      baseToolbar("table.deleteColumn", "删除列", "delete"),
      baseToolbar("table.mergeCells", "合并单元格", "merge-cells"),
      baseToolbar("table.splitCell", "拆分单元格", "split-cells"),
      ...nodeCommonToolbar(),
    ],
    inspector: { id: "presentation.table", title: "表格", fields: [
      { id: "cell-content", label: "单元格内容", kind: "text", action: "table.cellContent" },
      { id: "cell-style", label: "单元格样式", kind: "color", action: "table.cellStyle" },
      { id: "insert-rows", label: "插入行", kind: "number", action: "table.insertRows" },
      { id: "insert-columns", label: "插入列", kind: "number", action: "table.insertColumns" },
      { id: "delete-row", label: "删除行", kind: "toggle", action: "table.deleteRow" },
      { id: "delete-column", label: "删除列", kind: "toggle", action: "table.deleteColumn" },
      { id: "merge-cells", label: "合并单元格", kind: "toggle", action: "table.mergeCells" },
      { id: "split-cell", label: "拆分单元格", kind: "toggle", action: "table.splitCell" },
      { id: "transform", label: "位置与大小", kind: "number", action: "node.transform" },
    ] },
    mapAction: (action, invocation) => {
      const node = invocation.context.node;
      if (action === "table.cellContent") {
        const value = requireValue<{ row: number; column: number; content: PresentationV5RichText }>(invocation, "table cell content");
        return [{ type: "setTableCellContent", slideId: invocation.context.slideId, nodeId: node.id, ...value }];
      }
      if (action === "table.cellStyle") {
        const value = requireValue<{ cells: { row: number; column: number }[]; style: Extract<PresentationV5NodeKind, { type: "table" }>["data"]["cells"][number]["style"] }>(invocation, "table cell style");
        return [{ type: "setTableCellStyle", slideId: invocation.context.slideId, nodeId: node.id, ...value }];
      }
      if (action === "table.insertRows") {
        const value = requireValue<{ index: number; count: number }>(invocation, "table row insertion");
        return [{ type: "insertTableRows", slideId: invocation.context.slideId, nodeId: node.id, ...value }];
      }
      if (action === "table.insertColumns") {
        const value = requireValue<{ index: number; count: number }>(invocation, "table column insertion");
        return [{ type: "insertTableColumns", slideId: invocation.context.slideId, nodeId: node.id, ...value }];
      }
      if (action === "table.deleteRow") {
        const value = requireValue<{ index: number }>(invocation, "table row deletion");
        return [{ type: "deleteTableRow", slideId: invocation.context.slideId, nodeId: node.id, ...value }];
      }
      if (action === "table.deleteColumn") {
        const value = requireValue<{ index: number }>(invocation, "table column deletion");
        return [{ type: "deleteTableColumn", slideId: invocation.context.slideId, nodeId: node.id, ...value }];
      }
      if (action === "table.mergeCells") {
        const value = requireValue<{ start: { row: number; column: number }; end: { row: number; column: number } }>(invocation, "table merge range");
        return [{ type: "mergeTableCells", slideId: invocation.context.slideId, nodeId: node.id, ...value }];
      }
      if (action === "table.splitCell") {
        const value = requireValue<{ row: number; column: number }>(invocation, "table split anchor");
        return [{ type: "splitTableCell", slideId: invocation.context.slideId, nodeId: node.id, ...value }];
      }
      return mapCommonAction(action, invocation);
    },
  };
}

function createTextRegistration(): PresentationNodeRegistration<BuiltinPresentationNodeAction> {
  return {
    type: "text",
    renderer: (context) => ({ kind: "text", body: textData(context).frame.body }),
    selectionAdornment: selectionOutline,
    toolbar: [baseToolbar("text.content", "编辑文字", "text"), baseToolbar("text.frame", "文本框", "text-box"), ...nodeCommonToolbar()],
    inspector: { id: "presentation.text", title: "文本", fields: [
      { id: "content", label: "内容", kind: "text", action: "text.content" },
      { id: "frame", label: "文本框", kind: "select", action: "text.frame" },
      { id: "transform", label: "位置与大小", kind: "number", action: "node.transform" },
    ] },
    mapAction: (action, invocation) => {
      const node = invocation.context.node;
      if (action === "text.content") return [{ type: "setTextContent", slideId: invocation.context.slideId, nodeId: node.id, body: requireValue<Extract<PresentationV5NodeKind, { type: "text" }>["data"]["frame"]["body"]>(invocation, "rich text body") }];
      if (action === "text.frame") return [{ type: "setTextFrame", slideId: invocation.context.slideId, nodeId: node.id, frame: requireValue<Extract<PresentationV5NodeKind, { type: "text" }>["data"]["frame"]>(invocation, "text frame") }];
      return mapCommonAction(action, invocation);
    },
  };
}

function createImageRegistration(): PresentationNodeRegistration<BuiltinPresentationNodeAction> {
  return {
    type: "image",
    renderer: (context) => {
      const image = imageData(context);
      return { kind: "image", assetId: image.assetId, crop: image.crop, flipH: image.flipH, flipV: image.flipV };
    },
    selectionAdornment: selectionOutline,
    toolbar: nodeToolbar("image.config", "图片设置", "image"),
    inspector: { id: "presentation.image", title: "图片", fields: [
      { id: "crop", label: "裁剪与翻转", kind: "select", action: "image.config" },
      { id: "caption", label: "题注", kind: "text", action: "image.config" },
      { id: "transform", label: "位置与大小", kind: "number", action: "node.transform" },
    ] },
    mapAction: (action, invocation) => {
      const node = invocation.context.node;
      if (action === "image.config") return [{ type: "setImageConfig", slideId: invocation.context.slideId, nodeId: node.id, image: requireValue<Extract<PresentationV5NodeKind, { type: "image" }>["data"]>(invocation, "image config") }];
      return mapCommonAction(action, invocation);
    },
  };
}

/**
 * Audio and video share an immutable asset-reference contract. This registration deliberately
 * does not pretend to edit a timeline or binary media: it exposes only the typed reference
 * replacement command backed by the server capability catalog.
 */
function createMediaRegistration(mediaType: "video" | "audio"): PresentationNodeRegistration<BuiltinPresentationNodeAction> {
  return {
    type: mediaType,
    renderer: (context) => {
      const node = context.node;
      if (node.kind.type !== mediaType) throw new Error(`presentation media renderer requires ${mediaType}`);
      return { kind: "media", mediaType, assetId: node.kind.data.assetId, posterAssetId: node.kind.data.posterAssetId };
    },
    selectionAdornment: selectionOutline,
    toolbar: nodeToolbar("media.config", "媒体设置", mediaType === "video" ? "video" : "audio"),
    inspector: { id: `presentation.${mediaType}`, title: mediaType === "video" ? "视频" : "音频", fields: [
      { id: "asset", label: "媒体与封面", kind: "select", action: "media.config" },
      { id: "transform", label: "位置与大小", kind: "number", action: "node.transform" },
    ] },
    mapAction: (action, invocation) => {
      if (action === "media.config") return [{
        type: "setMediaConfig",
        slideId: invocation.context.slideId,
        nodeId: invocation.context.node.id,
        media: requireValue<Extract<PresentationV5NodeKind, { type: "video" | "audio" }>["data"]>(invocation, "media config"),
      }];
      return mapCommonAction(action, invocation);
    },
  };
}

function createGroupRegistration(): PresentationNodeRegistration<BuiltinPresentationNodeAction> {
  return {
    type: "group",
    renderer: (context) => ({
      kind: "group",
      childNodeIds: context.childNodeIds,
    }),
    selectionAdornment: selectionOutline,
    // Group duplication requires a deep hierarchy clone with new child IDs.
    // Do not expose a shallow duplicate that would create an empty group.
    toolbar: [baseToolbar("group.ungroup", "取消组合", "ungroup"), ...nodeOrderToolbar(), baseToolbar("node.transform", "位置与大小", "resize"), baseToolbar("node.lock", "锁定对象", "lock"), baseToolbar("node.delete", "删除", "delete")],
    inspector: { id: "presentation.group", title: "组合", fields: [
      { id: "ungroup", label: "取消组合", kind: "toggle", action: "group.ungroup" },
      { id: "transform", label: "位置与大小", kind: "number", action: "node.transform" },
    ] },
    mapAction: (action, invocation) => {
      if (action === "group.ungroup") return [{ type: "ungroupNodes", slideId: invocation.context.slideId, groupId: invocation.context.node.id }];
      return mapCommonAction(action, invocation);
    },
  };
}

function createExtensionRegistration(extensions: PresentationExtensionRegistry): PresentationNodeRegistration<BuiltinPresentationNodeAction> {
  return {
    type: "extension",
    renderer: (context) => {
      const extension = extensionData(context);
      const resolved = extensions.resolve(context);
      return resolved.status === "registered"
        ? { kind: "extension", namespace: extension.namespace, version: extension.version, typeId: extension.typeId, unsupported: false, label: resolved.renderModel.label, summary: resolved.renderModel.summary }
        : { kind: "extension", namespace: extension.namespace, version: extension.version, typeId: extension.typeId, unsupported: true, reason: resolved.reason };
    },
    selectionAdornment: selectionOutline,
    toolbar: [baseToolbar("node.delete", "删除", "delete")],
    inspector: { id: "presentation.extension", title: "扩展节点", fields: [{ id: "availability", label: "扩展节点以只读方式保留；只有宿主注册的 renderer 可显示其投影", kind: "readonly", unavailableReason: "extension node has no registered editor" }] },
    mapAction: (action, invocation) => mapCommonAction(action, invocation),
  };
}

function mapCommonAction(action: BuiltinPresentationNodeAction, invocation: PresentationActionInvocation): ReturnType<PresentationNodeRegistration<BuiltinPresentationNodeAction>["mapAction"]> {
  const node = invocation.context.node;
  if (action === "node.duplicate") return [requireValue<{ type: "insertNode"; slideId: string; node: PresentationV5Node; index: number }>(invocation, "duplicate node")];
  if (action === "node.delete") return [{ type: "deleteNode" as const, slideId: invocation.context.slideId, nodeId: node.id }];
  if (action === "node.bringForward" || action === "node.sendBackward" || action === "node.bringToFront" || action === "node.sendToBack") return [requireValue<{ type: "reorderNode"; slideId: string; nodeId: string; index: number }>(invocation, "node order")];
  if (action === "node.transform") return [{ type: "setNodeTransform" as const, slideId: invocation.context.slideId, nodeId: node.id, transform: requireValue<PresentationV5Transform>(invocation, "node transform") }];
  if (action === "node.lock") return [{ type: "setNodeLocked" as const, slideId: invocation.context.slideId, nodeId: node.id, locked: requireValue<boolean>(invocation, "node lock state") }];
  throw new Error(`presentation node action is not supported for ${node.kind.type}: ${action}`);
}

function nodeToolbar<ActionId extends "shape.style" | "chart.spec" | "text.content" | "image.config" | "media.config">(
  action: ActionId,
  label: string,
  icon: string,
): readonly PresentationNodeToolbarDescriptor<BuiltinPresentationNodeAction>[] {
  return [baseToolbar(action, label, icon), ...nodeCommonToolbar()];
}

function nodeCommonToolbar(): readonly PresentationNodeToolbarDescriptor<BuiltinPresentationNodeAction>[] {
  return [
    ...nodeOrderToolbar(),
    baseToolbar("node.transform", "位置与大小", "resize"),
    baseToolbar("node.lock", "锁定对象", "lock"),
    baseToolbar("node.duplicate", "复制对象", "copy"),
    baseToolbar("node.delete", "删除", "delete"),
  ];
}

function nodeOrderToolbar(): readonly PresentationNodeToolbarDescriptor<BuiltinPresentationNodeAction>[] {
  return [
    baseToolbar("node.bringForward", "上移一层", "bring-forward"),
    baseToolbar("node.sendBackward", "下移一层", "send-backward"),
    baseToolbar("node.bringToFront", "置于顶层", "bring-front"),
    baseToolbar("node.sendToBack", "置于底层", "send-back"),
  ];
}
