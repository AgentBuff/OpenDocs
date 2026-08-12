import type { PresentationV5NodeKind, PresentationV5Transform } from "@open-office/schema";
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
  | "node.delete"
  | "node.transform"
  | "shape.style"
  | "text.content"
  | "text.frame"
  | "image.config"
  | "media.config"
  | "group.ungroup";

export function createBuiltinPresentationNodeRegistry(options: { extensions?: PresentationExtensionRegistry } = {}): PresentationNodeRegistry<BuiltinPresentationNodeAction> {
  const extensions = options.extensions ?? new PresentationExtensionRegistry();
  return new PresentationNodeRegistry([
    createShapeRegistration(),
    createTextRegistration(),
    createImageRegistration(),
    createMediaRegistration("video"),
    createMediaRegistration("audio"),
    createGroupRegistration(),
    createExtensionRegistration(extensions),
  ]);
}

const COMMAND_CAPABILITIES: Record<BuiltinPresentationNodeAction, string> = {
  "node.delete": "presentation.deleteNode",
  "node.transform": "presentation.setNodeTransform",
  "shape.style": "presentation.setShapeStyle",
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

function textData(context: PresentationNodeContext): Extract<PresentationV5NodeKind, { type: "text" }>["data"] {
  if (context.node.kind.type !== "text") throw new Error(`presentation node renderer requires text, received ${context.node.kind.type}`);
  return context.node.kind.data;
}

function imageData(context: PresentationNodeContext): Extract<PresentationV5NodeKind, { type: "image" }>["data"] {
  if (context.node.kind.type !== "image") throw new Error(`presentation node renderer requires image, received ${context.node.kind.type}`);
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
    toolbar: [baseToolbar("shape.style", "形状样式", "format-painter"), baseToolbar("node.transform", "位置与大小", "resize"), baseToolbar("node.delete", "删除", "delete")],
    inspector: { id: "presentation.shape", title: "形状", fields: [
      { id: "style", label: "填充与轮廓", kind: "color", action: "shape.style" },
      { id: "transform", label: "位置与大小", kind: "number", action: "node.transform" },
    ] },
    mapAction: (action, invocation) => {
      const node = invocation.context.node;
      if (action === "shape.style") return [{ type: "setShapeStyle", slideId: invocation.context.slideId, nodeId: node.id, style: requireValue<Extract<PresentationV5NodeKind, { type: "shape" }>["data"]["style"]>(invocation, "shape style") }];
      return mapCommonAction(action, invocation);
    },
  };
}

function createTextRegistration(): PresentationNodeRegistration<BuiltinPresentationNodeAction> {
  return {
    type: "text",
    renderer: (context) => ({ kind: "text", body: textData(context).frame.body }),
    selectionAdornment: selectionOutline,
    toolbar: [baseToolbar("text.content", "编辑文字", "text"), baseToolbar("text.frame", "文本框", "text-box"), baseToolbar("node.transform", "位置与大小", "resize"), baseToolbar("node.delete", "删除", "delete")],
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
    toolbar: [baseToolbar("image.config", "图片设置", "image"), baseToolbar("node.transform", "位置与大小", "resize"), baseToolbar("node.delete", "删除", "delete")],
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
    toolbar: [baseToolbar("media.config", "媒体设置", mediaType === "video" ? "video" : "audio"), baseToolbar("node.transform", "位置与大小", "resize"), baseToolbar("node.delete", "删除", "delete")],
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
    toolbar: [baseToolbar("group.ungroup", "取消组合", "ungroup"), baseToolbar("node.transform", "位置与大小", "resize"), baseToolbar("node.delete", "删除", "delete")],
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
  if (action === "node.delete") return [{ type: "deleteNode" as const, slideId: invocation.context.slideId, nodeId: node.id }];
  if (action === "node.transform") return [{ type: "setNodeTransform" as const, slideId: invocation.context.slideId, nodeId: node.id, transform: requireValue<PresentationV5Transform>(invocation, "node transform") }];
  throw new Error(`presentation node action is not supported for ${node.kind.type}: ${action}`);
}
