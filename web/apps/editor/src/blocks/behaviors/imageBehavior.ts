import type { BlockBehavior } from "../registry.js";
import { createObjectKeyboardBehavior } from "./objectKeyboardBehavior.js";

/**
 * Image object semantics are registered at the block-definition boundary.
 * The renderer only renders a selected frame; root keyboard routing invokes
 * this behavior for the active object selection.
 */
export const imageBehavior: BlockBehavior = {
  selection: "object",
  keyboard(event, context) {
    createObjectKeyboardBehavior(context.blockId, context.session)(event);
  },
};
