import type { PresentationV5Node, PresentationV5Transform } from "@open-office/schema";

/** View-only interaction primitives for the presentation stage.
 *
 * Keeping this arithmetic outside React makes pointer gestures deterministic
 * and testable without a browser connection.  The returned values are never
 * a writable Deck; callers must still submit semantic commands through the
 * engine boundary.
 */
export type PresentationDragMode = "move" | "resize";

export const MIN_PRESENTATION_NODE_SIZE = 48_000;

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
  minimumSize = MIN_PRESENTATION_NODE_SIZE,
}: {
  nodeIds: readonly string[];
  mode: PresentationDragMode;
  origins: Readonly<Record<string, PresentationV5Transform>>;
  deltaClientX: number;
  deltaClientY: number;
  scale: number;
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
      : index === 0
        ? {
          ...origin,
          width: Math.max(minimumSize, origin.width + deltaX),
          height: Math.max(minimumSize, origin.height + deltaY),
        }
        : origin;
    return [[nodeId, transform]];
  }));
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

export function sameTransform(a: PresentationV5Transform, b: PresentationV5Transform): boolean {
  return a.x === b.x && a.y === b.y && a.width === b.width && a.height === b.height && a.rotation === b.rotation;
}
