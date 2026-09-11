import { createBuiltinPresentationNodeRegistry, type PresentationNodeContext } from "@open-office/presentation-ui";
import type { PresentationV5Node } from "@open-office/schema";
import type { PresentationSlideProjection } from "@open-office/schema/api";

export const presentationNodeUiRegistry = createBuiltinPresentationNodeRegistry();

export function createNodeContext({
  artifactId,
  revision,
  slide,
  node,
  selectedNodeIds,
  mode,
  availableCapabilities,
}: {
  artifactId: string;
  revision: number;
  slide: PresentationSlideProjection;
  node: PresentationV5Node;
  selectedNodeIds: readonly string[];
  mode: "node" | "text";
  availableCapabilities: ReadonlySet<string>;
}): PresentationNodeContext {
  const nodes = slide.nodes ?? [];
  return {
    artifactId,
    revision,
    slideId: slide.slideId,
    node,
    childNodeIds: nodes
      .filter((candidate) => candidate.parentId === node.id)
      .sort((left, right) => left.orderKey.localeCompare(right.orderKey))
      .map((candidate) => candidate.id),
    selection: selectedNodeIds.length
      ? {
        refs: selectedNodeIds.map((nodeId) => ({ slideId: slide.slideId, nodeId })),
        primary: selectedNodeIds.length ? { slideId: slide.slideId, nodeId: selectedNodeIds[selectedNodeIds.length - 1]! } : null,
        mode,
      }
      : { refs: [], primary: null, mode: "node" },
    availableCapabilities,
  };
}

