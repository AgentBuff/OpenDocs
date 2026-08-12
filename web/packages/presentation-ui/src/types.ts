import type {
  PresentationV5ImageCrop,
  PresentationV5Node,
  PresentationV5NodeKind,
  PresentationV5RichText,
  PresentationV5Transform,
} from "@open-office/schema";
import type { ToolbarDescriptor } from "@open-office/toolbar-core";

export type PresentationNodeType = PresentationV5NodeKind["type"];

/** A stable identity is always slide-scoped; bare node ids are intentionally forbidden. */
export interface PresentationNodeRef {
  slideId: string;
  nodeId: string;
}

export interface PresentationNodeSelection {
  readonly refs: readonly PresentationNodeRef[];
  readonly primary: PresentationNodeRef | null;
  readonly mode: "node" | "text";
}

/**
 * A projection-only context. The registry cannot receive a Deck, engine, Canvas or DOM handle,
 * which prevents a node UI module from creating a second write path.
 */
export interface PresentationNodeContext {
  readonly artifactId: string;
  readonly revision: number;
  readonly slideId: string;
  readonly node: Readonly<PresentationV5Node>;
  /** Child identities from the immutable slide projection, ordered by canonical `orderKey`. */
  readonly childNodeIds: readonly string[];
  readonly selection: PresentationNodeSelection;
  readonly availableCapabilities: ReadonlySet<string>;
}

export type PresentationNodeAdornment =
  | {
      kind: "outline";
      node: PresentationNodeRef;
      bounds: Readonly<PresentationV5Transform>;
      handles: readonly ("northWest" | "northEast" | "southEast" | "southWest" | "rotate")[];
    }
  | { kind: "label"; node: PresentationNodeRef; text: string };

/** Renderer-neutral input for the future Canvas/WebGL stage and DOM text overlay. */
export type PresentationNodeRenderModel =
  | { kind: "shape"; geometry: "rectangle" | "ellipse" | "line" | "arrow" }
  | { kind: "text"; body: Readonly<PresentationV5RichText> }
  | { kind: "image"; assetId: string; crop: Readonly<PresentationV5ImageCrop>; flipH: boolean; flipV: boolean }
  | { kind: "media"; mediaType: "video" | "audio"; assetId: string; posterAssetId: string | null }
  | { kind: "group"; childNodeIds: readonly string[] }
  | { kind: "extension"; namespace: string; version: string; typeId: string; unsupported: boolean; label?: string; summary?: string; reason?: string }
  | { kind: "unsupported"; nodeType: PresentationNodeType; reason: string };

export interface PresentationInspectorField<ActionId extends string> {
  id: string;
  label: string;
  kind: "text" | "number" | "toggle" | "color" | "select" | "readonly";
  action?: ActionId;
  /** A disabled field is explanatory only; it can never be mistaken for a working command. */
  unavailableReason?: string;
}

export interface PresentationInspectorDescriptor<ActionId extends string> {
  id: string;
  title: string;
  fields: readonly PresentationInspectorField<ActionId>[];
}

export interface PresentationNodeToolbarDescriptor<ActionId extends string>
  extends ToolbarDescriptor<ActionId, PresentationNodeContext> {
  capability: string;
}

/**
 * Browser serialization mirror of the canonical Rust `PresentationCommand`. It is only an
 * intent; `PresentationStore` / the server transaction endpoint owns execution and validation.
 */
export type PresentationSemanticCommand =
  | { type: "deleteNode"; slideId: string; nodeId: string }
  | { type: "ungroupNodes"; slideId: string; groupId: string }
  | { type: "setNodeTransform"; slideId: string; nodeId: string; transform: PresentationV5Transform }
  | { type: "setShapeStyle"; slideId: string; nodeId: string; style: Extract<PresentationV5NodeKind, { type: "shape" }>["data"]["style"] }
  | { type: "setTextContent"; slideId: string; nodeId: string; body: PresentationV5RichText }
  | { type: "setTextFrame"; slideId: string; nodeId: string; frame: Extract<PresentationV5NodeKind, { type: "text" }>["data"]["frame"] }
  | { type: "setImageConfig"; slideId: string; nodeId: string; image: Extract<PresentationV5NodeKind, { type: "image" }>["data"] }
  | { type: "setMediaConfig"; slideId: string; nodeId: string; media: Extract<PresentationV5NodeKind, { type: "video" | "audio" }>["data"] };

export interface PresentationActionInvocation {
  readonly context: PresentationNodeContext;
  readonly value?: unknown;
}

export interface PresentationNodeRegistration<ActionId extends string> {
  readonly type: PresentationNodeType;
  readonly renderer: (context: PresentationNodeContext) => PresentationNodeRenderModel;
  readonly selectionAdornment: (context: PresentationNodeContext) => readonly PresentationNodeAdornment[];
  readonly toolbar: readonly PresentationNodeToolbarDescriptor<ActionId>[];
  readonly inspector: PresentationInspectorDescriptor<ActionId>;
  /** Returns semantic commands only; it must not mutate a Deck or view state. */
  readonly mapAction: (action: ActionId, invocation: PresentationActionInvocation) => readonly PresentationSemanticCommand[];
}

export interface ResolvedPresentationNodeUi<ActionId extends string> {
  readonly registration: PresentationNodeRegistration<ActionId>;
  readonly renderModel: PresentationNodeRenderModel;
  readonly adornments: readonly PresentationNodeAdornment[];
  readonly toolbar: readonly PresentationNodeToolbarDescriptor<ActionId>[];
  readonly inspector: PresentationInspectorDescriptor<ActionId>;
}
