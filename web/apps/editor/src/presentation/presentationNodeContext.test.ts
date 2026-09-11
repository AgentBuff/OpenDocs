import { describe, expect, it } from "vitest";

import type { PresentationV5Node } from "@open-office/schema";
import type { PresentationSlideProjection } from "@open-office/schema/api";

import { createTextNode } from "./commands.js";
import { createNodeContext, presentationNodeUiRegistry } from "./presentationNodeContext.js";

const slide = (nodes: PresentationV5Node[]): PresentationSlideProjection => ({
  slideId: "slide-1",
  orderKey: "0001",
  name: "Slide",
  layoutId: null,
  background: { type: "none" },
  nodes,
  assetIds: [],
});

describe("presentation node context boundary", () => {
  it("derives ordered children and a stable primary selection from the projection", () => {
    const parent = { ...createTextNode("parent", "0001") };
    const later = { ...createTextNode("later", "0003"), parentId: parent.id };
    const earlier = { ...createTextNode("earlier", "0002"), parentId: parent.id };
    const context = createNodeContext({
      artifactId: "artifact-1",
      revision: 7,
      slide: slide([parent, later, earlier]),
      node: parent,
      selectedNodeIds: ["parent", "later"],
      mode: "text",
      availableCapabilities: new Set(["presentation.setTextContent"]),
    });

    expect(context.childNodeIds).toEqual(["earlier", "later"]);
    expect(context.selection).toEqual({
      refs: [
        { slideId: "slide-1", nodeId: "parent" },
        { slideId: "slide-1", nodeId: "later" },
      ],
      primary: { slideId: "slide-1", nodeId: "later" },
      mode: "text",
    });
  });

  it("keeps capability gating in the registry instead of the React container", () => {
    const node = createTextNode("text-1", "0001");
    const enabled = createNodeContext({
      artifactId: "artifact-1",
      revision: 1,
      slide: slide([node]),
      node,
      selectedNodeIds: [node.id],
      mode: "node",
      availableCapabilities: new Set(["presentation.setTextContent"]),
    });
    const disabled = { ...enabled, availableCapabilities: new Set<string>() };

    expect(presentationNodeUiRegistry.mapAction("text.content", {
      context: enabled,
      value: { text: "updated", runs: [] },
    })).toEqual([{
      type: "setTextContent",
      slideId: "slide-1",
      nodeId: "text-1",
      body: { text: "updated", runs: [] },
    }]);
    expect(() => presentationNodeUiRegistry.mapAction("text.content", {
      context: disabled,
      value: { text: "blocked", runs: [] },
    })).toThrow("capability is unavailable");
  });
});
