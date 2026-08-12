import { createArtifactToolbarAdapter, type ArtifactToolbarContext, type ArtifactToolbarDescriptor, type ArtifactToolbarAdapter } from "./types.js";

export const WHITEBOARD_TOOLBAR_NAMESPACE = "whiteboard" as const;

export const WHITEBOARD_TOOLBAR_CAPABILITIES = [
  "whiteboard.addElement",
  "whiteboard.updateElement",
  "whiteboard.deleteElement",
  "whiteboard.connectElements",
] as const;

export type WhiteboardSelection =
  | { kind: "canvas" }
  | { kind: "element"; elementIds: readonly string[] }
  | { kind: "marquee"; elementIds: readonly string[] };
export type WhiteboardToolbarContext = ArtifactToolbarContext<WhiteboardSelection>;
export type WhiteboardToolbarDescriptor<ActionId extends string = string> = ArtifactToolbarDescriptor<ActionId, WhiteboardToolbarContext>;
export type WhiteboardToolbarAdapter<ActionId extends string = string> = ArtifactToolbarAdapter<ActionId, WhiteboardToolbarContext>;

export function createWhiteboardToolbarAdapter<ActionId extends string = string>(
  descriptors: readonly WhiteboardToolbarDescriptor<ActionId>[] = [],
): WhiteboardToolbarAdapter<ActionId> {
  return createArtifactToolbarAdapter(WHITEBOARD_TOOLBAR_NAMESPACE, descriptors);
}
