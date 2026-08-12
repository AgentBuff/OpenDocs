import type { ResolvedToolbarDescriptor, ToolbarDescriptor, ToolbarPredicate, ToolbarResolutionContext } from "./descriptor.js";

function resolvePredicate<C extends ToolbarResolutionContext>(predicate: ToolbarPredicate<C> | undefined, context: C, defaultValue: boolean): boolean {
  if (predicate === undefined) return defaultValue;
  return typeof predicate === "function" ? predicate(context) : predicate;
}

function resolveOne<ActionId extends string, C extends ToolbarResolutionContext>(descriptor: ToolbarDescriptor<ActionId, C>, context: C): ResolvedToolbarDescriptor<ActionId> | undefined {
  // Visibility is evaluated at every level. Hidden menu children must never
  // leak into a renderer, otherwise a product adapter can accidentally expose
  // a capability it declared as unavailable.
  if (!resolvePredicate(descriptor.visible, context, true)) return undefined;

  const children = descriptor.children
    ?.map((child) => resolveOne(child, context))
    .filter((child): child is ResolvedToolbarDescriptor<ActionId> => child !== undefined);

  return {
    ...descriptor,
    enabled: resolvePredicate(descriptor.enabled, context, true),
    active: resolvePredicate(descriptor.active, context, false),
    children,
  };
}

/** Filter hidden capabilities and resolve state without rendering or side effects. */
export function resolveToolbar<ActionId extends string, C extends ToolbarResolutionContext>(descriptors: readonly ToolbarDescriptor<ActionId, C>[], context: C): ResolvedToolbarDescriptor<ActionId>[] {
  return descriptors
    .map((descriptor) => resolveOne(descriptor, context))
    .filter((descriptor): descriptor is ResolvedToolbarDescriptor<ActionId> => descriptor !== undefined);
}
