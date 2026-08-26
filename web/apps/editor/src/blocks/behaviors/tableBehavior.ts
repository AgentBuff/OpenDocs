import type { BlockBehavior } from "../registry.js";

/**
 * Table pointer and Shift-range expansion are owned by the table selection
 * controller. This declaration makes its selection ownership explicit to the
 * shared editor router without putting table model logic in the registry.
 */
export const tableBehavior: BlockBehavior = {
  selection: "table",
  tableSelection(selection, { blockId, interaction }) {
    if (selection) {
      interaction.select({ kind: "table", blockId, selection, mode: selection.kind });
      return;
    }
    const current = interaction.getSnapshot();
    if (current.kind === "table" && current.blockId === blockId) interaction.clear();
  },
};
