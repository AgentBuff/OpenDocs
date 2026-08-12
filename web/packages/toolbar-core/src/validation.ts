import type { ToolbarDescriptor, ToolbarResolutionContext } from "./descriptor.js";

/** Development-time contract check; it never invents a capability or fallback action. */
export function validateToolbar<ActionId extends string, C extends ToolbarResolutionContext>(descriptors: readonly ToolbarDescriptor<ActionId, C>[]): string[] {
  const errors: string[] = [];
  const ids = new Set<string>();

  const visit = (descriptor: ToolbarDescriptor<ActionId, C>): void => {
    const id = descriptor.id.trim();
    const displayId = id || "<missing>";
    if (!id) errors.push("toolbar item id is required");
    if (ids.has(id)) errors.push(`duplicate toolbar item id: ${displayId}`);
    if (id) ids.add(id);

    if (!descriptor.group.trim()) errors.push(`toolbar item group is required: ${displayId}`);
    if (descriptor.kind !== "separator" && !descriptor.label?.trim() && !descriptor.ariaLabel?.trim()) {
      errors.push(`toolbar item requires label or ariaLabel: ${displayId}`);
    }

    const interactive = descriptor.kind !== "separator";
    if (interactive && !descriptor.action && !descriptor.children?.length) {
      errors.push(`interactive toolbar item requires action or children: ${displayId}`);
    }
    if (descriptor.kind === "separator" && (descriptor.action || descriptor.children?.length)) {
      errors.push(`separator cannot define action or children: ${displayId}`);
    }
    if (descriptor.children?.length && descriptor.kind !== "menu") {
      errors.push(`toolbar children require menu kind: ${displayId}`);
    }

    for (const [name, value] of [["priority", descriptor.priority], ["overflowPriority", descriptor.overflowPriority]] as const) {
      if (value !== undefined && (!Number.isFinite(value) || value < 0)) {
        errors.push(`toolbar ${name} must be a non-negative finite number: ${displayId}`);
      }
    }

    descriptor.children?.forEach(visit);
  };

  descriptors.forEach(visit);
  return errors;
}
