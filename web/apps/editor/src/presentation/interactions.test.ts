import { describe, expect, it } from "vitest";

import { changedNodeTransforms, dragPreview, MIN_PRESENTATION_NODE_SIZE, nextNodeSelection } from "./interactions.js";

const transform = { x: 100, y: 200, width: 120_000, height: 80_000, rotation: 0 };

describe("presentation stage interaction primitives", () => {
  it("keeps a multi-selection's relative layout while moving at stage scale", () => {
    const preview = dragPreview({
      nodeIds: ["one", "two"],
      mode: "move",
      origins: { one: transform, two: { ...transform, x: 300 } },
      deltaClientX: 40,
      deltaClientY: -20,
      scale: 2,
    });
    expect(preview).toEqual({
      one: { ...transform, x: 120, y: 190 },
      two: { ...transform, x: 320, y: 190 },
    });
  });

  it("resizes only the primary node and clamps it to the schema-safe minimum", () => {
    const preview = dragPreview({
      nodeIds: ["one", "two"],
      mode: "resize",
      origins: { one: transform, two: { ...transform, x: 300 } },
      deltaClientX: -200_000,
      deltaClientY: -200_000,
      scale: 1,
    });
    expect(preview.one).toEqual({ ...transform, width: MIN_PRESENTATION_NODE_SIZE, height: MIN_PRESENTATION_NODE_SIZE });
    expect(preview.two).toEqual({ ...transform, x: 300 });
  });

  it("emits no persisted mutation for an unchanged preview", () => {
    const nodes = [{ id: "one", transform }] as never[];
    expect(changedNodeTransforms(nodes, { one: { ...transform } })).toEqual([]);
    expect(changedNodeTransforms(nodes, { one: { ...transform, x: 101 } })).toEqual([
      { nodeId: "one", transform: { ...transform, x: 101 } },
    ]);
  });

  it("uses stable additive selection semantics for pointer and keyboard gestures", () => {
    expect(nextNodeSelection([], "one", false)).toEqual(["one"]);
    expect(nextNodeSelection(["one"], "two", true)).toEqual(["one", "two"]);
    expect(nextNodeSelection(["one", "two"], "one", true)).toEqual(["two"]);
  });
});
