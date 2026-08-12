import type { ResolvedToolbarDescriptor, ToolbarDescriptor, ToolbarResolutionContext } from "@open-office/toolbar-core";
import type { IconName } from "@open-office/ui";

export interface ToolbarRendererProps<ActionId extends string = string, C extends ToolbarResolutionContext = ToolbarResolutionContext> {
  items: readonly ToolbarDescriptor<ActionId, C>[];
  context: C;
  onAction: (action: ActionId, item: ResolvedToolbarDescriptor<ActionId>) => void;
  className?: string;
  ariaLabel?: string;
  iconMap?: Partial<Record<string, IconName>>;
  /** Human-readable labels for stable group IDs; IDs remain the adapter contract. */
  groupLabels?: Readonly<Record<string, string>>;
}

export type ToolbarActionHandler<ActionId extends string> = (action: ActionId, item: ResolvedToolbarDescriptor<ActionId>) => void;
