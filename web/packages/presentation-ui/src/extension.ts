import type { PresentationV5ExtensionPayload, PresentationV5Node } from "@open-office/schema";
import type { PresentationInspectorField, PresentationNodeContext } from "./types.js";

type ExtensionNode = PresentationV5Node & {
  kind: { type: "extension"; data: { namespace: string; version: string } & PresentationV5ExtensionPayload };
};

/**
 * The only data an extension renderer may receive.  In particular it gets no
 * Deck, transport client, command mapper or mutable view state.  Extension
 * modules are therefore projection consumers; all writes remain host-owned
 * typed PresentationCommand transactions.
 */
export interface PresentationExtensionRenderContext {
  readonly artifactId: string;
  readonly revision: number;
  readonly slideId: string;
  readonly node: Readonly<ExtensionNode>;
  readonly payload: Readonly<PresentationV5ExtensionPayload>;
}

/** Renderer-neutral and serializable extension output; extensions do not return DOM or callbacks. */
export interface PresentationExtensionRenderModel {
  readonly label: string;
  readonly summary?: string;
}

/** Extension inspectors are explanatory data only. They cannot expose an action id. */
export interface PresentationExtensionInspector {
  readonly id: string;
  readonly title: string;
  readonly fields: readonly PresentationExtensionInspectorField[];
}

export type PresentationExtensionInspectorField = Omit<PresentationInspectorField<never>, "action" | "kind" | "unavailableReason"> & {
  readonly kind?: "readonly";
  readonly value?: string;
};

export interface PresentationExtensionRegistration {
  /** Stable plugin identity. Version matching is exact; migrations are explicit. */
  readonly namespace: string;
  readonly version: string;
  readonly typeId: string;
  /** Host-granted capability, never a command or generic patch permission. */
  readonly capability: string;
  readonly renderer: (context: PresentationExtensionRenderContext) => PresentationExtensionRenderModel;
  readonly inspector: PresentationExtensionInspector;
}

export type ResolvedPresentationExtension =
  | { readonly status: "registered"; readonly registration: PresentationExtensionRegistration; readonly renderModel: PresentationExtensionRenderModel; readonly inspector: PresentationExtensionInspector }
  | { readonly status: "unavailable"; readonly reason: string };

/**
 * Host-side manifest registry for extension renderers. It has deliberately no
 * action mapper, store or client: registering a renderer cannot create a new
 * write path into a presentation Deck.
 */
export class PresentationExtensionRegistry {
  private readonly registrations = new Map<string, PresentationExtensionRegistration>();

  constructor(registrations: readonly PresentationExtensionRegistration[] = []) {
    registrations.forEach((registration) => this.register(registration));
  }

  register(registration: PresentationExtensionRegistration): this {
    validateExtensionRegistration(registration);
    const key = keyFor(registration.namespace, registration.version, registration.typeId);
    if (this.registrations.has(key)) throw new Error(`presentation extension registration already exists: ${key}`);
    this.registrations.set(key, Object.freeze({
      ...registration,
      inspector: Object.freeze({ ...registration.inspector, fields: Object.freeze([...registration.inspector.fields]) }),
    }));
    return this;
  }

  resolve(context: PresentationNodeContext): ResolvedPresentationExtension {
    if (context.node.kind.type !== "extension") throw new Error(`extension registry requires extension node, received ${context.node.kind.type}`);
    const { namespace, version, typeId } = context.node.kind.data;
    const registration = this.registrations.get(keyFor(namespace, version, typeId));
    if (!registration) return { status: "unavailable", reason: `未注册扩展：${namespace}@${version}/${typeId}` };
    if (!context.availableCapabilities.has(registration.capability)) {
      return { status: "unavailable", reason: `当前环境未授予扩展能力：${registration.capability}` };
    }
    const node = context.node as ExtensionNode;
    const payload: PresentationV5ExtensionPayload = { typeId, data: node.kind.data.data };
    return {
      status: "registered",
      registration,
      renderModel: registration.renderer({ artifactId: context.artifactId, revision: context.revision, slideId: context.slideId, node, payload }),
      inspector: registration.inspector,
    };
  }
}

function keyFor(namespace: string, version: string, typeId: string): string {
  return `${namespace}\u0000${version}\u0000${typeId}`;
}

function validateExtensionRegistration(registration: PresentationExtensionRegistration): void {
  for (const [name, value] of [["namespace", registration.namespace], ["version", registration.version], ["typeId", registration.typeId], ["capability", registration.capability]] as const) {
    if (!value.trim()) throw new Error(`presentation extension ${name} must be non-empty`);
  }
  if (!registration.capability.startsWith("presentation.extension.")) {
    throw new Error(`presentation extension capability must use presentation.extension namespace: ${registration.capability}`);
  }
  for (const field of registration.inspector.fields) {
    if (!field.id.trim() || !field.label.trim()) throw new Error("presentation extension inspector field must have id and label");
    if (field.kind !== undefined && field.kind !== "readonly") throw new Error("presentation extension inspector is read-only");
  }
}
