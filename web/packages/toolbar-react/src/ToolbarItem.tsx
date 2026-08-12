import { Icon, ToolbarMenuButton } from "@open-office/ui";
import type { IconName } from "@open-office/ui";
import type { ResolvedToolbarDescriptor } from "@open-office/toolbar-core";
import type { ToolbarActionHandler } from "./types.js";

function cx(...values: Array<string | undefined | false>): string {
  return values.filter(Boolean).join(" ");
}

export function ToolbarItem<ActionId extends string>({ item, onAction, iconMap }: { item: ResolvedToolbarDescriptor<ActionId>; onAction: ToolbarActionHandler<ActionId>; iconMap?: Partial<Record<string, IconName>> }) {
  if (item.kind === "separator") return <span className="oo-toolbar__separator" role="separator" />;
  const label = item.ariaLabel ?? item.label ?? item.id;
  if (item.kind === "menu" || item.kind === "select") {
    return <ToolbarMenuButton
      type="button"
      disabled={!item.enabled}
      aria-label={label}
      aria-keyshortcuts={item.shortcut}
      aria-haspopup={item.kind === "select" ? "listbox" : "menu"}
      title={item.shortcut ? `${label} (${item.shortcut})` : label}
      data-toolbar-id={item.id}
      data-toolbar-kind={item.kind}
      onClick={() => item.action && onAction(item.action, item)}
    >
      {item.icon && <Icon name={(iconMap?.[item.icon] ?? item.icon) as IconName} size={16} />}
      {item.label}
    </ToolbarMenuButton>;
  }
  if (!item.action) return null;
  return <button
    type="button"
    className={cx("oo-toolbar__item", item.active && "is-active")}
    aria-label={label}
    aria-keyshortcuts={item.shortcut}
    aria-pressed={item.kind === "toggle" ? item.active : undefined}
    title={item.shortcut ? `${label} (${item.shortcut})` : label}
    data-toolbar-id={item.id}
    data-toolbar-kind={item.kind}
    disabled={!item.enabled}
    onClick={() => onAction(item.action!, item)}
  >
    {item.icon && <Icon name={(iconMap?.[item.icon] ?? item.icon) as IconName} size={16} />}
    {item.label && <span className="oo-toolbar__item-label">{item.label}</span>}
  </button>;
}
