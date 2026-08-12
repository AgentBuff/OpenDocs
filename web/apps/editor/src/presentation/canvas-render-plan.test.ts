import { describe, expect, it } from "vitest";

import { deriveCanvasRenderPlan } from "./canvas-render-plan.js";
import { createTextNode } from "./commands.js";

const node = (id: string, x = 0) => ({ ...createTextNode(id, id), transform: { x, y: 0, width: 100, height: 100, rotation: 0 } });

describe("Canvas retained render plan", () => {
  it("does not repaint when an immutable projection is re-read unchanged", () => {
    const nodes = [node("one")];
    const first = deriveCanvasRenderPlan({ previous: null, nodes, preview: {}, width: 800, height: 450, scale: 1 });
    const next = deriveCanvasRenderPlan({ previous: first.snapshot, nodes: [{ ...nodes[0]! }], preview: {}, width: 800, height: 450, scale: 1 });
    expect(next.kind).toBe("noop");
    expect(next.nodes).toEqual([]);
  });

  it("clears the previous and next bounds, then repaints only intersecting nodes for a preview move", () => {
    const nodes = [node("moving"), node("far", 600)];
    const first = deriveCanvasRenderPlan({ previous: null, nodes, preview: {}, width: 800, height: 450, scale: 1 });
    const next = deriveCanvasRenderPlan({ previous: first.snapshot, nodes, preview: { moving: { ...nodes[0]!.transform, x: 200 } }, width: 800, height: 450, scale: 1 });
    expect(next.kind).toBe("partial");
    // The left one-pixel anti-aliasing guard is clipped by the viewport.
    expect(next.dirtyRect).toMatchObject({ x: 0, width: 301 });
    expect(next.nodes.map((candidate) => candidate.id)).toEqual(["moving"]);
  });

  it("uses a full repaint for a structural viewport change", () => {
    const nodes = [node("one")];
    const first = deriveCanvasRenderPlan({ previous: null, nodes, preview: {}, width: 800, height: 450, scale: 1 });
    const next = deriveCanvasRenderPlan({ previous: first.snapshot, nodes, preview: {}, width: 600, height: 450, scale: 0.75 });
    expect(next.kind).toBe("full");
    expect(next.nodes).toEqual(nodes);
  });
});
