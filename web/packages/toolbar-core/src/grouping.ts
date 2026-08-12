import type { ToolbarDescriptor, ToolbarGroupDescriptor, ToolbarResolutionContext } from "./descriptor.js";

/** Stable grouping used by every Artifact toolbar renderer. */
export function groupToolbar<ActionId extends string, C extends ToolbarResolutionContext>(descriptors: readonly ToolbarDescriptor<ActionId, C>[]): ToolbarGroupDescriptor<ActionId, C>[] {
  const groups = new Map<string, ToolbarGroupDescriptor<ActionId, C>>();
  descriptors.forEach((descriptor) => {
    const group = groups.get(descriptor.group);
    if (group) {
      // Keep the public `items` collection readonly while avoiding an
      // allocation for every item. Toolbar descriptors are already immutable
      // input data; grouping only creates the output buckets.
      (group.items as ToolbarDescriptor<ActionId, C>[]).push(descriptor);
      return;
    }
    const next: ToolbarGroupDescriptor<ActionId, C> = { id: descriptor.group, items: [descriptor] };
    groups.set(descriptor.group, next);
  });
  return [...groups.values()];
}
