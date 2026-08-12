/** Framework-free toolbar contract. Actions are opaque IDs resolved by an Artifact adapter. */
export type ToolbarItemKind = "button" | "toggle" | "select" | "menu" | "separator";
export type ToolbarResolutionContext = object;
export type ToolbarPredicate<C extends ToolbarResolutionContext = ToolbarResolutionContext> = boolean | ((context: C) => boolean);

export interface ToolbarDescriptor<ActionId extends string = string, C extends ToolbarResolutionContext = ToolbarResolutionContext> {
  id: string;
  group: string;
  kind: ToolbarItemKind;
  label?: string;
  ariaLabel?: string;
  icon?: string;
  shortcut?: string;
  action?: ActionId;
  visible?: ToolbarPredicate<C>;
  enabled?: ToolbarPredicate<C>;
  active?: ToolbarPredicate<C>;
  priority?: number;
  overflowPriority?: number;
  children?: readonly ToolbarDescriptor<ActionId, C>[];
}

export interface ResolvedToolbarDescriptor<ActionId extends string = string> extends Omit<ToolbarDescriptor<ActionId>, "visible" | "enabled" | "active" | "children"> {
  enabled: boolean;
  active: boolean;
  children?: readonly ResolvedToolbarDescriptor<ActionId>[];
}

export interface ToolbarGroupDescriptor<ActionId extends string = string, C extends ToolbarResolutionContext = ToolbarResolutionContext> {
  id: string;
  label?: string;
  priority?: number;
  items: readonly ToolbarDescriptor<ActionId, C>[];
}
