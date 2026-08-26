import type { KeyboardEvent as ReactKeyboardEvent } from "react";

import type { BlockSessionApi } from "../hooks/useBlockSession.js";
import type { BlockRegistry } from "../blocks/registry.js";
import type { InteractionStore } from "./interactionStore.js";

/**
 * Root keyboard dispatch for selected atomic blocks. Native editable text and
 * table-cell editing remain untouched unless their registered behavior claims
 * a key; this prevents a generic router from stealing browser caret input.
 */
export function routeEditorKeyDown(
  event: ReactKeyboardEvent<HTMLElement>,
  { interaction, registry, session }: {
    interaction: InteractionStore;
    registry: BlockRegistry;
    session: BlockSessionApi;
  },
): void {
  if (event.defaultPrevented) return;
  const target = event.target;
  if (!(target instanceof Element)) return;
  const row = target.closest<HTMLElement>("[data-block-id]");
  const blockId = row?.dataset.blockId;
  if (!blockId) return;
  const block = session.projection.getBlock(blockId);
  if (!block) return;
  const behavior = registry.resolve(block).behavior;
  if (behavior?.selection !== "object") return;

  // Focus is the authoritative DOM signal for atomic objects. The
  // interaction store normally contains the same object selection, but a
  // neighbouring contenteditable may emit a delayed `selectionchange` while
  // an image is being re-selected. Routing from the focused object keeps
  // Arrow/Enter deterministic without ever intercepting editable text.
  const selection = interaction.getSnapshot();
  if (selection.kind !== "object" || selection.blockId !== block.id) {
    if (block.kind.type === "image" || block.kind.type === "code") {
      interaction.select({ kind: "object", blockId: block.id, objectType: block.kind.type });
    }
  }
  // Escape must be an explicit object-cancel affordance even when browser
  // focus has moved between the object frame and its toolbar. Overlay
  // Coordinator handles an open crop/dialog first; once no overlay claims
  // the key this clears the selected object and removes its toolbar.
  if (event.key === "Escape") {
    interaction.clear();
    event.preventDefault();
    return;
  }
  behavior.keyboard?.(event, { block, blockId: block.id, session });
}
