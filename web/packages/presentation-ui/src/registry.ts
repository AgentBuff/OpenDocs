import type { PresentationV5NodeKind } from "@open-office/schema";
import type {
  PresentationActionInvocation,
  PresentationNodeContext,
  PresentationNodeRegistration,
  PresentationNodeType,
  ResolvedPresentationNodeUi,
} from "./types.js";

const ALL_NODE_TYPES = [
  "shape", "text", "image", "video", "audio", "table", "chart", "connector", "group", "embed", "extension",
] as const satisfies readonly PresentationNodeType[];

export const PRESENTATION_NODE_TYPES = ALL_NODE_TYPES;

/**
 * PresentationNodeRegistry composes node UI from isolated registrations. It deliberately has no
 * `switch(node.kind.type)` in consumers and never exposes a mutable Deck. Unsupported node types
 * resolve to an explicit read-only registration instead of fake toolbar actions.
 */
export class PresentationNodeRegistry<ActionId extends string> {
  private readonly registrations = new Map<PresentationNodeType, PresentationNodeRegistration<ActionId>>();

  constructor(registrations: readonly PresentationNodeRegistration<ActionId>[] = []) {
    registrations.forEach((registration) => this.register(registration));
  }

  register(registration: PresentationNodeRegistration<ActionId>): this {
    if (this.registrations.has(registration.type)) {
      throw new Error(`presentation node registration already exists: ${registration.type}`);
    }
    validateRegistration(registration);
    this.registrations.set(registration.type, freezeRegistration(registration));
    return this;
  }

  has(type: PresentationNodeType): boolean {
    return this.registrations.has(type);
  }

  resolve(context: PresentationNodeContext): ResolvedPresentationNodeUi<ActionId> {
    const registration = this.registrations.get(context.node.kind.type) ?? createUnsupportedRegistration<ActionId>(context.node.kind.type);
    const toolbar = registration.toolbar.filter((item) => isAvailable(item.capability, context));
    const enabledActions = new Set(toolbar.flatMap((item) => item.action ? [item.action] : []));
    return {
      registration,
      renderModel: registration.renderer(context),
      adornments: registration.selectionAdornment(context),
      toolbar,
      // Inspector controls follow the same capability gate as the toolbar.
      // A deployment that does not advertise a command gets an explanatory
      // read-only field, never an optimistic control that cannot commit.
      inspector: {
        ...registration.inspector,
        fields: registration.inspector.fields.map((field) => field.action && !enabledActions.has(field.action)
          ? {
            ...field,
            kind: "readonly" as const,
            action: undefined,
            unavailableReason: `当前服务未声明 ${field.action} 能力`,
          }
          : field),
      },
    };
  }

  mapAction(action: ActionId, invocation: PresentationActionInvocation) {
    const registration = this.registrations.get(invocation.context.node.kind.type)
      ?? createUnsupportedRegistration<ActionId>(invocation.context.node.kind.type);
    const descriptor = registration.toolbar.find((item) => item.action === action);
    if (!descriptor) {
      throw new Error(`presentation node action is not registered for ${invocation.context.node.kind.type}: ${action}`);
    }
    if (!isAvailable(descriptor.capability, invocation.context)) {
      throw new Error(`presentation node capability is unavailable: ${descriptor.capability}`);
    }
    return registration.mapAction(action, invocation);
  }
}

function isAvailable(capability: string, context: PresentationNodeContext): boolean {
  return context.availableCapabilities.has(capability);
}

function validateRegistration<ActionId extends string>(registration: PresentationNodeRegistration<ActionId>): void {
  const ids = new Set<string>();
  const actions = new Set<ActionId>();
  for (const item of registration.toolbar) {
    if (!item.id.trim() || ids.has(item.id)) throw new Error(`presentation node toolbar id must be unique: ${item.id || "<missing>"}`);
    ids.add(item.id);
    if (!item.capability.startsWith("presentation.")) throw new Error(`presentation node toolbar capability must use presentation namespace: ${item.capability}`);
    if (item.kind !== "separator" && !item.action && !item.children?.length) {
      throw new Error(`presentation node toolbar action required: ${item.id}`);
    }
    if (item.action) actions.add(item.action);
  }
  for (const field of registration.inspector.fields) {
    if (field.action && !actions.has(field.action)) {
      throw new Error(`presentation inspector action must have a registered toolbar command: ${field.action}`);
    }
    if (field.action === undefined && field.kind !== "readonly" && !field.unavailableReason) {
      throw new Error(`presentation inspector field needs action or unavailable reason: ${field.id}`);
    }
  }
}

function freezeRegistration<ActionId extends string>(registration: PresentationNodeRegistration<ActionId>): PresentationNodeRegistration<ActionId> {
  return Object.freeze({
    ...registration,
    toolbar: Object.freeze([...registration.toolbar]),
    inspector: Object.freeze({ ...registration.inspector, fields: Object.freeze([...registration.inspector.fields]) }),
  });
}

function createUnsupportedRegistration<ActionId extends string>(type: PresentationV5NodeKind["type"]): PresentationNodeRegistration<ActionId> {
  const reason = `${type} 节点尚未接入 Presentation 编辑器；不会提供不可执行的工具栏按钮`;
  return {
    type,
    renderer: () => ({ kind: "unsupported", nodeType: type, reason }),
    selectionAdornment: () => [],
    toolbar: [],
    inspector: {
      id: `presentation.node.${type}.unsupported`,
      title: "暂不支持的节点",
      fields: [{ id: "availability", label: reason, kind: "readonly", unavailableReason: reason }],
    },
    mapAction: () => [],
  };
}
