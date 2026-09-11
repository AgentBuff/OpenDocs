import type { PresentationV5Node, PresentationV5Transform } from "@open-office/schema";

/** View-only interaction primitives for the presentation stage.
 *
 * Keeping this arithmetic outside React makes pointer gestures deterministic
 * and testable without a browser connection.  The returned values are never
 * a writable Deck; callers must still submit semantic commands through the
 * engine boundary.
 */
export type PresentationResizeHandle = "northWest" | "north" | "northEast" | "east" | "southEast" | "south" | "southWest" | "west";
export type PresentationDragMode = "move" | "resize" | "rotate" | `resize:${PresentationResizeHandle}`;

export const MIN_PRESENTATION_NODE_SIZE = 48_000;

export type PresentationSnapGuide = { axis: "x" | "y"; position: number };

export function nextNodeSelection(current: readonly string[], nodeId: string, extend: boolean): readonly string[] {
  if (!extend) return [nodeId];
  return current.includes(nodeId)
    ? current.filter((candidate) => candidate !== nodeId)
    : [...current, nodeId];
}

export function dragPreview({
  nodeIds,
  mode,
  origins,
  deltaClientX,
  deltaClientY,
  scale,
  rotationDelta = 0,
  minimumSize = MIN_PRESENTATION_NODE_SIZE,
}: {
  nodeIds: readonly string[];
  mode: PresentationDragMode;
  origins: Readonly<Record<string, PresentationV5Transform>>;
  deltaClientX: number;
  deltaClientY: number;
  scale: number;
  rotationDelta?: number;
  minimumSize?: number;
}): Record<string, PresentationV5Transform> {
  if (!Number.isFinite(scale) || scale <= 0) return {};
  const deltaX = deltaClientX / scale;
  const deltaY = deltaClientY / scale;
  return Object.fromEntries(nodeIds.flatMap((nodeId, index) => {
    const origin = origins[nodeId];
    if (!origin) return [];
    const transform = mode === "move"
      ? { ...origin, x: origin.x + deltaX, y: origin.y + deltaY }
      : mode === "rotate" && index === 0
        ? { ...origin, rotation: origin.rotation + rotationDelta }
        : index === 0
        ? resizeTransform(origin, mode === "resize" ? "southEast" : mode.slice(7) as PresentationResizeHandle, deltaX, deltaY, minimumSize)
        : origin;
    return [[nodeId, transform]];
  }));
}

function resizeTransform(
  origin: PresentationV5Transform,
  handle: PresentationResizeHandle,
  deltaX: number,
  deltaY: number,
  minimumSize: number,
): PresentationV5Transform {
  const west = handle === "west" || handle === "northWest" || handle === "southWest";
  const east = handle === "east" || handle === "northEast" || handle === "southEast";
  const north = handle === "north" || handle === "northWest" || handle === "northEast";
  const south = handle === "south" || handle === "southWest" || handle === "southEast";
  const width = west ? Math.max(minimumSize, origin.width - deltaX) : east ? Math.max(minimumSize, origin.width + deltaX) : origin.width;
  const height = north ? Math.max(minimumSize, origin.height - deltaY) : south ? Math.max(minimumSize, origin.height + deltaY) : origin.height;
  return {
    ...origin,
    x: west ? origin.x + origin.width - width : origin.x,
    y: north ? origin.y + origin.height - height : origin.y,
    width,
    height,
  };
}

/** Only changed transforms become engine commands when a pointer gesture ends. */
export function changedNodeTransforms(
  nodes: readonly PresentationV5Node[],
  preview: Readonly<Record<string, PresentationV5Transform>>,
): readonly { nodeId: string; transform: PresentationV5Transform }[] {
  return nodes.flatMap((node) => {
    const transform = preview[node.id];
    return transform && !sameTransform(node.transform, transform)
      ? [{ nodeId: node.id, transform }]
      : [];
  });
}

export function snapMovePreview({
  preview,
  movingNodeIds,
  nodes,
  page,
  threshold,
}: {
  preview: Readonly<Record<string, PresentationV5Transform>>;
  movingNodeIds: readonly string[];
  nodes: readonly PresentationV5Node[];
  page: { width: number; height: number };
  threshold: number;
}): { preview: Record<string, PresentationV5Transform>; guides: readonly PresentationSnapGuide[] } {
  const moving = movingNodeIds.flatMap((id) => {
    const transform = preview[id];
    return transform ? [transform] : [];
  });
  if (moving.length === 0) return { preview: { ...preview }, guides: [] };
  const bounds = {
    left: Math.min(...moving.map((value) => value.x)),
    top: Math.min(...moving.map((value) => value.y)),
    right: Math.max(...moving.map((value) => value.x + value.width)),
    bottom: Math.max(...moving.map((value) => value.y + value.height)),
  };
  const xTargets = [0, page.width / 2, page.width];
  const yTargets = [0, page.height / 2, page.height];
  for (const node of nodes) {
    if (movingNodeIds.includes(node.id) || !node.visible) continue;
    xTargets.push(node.transform.x, node.transform.x + node.transform.width / 2, node.transform.x + node.transform.width);
    yTargets.push(node.transform.y, node.transform.y + node.transform.height / 2, node.transform.y + node.transform.height);
  }
  const xSources = [bounds.left, (bounds.left + bounds.right) / 2, bounds.right];
  const ySources = [bounds.top, (bounds.top + bounds.bottom) / 2, bounds.bottom];
  const xSnap = closestSnap(xSources, xTargets, threshold);
  const ySnap = closestSnap(ySources, yTargets, threshold);
  const next = Object.fromEntries(Object.entries(preview).map(([id, transform]) => [
    id,
    movingNodeIds.includes(id)
      ? { ...transform, x: transform.x + (xSnap?.delta ?? 0), y: transform.y + (ySnap?.delta ?? 0) }
      : transform,
  ]));
  const guides: PresentationSnapGuide[] = [];
  if (xSnap) guides.push({ axis: "x", position: xSnap.target });
  if (ySnap) guides.push({ axis: "y", position: ySnap.target });
  return { preview: next, guides };
}

function closestSnap(sources: readonly number[], targets: readonly number[], threshold: number) {
  let best: { delta: number; target: number } | null = null;
  for (const source of sources) {
    for (const target of targets) {
      const delta = target - source;
      if (Math.abs(delta) <= threshold && (!best || Math.abs(delta) < Math.abs(best.delta))) {
        best = { delta, target };
      }
    }
  }
  return best;
}

export function sameTransform(a: PresentationV5Transform, b: PresentationV5Transform): boolean {
  return a.x === b.x && a.y === b.y && a.width === b.width && a.height === b.height && a.rotation === b.rotation;
}
