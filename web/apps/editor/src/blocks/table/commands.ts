import type { DocumentCommand } from "@open-office/schema/artifact";

import type { TableSelection } from "./model.js";

/** Projects a stable-id view selection to the canonical table command shape. */
export function tableCommandSelection(
  selection: TableSelection,
): Extract<DocumentCommand, { type: "formatTableCells" }>["selection"] {
  if (selection.kind === "cell") return { kind: "cell", rowId: selection.rowId, cellId: selection.cellId };
  if (selection.kind === "row") return { kind: "row", rowId: selection.id };
  if (selection.kind === "column") return { kind: "column", columnId: selection.id };
  if (selection.kind === "range") return {
    kind: "range",
    startRowId: selection.startRowId,
    endRowId: selection.endRowId,
    startColumnId: selection.startColumnId,
    endColumnId: selection.endColumnId,
  };
  return { kind: "all" };
}

/** Reject unsupported toolbar attributes before they cross into the engine. */
export function tableTextAttrsToInlinePatch(
  attrs: Record<string, unknown>,
): Extract<DocumentCommand, { type: "patchTableCellInlineRange" }>["patch"] | null {
  const patch: Extract<DocumentCommand, { type: "patchTableCellInlineRange" }>["patch"] = {};
  for (const [key, value] of Object.entries(attrs)) {
    switch (key) {
      case "bold":
      case "italic":
      case "underline":
      case "strikethrough":
        if (typeof value !== "boolean") return null;
        patch[key] = value;
        break;
      case "color":
      case "highlight":
        if (value !== null && typeof value !== "string") return null;
        patch[key] = value;
        break;
      case "fontSize":
        if (typeof value !== "number" || !Number.isFinite(value)) return null;
        patch.fontSize = value;
        break;
      default:
        return null;
    }
  }
  return Object.keys(patch).length > 0 ? patch : null;
}
