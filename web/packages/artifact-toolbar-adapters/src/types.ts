import {
  resolveToolbar,
  validateToolbar,
  type ResolvedToolbarDescriptor,
  type ToolbarDescriptor,
  type ToolbarResolutionContext,
} from "@open-office/toolbar-core";

export type ArtifactToolbarKind = "spreadsheet" | "presentation" | "mindmap" | "whiteboard";

/**
 * Selection is intentionally a small, read-only view contract. Adapters do not receive an
 * Artifact model or an engine reference; a product shell supplies only the state needed to
 * resolve capability visibility and active/enabled predicates.
 */
export interface ArtifactToolbarContext<Selection = unknown> extends ToolbarResolutionContext {
  artifactId: string;
  revision: number;
  availableCapabilities: ReadonlySet<string>;
  selection: Selection | null;
}

export interface ArtifactToolbarDescriptor<ActionId extends string, Context extends ArtifactToolbarContext> extends Omit<ToolbarDescriptor<ActionId, Context>, "children"> {
  /** Registry key owned by the artifact command/capability layer. */
  capability: string;
  children?: readonly ArtifactToolbarDescriptor<ActionId, Context>[];
}

export interface ArtifactToolbarAdapter<ActionId extends string, Context extends ArtifactToolbarContext> {
  readonly artifact: ArtifactToolbarKind;
  /** Source descriptors remain immutable and retain capability metadata for inspection. */
  readonly descriptors: readonly ArtifactToolbarDescriptor<ActionId, Context>[];
  /** Core descriptors are safe to pass to toolbar-react; unavailable capabilities resolve away. */
  readonly toolbar: readonly ToolbarDescriptor<ActionId, Context>[];
  resolve(context: Context): ResolvedToolbarDescriptor<ActionId>[];
}

function evaluateVisibility<Context extends ArtifactToolbarContext>(
  descriptor: ArtifactToolbarDescriptor<string, Context>,
  context: Context,
): boolean {
  if (!context.availableCapabilities.has(descriptor.capability)) return false;
  if (descriptor.visible === undefined) return true;
  return typeof descriptor.visible === "function" ? descriptor.visible(context) : descriptor.visible;
}

function toCoreDescriptor<ActionId extends string, Context extends ArtifactToolbarContext>(
  descriptor: ArtifactToolbarDescriptor<ActionId, Context>,
): ToolbarDescriptor<ActionId, Context> {
  const { capability, children, visible, ...item } = descriptor;
  const core: ToolbarDescriptor<ActionId, Context> = {
    ...item,
    visible: (context) => evaluateVisibility({ ...descriptor, visible }, context),
  };
  if (children !== undefined) core.children = children.map(toCoreDescriptor);
  return core;
}

function validateCapabilityTree<ActionId extends string, Context extends ArtifactToolbarContext>(
  artifact: ArtifactToolbarKind,
  descriptors: readonly ArtifactToolbarDescriptor<ActionId, Context>[],
): void {
  const prefix = `${artifact}.`;
  const capabilities = new Set<string>();
  const visit = (descriptor: ArtifactToolbarDescriptor<ActionId, Context>): void => {
    const capability = descriptor.capability.trim();
    if (!capability) throw new Error(`toolbar capability is required: ${descriptor.id || "<missing>"}`);
    if (!capability.startsWith(prefix)) {
      throw new Error(`toolbar capability must use ${prefix} namespace: ${capability}`);
    }
    if (capabilities.has(capability)) throw new Error(`duplicate toolbar capability: ${capability}`);
    capabilities.add(capability);
    descriptor.children?.forEach(visit);
  };
  descriptors.forEach(visit);
}

/**
 * Creates a framework-free adapter. No descriptors are supplied by default: a capability is
 * only rendered after its real Artifact command registry advertises the matching key.
 */
export function createArtifactToolbarAdapter<ActionId extends string, Context extends ArtifactToolbarContext>(
  artifact: ArtifactToolbarKind,
  descriptors: readonly ArtifactToolbarDescriptor<ActionId, Context>[],
): ArtifactToolbarAdapter<ActionId, Context> {
  validateCapabilityTree(artifact, descriptors);
  const toolbar = descriptors.map(toCoreDescriptor);
  const errors = validateToolbar(toolbar);
  if (errors.length > 0) throw new Error(`invalid ${artifact} toolbar descriptor: ${errors.join("; ")}`);
  return {
    artifact,
    descriptors,
    toolbar,
    resolve: (context) => resolveToolbar(toolbar, context),
  };
}
