import type { DocumentBlock } from "@open-office/schema/artifact";

import type { BlockSessionApi } from "../../hooks/useBlockSession.js";
import { focusBlock } from "../focus.js";

export interface GutterActionsInput {
  block: DocumentBlock;
  listType: "ordered" | "bullet" | null;
  session: BlockSessionApi;
  closeMenu: () => void;
}

export interface GutterActions {
  onInsert: () => void;
  onDelete: () => void;
  onKind: (kind: Parameters<BlockSessionApi["convertBlock"]>[1]) => void;
  onAlignment: (align: Parameters<BlockSessionApi["setBlockPresentation"]>[1] extends { align?: infer A } ? A : never) => void;
  onList: (type: "ordered" | "bullet") => void;
  onLink: () => void;
  onInsertTable: (rows: number, columns: number) => void;
  onInsertImage: (file: File) => Promise<void>;
  onInsertQuote: () => void;
  onInsertCallout: () => void;
  onInsertTodo: () => void;
  onInsertCode: () => void;
  onDivider: () => void;
}

/**
 * Owns every gutter menu action as one behavior module. BlockNode stays a DOM
 * projection: it passes session callbacks through without embedding menu
 * semantics, so adding a gutter action never grows the renderer.
 */
export function createGutterActions({ block, listType, session, closeMenu }: GutterActionsInput): GutterActions {
  const insertAfter = (payload?: Parameters<BlockSessionApi["insertAfter"]>[1]) => {
    session.insertAfter(block.id, payload);
    closeMenu();
  };
  return {
    onInsert: () => {
      const nextId = session.insertAfter(block.id);
      closeMenu();
      if (nextId) requestAnimationFrame(() => focusBlock(nextId));
    },
    onDelete: () => { session.deleteBlock(block.id); closeMenu(); },
    onKind: (kind) => { session.convertBlock(block.id, kind); closeMenu(); },
    onAlignment: (nextAlign) => { session.setBlockPresentation(block.id, { align: nextAlign }); closeMenu(); },
    onList: (type) => { session.setBlockPresentation(block.id, { listType: listType === type ? null : type }); closeMenu(); },
    onLink: () => {
      const currentUrl = block.data.type === "link" ? block.data.data.url : "";
      const url = window.prompt("链接地址", currentUrl || "https://");
      if (!url?.trim()) return;
      if (block.kind.type === "link") session.setLinkTarget(block.id, url.trim());
      else session.convertToLink(block.id, url.trim());
      closeMenu();
    },
    onInsertTable: (rows, columns) => { session.insertTableAfter(block.id, rows, columns); closeMenu(); },
    onInsertImage: async (file) => {
      closeMenu();
      const imageId = await session.insertPastedImage(block.id, file);
      if (imageId) session.setActiveBlock(imageId);
    },
    onInsertQuote: () => insertAfter({ type: "quote" }),
    onInsertCallout: () => insertAfter({ type: "callout" }),
    onInsertTodo: () => insertAfter({ type: "todo" }),
    onInsertCode: () => insertAfter({ type: "code" }),
    onDivider: () => insertAfter({ type: "divider" }),
  };
}
