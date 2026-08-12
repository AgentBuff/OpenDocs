import { createArtifactToolbarAdapter, type ArtifactToolbarContext, type ArtifactToolbarDescriptor, type ArtifactToolbarAdapter } from "./types.js";

export const MINDMAP_TOOLBAR_NAMESPACE = "mindmap" as const;

/** Capability keys mirror the server catalog and are intentionally semantic;
 * renderers decide icon/layout while the engine owns command execution. */
export const MINDMAP_TOOLBAR_CAPABILITIES = [
  "mindmap.addNode",
  "mindmap.updateNode",
  "mindmap.moveNode",
  "mindmap.deleteNode",
  "mindmap.setNodeCollapsed",
  "mindmap.addEdge",
  "mindmap.updateEdge",
  "mindmap.deleteEdge",
] as const;

export type MindmapSelection =
  | { kind: "canvas" }
  | { kind: "node"; nodeIds: readonly string[] }
  | { kind: "edge"; edgeIds: readonly string[] };
export type MindmapToolbarContext = ArtifactToolbarContext<MindmapSelection>;
export type MindmapToolbarDescriptor<ActionId extends string = string> = ArtifactToolbarDescriptor<ActionId, MindmapToolbarContext>;
export type MindmapToolbarAdapter<ActionId extends string = string> = ArtifactToolbarAdapter<ActionId, MindmapToolbarContext>;

export function createMindmapToolbarAdapter<ActionId extends string = string>(
  descriptors: readonly MindmapToolbarDescriptor<ActionId>[] = [],
): MindmapToolbarAdapter<ActionId> {
  return createArtifactToolbarAdapter(MINDMAP_TOOLBAR_NAMESPACE, descriptors);
}
