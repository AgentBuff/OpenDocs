import type { KeyboardEvent } from "react";

import type { DocumentBlock } from "@open-office/schema/artifact";

import type { BlockSessionApi } from "../../hooks/useBlockSession.js";

type CursorEdge = "start" | "end";

function canHostTextCursor(block: DocumentBlock | null): block is DocumentBlock {
  return block !== null
    && block.content !== null
    && block.data.type !== "image"
    && block.data.type !== "table"
    && block.data.type !== "code"
    && block.kind.type !== "divider";
}

function focusEditableBlock(id: string, edge: CursorEdge) {
  requestAnimationFrame(() => {
    const target = document.querySelector<HTMLElement>(`[data-block-id="${id}"] [contenteditable="true"]`);
    if (!target) return;
    target.focus();
    const selection = window.getSelection();
    if (!selection) return;
    const range = document.createRange();
    range.selectNodeContents(target);
    range.collapse(edge === "start");
    selection.removeAllRanges();
    selection.addRange(range);
  });
}

/**
 * Semantic object navigation shared by image and future embedded blocks.
 * There is no object-local model here: sibling resolution is read-only and
 * paragraph insertion delegates to the existing session command boundary.
 */
export function createObjectKeyboardBehavior(blockId: string, session: BlockSessionApi) {
  const moveCursor = (direction: "before" | "after") => {
    const location = session.projection.findLocation(blockId);
    if (!location) return;
    const siblings = location.parentId
      ? session.projection.getBlock(location.parentId)?.children ?? []
      : session.projection.getStructureSnapshot().root;
    const siblingId = siblings[location.index + (direction === "before" ? -1 : 1)];
    const sibling = siblingId ? session.projection.getBlock(siblingId) : null;
    if (canHostTextCursor(sibling)) {
      session.setActiveBlock(sibling.id);
      focusEditableBlock(sibling.id, direction === "before" ? "end" : "start");
      return;
    }
    const cursorId = direction === "before"
      ? session.insertBefore(blockId, { type: "paragraph" })
      : session.insertAfter(blockId, { type: "paragraph" });
    if (!cursorId) return;
    session.setActiveBlock(cursorId);
    focusEditableBlock(cursorId, direction === "before" ? "end" : "start");
  };

  return (event: KeyboardEvent<HTMLElement>) => {
    if (event.defaultPrevented || event.altKey || event.ctrlKey || event.metaKey) return;
    if (event.key === "ArrowLeft") {
      event.preventDefault();
      moveCursor("before");
    } else if (event.key === "ArrowRight" || event.key === "Enter") {
      event.preventDefault();
      moveCursor("after");
    }
  };
}
