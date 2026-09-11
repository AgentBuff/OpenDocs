import type { MindmapProjection } from "@open-office/schema/api";

export interface ViewportRect { x: number; y: number; width: number; height: number }

interface IndexedRect extends ViewportRect { index: number }

interface BucketIndex {
  entries: readonly IndexedRect[];
  buckets: ReadonlyMap<string, readonly number[]>;
}

export interface MindmapSpatialIndex {
  cellSize: number;
  nodes: BucketIndex;
  routes: BucketIndex;
  summaries: BucketIndex;
  boundaries: BucketIndex;
  formulas: BucketIndex;
}

export interface MindmapViewportSlice {
  nodeIndexes: ReadonlySet<number>;
  routeIndexes: ReadonlySet<number>;
  summaryIndexes: ReadonlySet<number>;
  boundaryIndexes: ReadonlySet<number>;
  formulaIndexes: ReadonlySet<number>;
}

export function createMindmapSpatialIndex(projection: MindmapProjection, cellSize = 512): MindmapSpatialIndex {
  if (!Number.isFinite(cellSize) || cellSize < 64) throw new Error("Mindmap spatial index cellSize 必须至少为 64");
  return {
    cellSize,
    nodes: indexRects(projection.layout.nodes.map((node, index) => ({ ...node, index })), cellSize),
    routes: indexRects(projection.edges.routes.map((route, index) => pointsRect(route.points, index, 12)), cellSize),
    summaries: indexRects(projection.advanced.summaries.map((summary, index) => pointsRect([...summary.points, summary.labelAnchor], index, 24)), cellSize),
    boundaries: indexRects(projection.advanced.boundaries.map((boundary, index) => ({ ...boundary.rect, index })), cellSize),
    formulas: indexRects(projection.advanced.formulas.map((formula, index) => ({ x: formula.anchor.x - 80, y: formula.anchor.y - 20, width: 160, height: 40, index })), cellSize),
  };
}

export function queryMindmapViewport(index: MindmapSpatialIndex, bounds: ViewportRect): MindmapViewportSlice {
  return {
    nodeIndexes: query(index.nodes, index.cellSize, bounds),
    routeIndexes: query(index.routes, index.cellSize, bounds),
    summaryIndexes: query(index.summaries, index.cellSize, bounds),
    boundaryIndexes: query(index.boundaries, index.cellSize, bounds),
    formulaIndexes: query(index.formulas, index.cellSize, bounds),
  };
}

function indexRects(entries: readonly IndexedRect[], cellSize: number): BucketIndex {
  const buckets = new Map<string, number[]>();
  entries.forEach((entry, index) => {
    forEachCell(entry, cellSize, (key) => {
      const bucket = buckets.get(key);
      if (bucket) bucket.push(index);
      else buckets.set(key, [index]);
    });
  });
  return { entries, buckets };
}

function query(index: BucketIndex, cellSize: number, bounds: ViewportRect): ReadonlySet<number> {
  const candidates = new Set<number>();
  forEachCell(bounds, cellSize, (key) => index.buckets.get(key)?.forEach((value) => candidates.add(value)));
  const matches = new Set<number>();
  candidates.forEach((indexValue) => {
    if (intersects(bounds, index.entries[indexValue]!)) matches.add(index.entries[indexValue]!.index);
  });
  return matches;
}

function forEachCell(rect: ViewportRect, cellSize: number, visit: (key: string) => void) {
  const minX = Math.floor(rect.x / cellSize);
  const maxX = Math.floor((rect.x + Math.max(0, rect.width)) / cellSize);
  const minY = Math.floor(rect.y / cellSize);
  const maxY = Math.floor((rect.y + Math.max(0, rect.height)) / cellSize);
  for (let y = minY; y <= maxY; y++) for (let x = minX; x <= maxX; x++) visit(`${x}:${y}`);
}

function pointsRect(points: readonly { x: number; y: number }[], index: number, padding: number): IndexedRect {
  const first = points[0] ?? { x: 0, y: 0 };
  let minX = first.x;
  let maxX = first.x;
  let minY = first.y;
  let maxY = first.y;
  for (const point of points) {
    minX = Math.min(minX, point.x);
    maxX = Math.max(maxX, point.x);
    minY = Math.min(minY, point.y);
    maxY = Math.max(maxY, point.y);
  }
  return { x: minX - padding, y: minY - padding, width: maxX - minX + padding * 2, height: maxY - minY + padding * 2, index };
}

export function intersects(left: ViewportRect, right: ViewportRect): boolean {
  return left.x <= right.x + right.width && left.x + left.width >= right.x && left.y <= right.y + right.height && left.y + left.height >= right.y;
}
