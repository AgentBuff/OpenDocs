import { createArtifactToolbarAdapter, type ArtifactToolbarContext, type ArtifactToolbarDescriptor, type ArtifactToolbarAdapter } from "./types.js";

export const PRESENTATION_TOOLBAR_NAMESPACE = "presentation" as const;

export type PresentationSelection =
  | { kind: "slide"; slideId: string }
  | { kind: "element"; slideId: string; elementIds: readonly string[] };
export type PresentationToolbarContext = ArtifactToolbarContext<PresentationSelection>;
export type PresentationToolbarDescriptor<ActionId extends string = string> = ArtifactToolbarDescriptor<ActionId, PresentationToolbarContext>;
export type PresentationToolbarAdapter<ActionId extends string = string> = ArtifactToolbarAdapter<ActionId, PresentationToolbarContext>;

/** Semantic presentation actions exposed by the scene-graph runtime. The adapter remains
 * descriptor-driven so a host may hide capabilities that are not available in its build. */
export type PresentationToolbarAction =
  | "slide.add"
  | "slide.delete"
  | "slide.notes"
  | "element.insert"
  | "element.text"
  | "element.style"
  | "element.group"
  | "element.ungroup"
  | "element.reorder"
  | "presentation.theme"
  | "presentation.animation";

export const PRESENTATION_CAPABILITIES = Object.freeze([
  "presentation.slide.add",
  "presentation.slide.delete",
  "presentation.slide.notes",
  "presentation.element.insert",
  "presentation.element.text",
  "presentation.element.style",
  "presentation.element.group",
  "presentation.element.ungroup",
  "presentation.element.reorder",
  "presentation.theme",
  "presentation.animation",
] as const);

export function createPresentationToolbarAdapter<ActionId extends string = string>(
  descriptors: readonly PresentationToolbarDescriptor<ActionId>[] = [],
): PresentationToolbarAdapter<ActionId> {
  return createArtifactToolbarAdapter(PRESENTATION_TOOLBAR_NAMESPACE, descriptors);
}
