import type { PointerEvent as ReactPointerEvent } from "react";

import type { InteractionStore } from "./interactionStore.js";

/**
 * Routes editor-surface pointer transitions that do not belong to a block
 * behavior. It never prevents native editing events: a text/table/image
 * behavior receives the same event afterwards and may claim it explicitly.
 */
export function routeEditorPointerDown(event: ReactPointerEvent<HTMLElement>, interaction: InteractionStore): void {
  const target = event.target;
  if (!(target instanceof Element)) return;
  const current = interaction.getSnapshot();
  if (current.kind === "object" && !target.closest(".block-image")) {
    interaction.clear();
    return;
  }
  if (current.kind === "table" && !target.closest(".block-table-wrap")) interaction.clear();
}
