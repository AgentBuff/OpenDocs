import { describe, expect, it } from "vitest";
import { createBuiltinPresentationNodeRegistry, PRESENTATION_NODE_TYPES, PresentationExtensionRegistry } from "../src/index.js";
import type { PresentationV5Node } from "@open-office/schema";
import type { PresentationNodeContext } from "../src/index.js";

function context(node: PresentationV5Node, capabilities = new Set<string>()): PresentationNodeContext {
  return {
    artifactId: "deck-1",
    revision: 4,
    slideId: "slide-1",
    node,
    childNodeIds: [],
    selection: { refs: [{ slideId: "slide-1", nodeId: node.id }], primary: { slideId: "slide-1", nodeId: node.id }, mode: "node" },
    availableCapabilities: capabilities,
  };
}

const base = {
  id: "node-1", parentId: null, orderKey: "a", name: null, altText: null, layoutPlaceholderId: null,
  transform: { x: 1, y: 2, width: 100, height: 80, rotation: 0 }, visible: true, locked: false, opacity: 1,
} as const;

describe("PresentationNodeRegistry", () => {
  it("registers complete node UI only for supported node types", () => {
    const registry = createBuiltinPresentationNodeRegistry();
    expect(PRESENTATION_NODE_TYPES).toEqual(["shape", "text", "image", "video", "audio", "table", "chart", "connector", "group", "embed", "extension"]);
    expect(registry.has("text")).toBe(true);
    expect(registry.has("table")).toBe(false);
    const unsupported = registry.resolve(context({ ...base, kind: { type: "table", data: { rows: 1, columns: 1, cells: [] } } }));
    expect(unsupported.renderModel).toMatchObject({ kind: "unsupported", nodeType: "table" });
    expect(unsupported.toolbar).toEqual([]);
    expect(unsupported.inspector.fields[0]?.unavailableReason).toContain("不会提供不可执行");
  });

  it("maps text actions to semantic commands without mutating the node", () => {
    const registry = createBuiltinPresentationNodeRegistry();
    const node = { ...base, kind: { type: "text", data: { frame: { body: { text: "old", runs: [] }, verticalAlign: "top", padding: { top: 0, right: 0, bottom: 0, left: 0 }, autoFit: "none" } } } } satisfies PresentationV5Node;
    const source = context(node, new Set(["presentation.setTextContent"]));
    const resolved = registry.resolve(source);
    expect(resolved.toolbar.map((item) => item.id)).toEqual(["presentation.node.text.content"]);
    const commands = registry.mapAction("text.content", { context: source, value: { text: "new", runs: [] } });
    expect(commands).toEqual([{ type: "setTextContent", slideId: "slide-1", nodeId: "node-1", body: { text: "new", runs: [] } }]);
    expect(node.kind.data.frame.body.text).toBe("old");
    expect(() => registry.mapAction("text.content", { context: context(node), value: { text: "blocked", runs: [] } })).toThrow("capability is unavailable");
  });

  it("maps image crop, flip and caption as one strict image config command", () => {
    const registry = createBuiltinPresentationNodeRegistry();
    const image = {
      ...base,
      kind: {
        type: "image",
        data: {
          assetId: "asset-current",
          originalAssetId: "asset-original",
          crop: { top: 0, right: 0, bottom: 0, left: 0 },
          flipH: false,
          flipV: false,
          caption: null,
        },
      },
    } satisfies PresentationV5Node;
    const source = context(image, new Set(["presentation.setImageConfig"]));
    expect(registry.resolve(source).toolbar.map((item) => item.id)).toEqual(["presentation.node.image.config"]);
    const next = { ...image.kind.data, crop: { top: 0.1, right: 0, bottom: 0, left: 0.1 }, flipH: true, caption: "示例" };
    expect(registry.mapAction("image.config", { context: source, value: next })).toEqual([
      { type: "setImageConfig", slideId: "slide-1", nodeId: "node-1", image: next },
    ]);
    expect(image.kind.data.caption).toBeNull();
  });

  it("maps complete shape fill and outline through the concrete style capability", () => {
    const registry = createBuiltinPresentationNodeRegistry();
    const shape = { ...base, kind: { type: "shape", data: { geometry: "rectangle", style: { fill: { type: "none" }, stroke: null } } } } satisfies PresentationV5Node;
    const source = context(shape, new Set(["presentation.setShapeStyle"]));
    const style = {
      fill: { type: "solid" as const, value: { type: "rgba" as const, value: { r: 32, g: 84, b: 180, a: 255 } } },
      stroke: { color: { type: "rgba" as const, value: { r: 10, g: 20, b: 30, a: 255 } }, width: 1.5 },
    };
    expect(registry.mapAction("shape.style", { context: source, value: style })).toEqual([
      { type: "setShapeStyle", slideId: "slide-1", nodeId: "node-1", style },
    ]);
    expect(shape.kind.data.style.stroke).toBeNull();
  });

  it("gets group children from the immutable slide projection instead of selection state", () => {
    const registry = createBuiltinPresentationNodeRegistry();
    const node: PresentationV5Node = { ...base, kind: { type: "group", data: {} } };
    const source = { ...context(node), childNodeIds: ["child-a", "child-b"] };
    expect(registry.resolve(source).renderModel).toEqual({ kind: "group", childNodeIds: ["child-a", "child-b"] });
  });

  it("keeps unregistered extension data read-only while allowing only its real delete command", () => {
    const registry = createBuiltinPresentationNodeRegistry();
    const node: PresentationV5Node = { ...base, kind: { type: "extension", data: { namespace: "com.example.widget", version: "1", typeId: "widget", data: { opaque: true } } } };
    const resolved = registry.resolve(context(node));
    expect(resolved.renderModel).toMatchObject({ kind: "extension", unsupported: true, reason: expect.stringMatching(/未注册/) });
    expect(resolved.toolbar).toEqual([]);
    expect(resolved.adornments).toEqual(expect.arrayContaining([
      expect.objectContaining({ kind: "outline", handles: [] }),
    ]));
    const enabled = context(node, new Set(["presentation.deleteNode"]));
    expect(registry.mapAction("node.delete", { context: enabled })).toEqual([{ type: "deleteNode", slideId: "slide-1", nodeId: "node-1" }]);
    expect(() => registry.mapAction("node.transform", { context: enabled })).toThrow("not registered");
  });

  it("resolves extension renderers only through an exact manifest and capability grant", () => {
    const extensions = new PresentationExtensionRegistry([{
      namespace: "com.example.widget", version: "1", typeId: "widget", capability: "presentation.extension.com.example.widget",
      renderer: ({ payload }) => ({ label: "Widget", summary: String(payload.data.title) }),
      inspector: { id: "example.widget", title: "Widget", fields: [{ id: "title", label: "标题", value: "只读" }] },
    }]);
    const registry = createBuiltinPresentationNodeRegistry({ extensions });
    const node: PresentationV5Node = { ...base, kind: { type: "extension", data: { namespace: "com.example.widget", version: "1", typeId: "widget", data: { title: "Hello" } } } };
    expect(registry.resolve(context(node)).renderModel).toMatchObject({ kind: "extension", unsupported: true, reason: expect.stringMatching(/未授予/) });
    const granted = context(node, new Set(["presentation.extension.com.example.widget"]));
    expect(registry.resolve(granted).renderModel)
      .toMatchObject({ kind: "extension", unsupported: false, label: "Widget", summary: "Hello" });
  });

  it("rejects extension registrations that could masquerade as host write capabilities", () => {
    expect(() => new PresentationExtensionRegistry([{
      namespace: "com.example", version: "1", typeId: "unsafe", capability: "presentation.setNodeTransform",
      renderer: () => ({ label: "unsafe" }), inspector: { id: "unsafe", title: "unsafe", fields: [] },
    }])).toThrow(/presentation\.extension/);
  });

  it("rejects duplicate registrations and invalid toolbar capability namespaces", () => {
    const registry = createBuiltinPresentationNodeRegistry();
    expect(() => registry.register({
      type: "text",
      renderer: () => ({ kind: "text", body: { text: "", runs: [] } }),
      selectionAdornment: () => [], toolbar: [], inspector: { id: "x", title: "x", fields: [] }, mapAction: () => [],
    })).toThrow("already exists");
  });

  it("makes Inspector fields read-only when the server did not advertise their command", () => {
    const registry = createBuiltinPresentationNodeRegistry();
    const node = { ...base, kind: { type: "shape", data: { geometry: "rectangle", style: { fill: { type: "none" }, stroke: null } } } } satisfies PresentationV5Node;
    const resolved = registry.resolve(context(node));
    expect(resolved.toolbar).toEqual([]);
    expect(resolved.inspector.fields).toEqual(expect.arrayContaining([
      expect.objectContaining({ id: "style", kind: "readonly", unavailableReason: "当前服务未声明 shape.style 能力" }),
    ]));
  });
});
