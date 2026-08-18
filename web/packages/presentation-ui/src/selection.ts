import type { PresentationV5Node } from "@open-office/schema";
import type { PresentationControlSurface, PresentationSelectionRequirement } from "./capabilities.js";

/**
 * Multi-object controls are a separate surface from a node's own toolbar.
 * They operate only on stable selected ids and each descriptor owns a real
 * server capability; node renderers never receive this responsibility.
 */
export type PresentationMultiSelectionAction =
  | "selection.alignLeft"
  | "selection.alignCenter"
  | "selection.alignRight"
  | "selection.alignTop"
  | "selection.alignMiddle"
  | "selection.alignBottom"
  | "selection.distributeHorizontal"
  | "selection.distributeVertical"
  | "selection.bringForward"
  | "selection.sendBackward"
  | "selection.bringToFront"
  | "selection.sendToBack";

type MultiSelectionCapability =
  | "presentation.alignNodes"
  | "presentation.distributeNodes"
  | "presentation.reorderNode";

export interface PresentationMultiSelectionControl {
  readonly action: PresentationMultiSelectionAction;
  readonly capability: MultiSelectionCapability;
  readonly label: string;
  readonly icon: string;
  readonly minimumSelection: 2 | 3;
}

const MULTI_SELECTION_CONTROLS: readonly PresentationMultiSelectionControl[] = [
  { action: "selection.alignLeft", capability: "presentation.alignNodes", label: "左对齐", icon: "align-left", minimumSelection: 2 },
  { action: "selection.alignCenter", capability: "presentation.alignNodes", label: "水平居中", icon: "align-center", minimumSelection: 2 },
  { action: "selection.alignRight", capability: "presentation.alignNodes", label: "右对齐", icon: "align-right", minimumSelection: 2 },
  { action: "selection.alignTop", capability: "presentation.alignNodes", label: "顶端对齐", icon: "align-top", minimumSelection: 2 },
  { action: "selection.alignMiddle", capability: "presentation.alignNodes", label: "垂直居中", icon: "align-middle", minimumSelection: 2 },
  { action: "selection.alignBottom", capability: "presentation.alignNodes", label: "底端对齐", icon: "align-bottom", minimumSelection: 2 },
  { action: "selection.distributeHorizontal", capability: "presentation.distributeNodes", label: "横向分布", icon: "distribute-horizontal", minimumSelection: 3 },
  { action: "selection.distributeVertical", capability: "presentation.distributeNodes", label: "纵向分布", icon: "distribute-vertical", minimumSelection: 3 },
  { action: "selection.bringForward", capability: "presentation.reorderNode", label: "上移一层", icon: "bring-forward", minimumSelection: 2 },
  { action: "selection.sendBackward", capability: "presentation.reorderNode", label: "下移一层", icon: "send-backward", minimumSelection: 2 },
  { action: "selection.bringToFront", capability: "presentation.reorderNode", label: "置于顶层", icon: "bring-front", minimumSelection: 2 },
  { action: "selection.sendToBack", capability: "presentation.reorderNode", label: "置于底层", icon: "send-back", minimumSelection: 2 },
];

export function resolveMultiSelectionControls(
  availableCapabilities: ReadonlySet<string>,
  selected: number | readonly PresentationV5Node[],
): readonly PresentationMultiSelectionControl[] {
  const selectedCount = typeof selected === "number" ? selected : selected.length;
  // The numeric overload keeps this tiny pure resolver usable by generic
  // callers. Studio passes real nodes so locked, unsupported, and mixed-parent
  // selections cannot accidentally look actionable.
  const nodes = typeof selected === "number" ? null : selected;
  const compatible = !nodes || areCompatibleForMultiObjectArrange(nodes);
  return MULTI_SELECTION_CONTROLS.filter((control) =>
    compatible && selectedCount >= control.minimumSelection && availableCapabilities.has(control.capability),
  );
}

/**
 * Alignment, distribution and z-order all operate on one stable sibling
 * collection. A locked or renderer-unsupported object has no safe common
 * mutation surface, therefore the contextual surface exposes the intersection
 * (nothing) instead of partially applying a multi-object action.
 */
function areCompatibleForMultiObjectArrange(nodes: readonly PresentationV5Node[]): boolean {
  if (nodes.length < 2) return false;
  const parentId = nodes[0]?.parentId ?? null;
  return nodes.every((node) =>
    !node.locked
    && node.parentId === parentId
    && node.kind.type !== "extension",
  );
}

export interface PresentationSelectionSnapshot {
  readonly slideId: string | null;
  readonly selectedNodes: readonly PresentationV5Node[];
  readonly textEditingNodeId: string | null;
}

/**
 * Derives UI state from immutable projection plus ephemeral selection only.
 * It must stay free of React, DOM Range and transaction concerns so every
 * toolbar, inspector and context menu makes the same visibility decision.
 */
export function selectionRequirement(snapshot: PresentationSelectionSnapshot): PresentationSelectionRequirement {
  if (snapshot.textEditingNodeId !== null) return "text-editing";
  if (snapshot.selectedNodes.length > 1) return "multi-node";
  if (snapshot.selectedNodes.length === 1) return "single-node";
  return snapshot.slideId === null ? "none" : "slide";
}

export function selectionSurfaces(snapshot: PresentationSelectionSnapshot): readonly PresentationControlSurface[] {
  const requirement = selectionRequirement(snapshot);
  if (requirement === "none") return ["global"];
  if (requirement === "slide") return ["global", "insert", "slide", "timeline"];
  if (requirement === "multi-node") return ["global", "node"];
  const node = snapshot.selectedNodes[0];
  if (!node) return ["global"];
  if (requirement === "text-editing") return ["global", "text"];
  switch (node.kind.type) {
    case "text": return ["global", "node", "text"];
    case "shape": return ["global", "node", "shape"];
    case "image": return ["global", "node", "image"];
    case "video":
    case "audio": return ["global", "node", "media"];
    default: return ["global", "node"];
  }
}

export function selectionNodeKinds(snapshot: PresentationSelectionSnapshot): readonly PresentationV5Node["kind"]["type"][] {
  return [...new Set(snapshot.selectedNodes.map((node) => node.kind.type))];
}
