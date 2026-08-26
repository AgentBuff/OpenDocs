import type { TextRange } from "@open-office/schema/artifact";

import type { TableSelection } from "../blocks/table/model.js";

/** Ephemeral, mutually-exclusive editor selection. It is never persisted. */
export type EditorSelection =
  | { kind: "none" }
  | { kind: "text"; blockId: string; range: TextRange; affinity: "forward" | "backward" }
  | { kind: "blocks"; blockIds: readonly string[]; anchorId: string; focusId: string }
  | { kind: "object"; blockId: string; objectType: ObjectBlockType }
  | { kind: "table"; blockId: string; selection: TableSelection; mode: TableSelectionMode };

export type ObjectBlockType = "image" | "code";
export type TableSelectionMode = "cell" | "range" | "row" | "column" | "all";

export type SelectionEvent =
  | { type: "select"; selection: EditorSelection }
  | { type: "clear" }
  | { type: "escape"; overlayHandled: boolean };

export type OverlayKind = "blockMenu" | "contextMenu" | "toolbar" | "popover" | "dialog" | "toast";

export interface OverlayRegistration {
  id: string;
  kind: OverlayKind;
  ownerBlockId?: string;
  closeOnEscape: boolean;
  closeOnOutsidePointer: boolean;
  priority: number;
}

export const EMPTY_EDITOR_SELECTION: EditorSelection = { kind: "none" };

export function tableSelectionMode(selection: TableSelection): TableSelectionMode {
  return selection.kind;
}
