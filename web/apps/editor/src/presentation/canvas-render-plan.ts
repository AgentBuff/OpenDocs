import type { PresentationV5Node, PresentationV5Transform } from "@open-office/schema";

/**
 * Renderer-local bookkeeping for the Canvas backing layer.  It is derived
 * from an immutable slide projection and pointer preview only; it must never
 * be persisted or sent through a document transaction.
 */
export interface CanvasRenderSnapshot {
  readonly width: number;
  readonly height: number;
  readonly scale: number;
  readonly nodes: ReadonlyMap<string, CanvasRenderNodeSnapshot>;
}

export interface CanvasRenderNodeSnapshot {
  readonly signature: string;
  readonly bounds: CanvasRect;
}

export interface CanvasRect {
  readonly x: number;
  readonly y: number;
  readonly width: number;
  readonly height: number;
}

export interface CanvasRenderPlan {
  readonly snapshot: CanvasRenderSnapshot;
  readonly kind: "noop" | "partial" | "full";
  /** Nodes that need repainting after the dirty rectangle was cleared. */
  readonly nodes: readonly PresentationV5Node[];
  readonly dirtyRect: CanvasRect | null;
}

const MAX_PARTIAL_INVALIDATIONS = 16;
const PARTIAL_INVALIDATION_RATIO = 0.4;

/**
 * Produces a conservative retained-rendering plan. Rotated bounds use the
 * enclosing axis-aligned rectangle, so clearing a dirty region cannot leave
 * old pixels behind. A large or structural change deliberately falls back to
 * a full repaint rather than maintaining a second scene graph.
 */
export function deriveCanvasRenderPlan({
  previous,
  nodes,
  preview,
  width,
  height,
  scale,
}: {
  readonly previous: CanvasRenderSnapshot | null;
  readonly nodes: readonly PresentationV5Node[];
  readonly preview: Readonly<Record<string, PresentationV5Transform>>;
  readonly width: number;
  readonly height: number;
  readonly scale: number;
}): CanvasRenderPlan {
  const snapshot = createCanvasRenderSnapshot(nodes, preview, width, height, scale);
  if (!previous || previous.width !== width || previous.height !== height || previous.scale !== scale) {
    return { snapshot, kind: "full", nodes, dirtyRect: { x: 0, y: 0, width, height } };
  }

  const changedIds = new Set<string>();
  for (const [id, current] of snapshot.nodes) {
    if (previous.nodes.get(id)?.signature !== current.signature) changedIds.add(id);
  }
  for (const id of previous.nodes.keys()) {
    if (!snapshot.nodes.has(id)) changedIds.add(id);
  }
  if (changedIds.size === 0) return { snapshot, kind: "noop", nodes: [], dirtyRect: null };
  if (changedIds.size > MAX_PARTIAL_INVALIDATIONS || changedIds.size > Math.max(1, nodes.length * PARTIAL_INVALIDATION_RATIO)) {
    return { snapshot, kind: "full", nodes, dirtyRect: { x: 0, y: 0, width, height } };
  }

  let dirtyRect: CanvasRect | null = null;
  for (const id of changedIds) {
    const previousBounds = previous.nodes.get(id)?.bounds;
    const currentBounds = snapshot.nodes.get(id)?.bounds;
    if (previousBounds) dirtyRect = unionRect(dirtyRect, previousBounds);
    if (currentBounds) dirtyRect = unionRect(dirtyRect, currentBounds);
  }
  const clippedDirtyRect = dirtyRect ? clipRect(dirtyRect, width, height) : null;
  if (!clippedDirtyRect) return { snapshot, kind: "partial", nodes: [], dirtyRect: null };
  return {
    snapshot,
    kind: "partial",
    dirtyRect: clippedDirtyRect,
    nodes: nodes.filter((node) => node.visible && intersects(snapshot.nodes.get(node.id)?.bounds, clippedDirtyRect)),
  };
}

function createCanvasRenderSnapshot(
  nodes: readonly PresentationV5Node[],
  preview: Readonly<Record<string, PresentationV5Transform>>,
  width: number,
  height: number,
  scale: number,
): CanvasRenderSnapshot {
  const snapshots = new Map<string, CanvasRenderNodeSnapshot>();
  for (const node of nodes) {
    const transform = preview[node.id] ?? node.transform;
    snapshots.set(node.id, {
      signature: JSON.stringify({ visible: node.visible, opacity: node.opacity, kind: node.kind, transform }),
      bounds: transformBounds(transform, scale),
    });
  }
  return { width, height, scale, nodes: snapshots };
}

function transformBounds(transform: PresentationV5Transform, scale: number): CanvasRect {
  const width = transform.width * scale;
  const height = transform.height * scale;
  const radians = transform.rotation * Math.PI / 180;
  const cosine = Math.abs(Math.cos(radians));
  const sine = Math.abs(Math.sin(radians));
  const rotatedWidth = width * cosine + height * sine;
  const rotatedHeight = width * sine + height * cosine;
  const centerX = (transform.x * scale) + width / 2;
  const centerY = (transform.y * scale) + height / 2;
  // One physical pixel guards against anti-aliased edges at a rotation.
  return { x: centerX - rotatedWidth / 2 - 1, y: centerY - rotatedHeight / 2 - 1, width: rotatedWidth + 2, height: rotatedHeight + 2 };
}

function unionRect(left: CanvasRect | null, right: CanvasRect): CanvasRect {
  if (!left) return right;
  const x = Math.min(left.x, right.x);
  const y = Math.min(left.y, right.y);
  const maxX = Math.max(left.x + left.width, right.x + right.width);
  const maxY = Math.max(left.y + left.height, right.y + right.height);
  return { x, y, width: maxX - x, height: maxY - y };
}

function clipRect(rect: CanvasRect, width: number, height: number): CanvasRect | null {
  const x = Math.max(0, rect.x);
  const y = Math.max(0, rect.y);
  const maxX = Math.min(width, rect.x + rect.width);
  const maxY = Math.min(height, rect.y + rect.height);
  return maxX > x && maxY > y ? { x, y, width: maxX - x, height: maxY - y } : null;
}

function intersects(bounds: CanvasRect | undefined, dirtyRect: CanvasRect): boolean {
  return Boolean(bounds && bounds.x < dirtyRect.x + dirtyRect.width && bounds.x + bounds.width > dirtyRect.x && bounds.y < dirtyRect.y + dirtyRect.height && bounds.y + bounds.height > dirtyRect.y);
}
