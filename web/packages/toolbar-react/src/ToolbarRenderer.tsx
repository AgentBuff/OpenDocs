import { useMemo } from "react";
import { Toolbar, ToolbarGroup } from "@open-office/ui";
import { resolveToolbar, type ResolvedToolbarDescriptor, type ToolbarResolutionContext } from "@open-office/toolbar-core";
import { ToolbarItem } from "./ToolbarItem.js";
import type { ToolbarActionHandler, ToolbarRendererProps } from "./types.js";

function cx(...values: Array<string | undefined | false>): string {
  return values.filter(Boolean).join(" ");
}

export function ToolbarRenderer<ActionId extends string = string, C extends ToolbarResolutionContext = ToolbarResolutionContext>({ items, context, onAction, className, ariaLabel = "工具栏", iconMap, groupLabels }: ToolbarRendererProps<ActionId, C>) {
  const resolved = useMemo(() => resolveToolbar(items, context), [context, items]);
  const grouped = useMemo(() => {
    const groups = new Map<string, ResolvedToolbarDescriptor<ActionId>[]>();
    resolved.forEach((item) => {
      const group = groups.get(item.group);
      if (group) {
        group.push(item);
      } else {
        groups.set(item.group, [item]);
      }
    });
    return [...groups.entries()];
  }, [resolved]);
  const handler: ToolbarActionHandler<ActionId> = onAction;
  return <Toolbar className={cx("oo-toolbar--descriptor", className)} role="toolbar" aria-label={ariaLabel}>
    {grouped.map(([group, groupItems]) => <ToolbarGroup key={group} role="group" aria-label={groupLabels?.[group] ?? group}>{groupItems.map((item) => <ToolbarItem key={item.id} item={item} onAction={handler} iconMap={iconMap} />)}</ToolbarGroup>)}
  </Toolbar>;
}
