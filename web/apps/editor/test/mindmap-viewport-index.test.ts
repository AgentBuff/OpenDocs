import { describe, expect, it } from "vitest";
import type { MindmapProjection } from "@open-office/schema/api";
import { createMindmapSpatialIndex, queryMindmapViewport } from "../src/mindmap/viewport-index.js";

const projection: MindmapProjection = {
  theme: "light",
  layout: {
    width: 10_000,
    height: 10_000,
    nodes: Array.from({ length: 10_000 }, (_, index) => ({ id: `n${index}`, depth: 1, x: (index % 100) * 100, y: Math.floor(index / 100) * 100, width: 80, height: 40 })),
  },
  edges: { routes: Array.from({ length: 9_999 }, (_, index) => ({ edgeId: null, parentId: `n${index}`, childId: `n${index + 1}`, points: [{ x: (index % 100) * 100, y: Math.floor(index / 100) * 100 }, { x: ((index + 1) % 100) * 100, y: Math.floor((index + 1) / 100) * 100 }] })) },
  advanced: {
    summaries: [{ summaryId: "s", nodeIds: ["n0", "n1"], points: [{ x: 0, y: 0 }, { x: 100, y: 0 }], labelAnchor: { x: 120, y: 0 } }],
    boundaries: [{ boundaryId: "b", nodeIds: ["n0"], rect: { x: 0, y: 0, width: 80, height: 40 }, labelAnchor: { x: 5, y: 5 } }],
    formulas: [{ formulaId: "f", nodeId: "n0", anchor: { x: 40, y: 60 } }],
  },
};

describe("mindmap viewport spatial index", () => {
  it("keeps a 10k query bounded and includes intersecting graph geometry", () => {
    const index = createMindmapSpatialIndex(projection);
    const samples = Array.from({ length: 101 }, (_, sample) => {
      const started = performance.now();
      queryMindmapViewport(index, { x: sample * 37 - 50, y: sample * 29 - 50, width: 600, height: 500 });
      return performance.now() - started;
    }).sort((left, right) => left - right);
    expect(samples[Math.ceil(samples.length * .95) - 1]).toBeLessThan(8);
    const slice = queryMindmapViewport(index, { x: -50, y: -50, width: 600, height: 500 });
    expect(slice.nodeIndexes.size).toBeLessThan(80);
    expect(slice.nodeIndexes.has(0)).toBe(true);
    expect(slice.summaryIndexes.has(0)).toBe(true);
    expect(slice.boundaryIndexes.has(0)).toBe(true);
    expect(slice.formulaIndexes.has(0)).toBe(true);
    expect(slice.routeIndexes.size).toBeLessThan(100);
  });
});
