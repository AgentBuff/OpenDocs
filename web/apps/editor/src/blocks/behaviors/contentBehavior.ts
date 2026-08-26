import type { KeyboardEvent } from "react";

import type { DocumentBlock } from "@open-office/schema/artifact";

import type { BlockSessionApi } from "../../hooks/useBlockSession.js";
import { focusBlock } from "../focus.js";
import { richTextFromHtml } from "../richText.js";

interface ContentBehaviorOptions {
  block: DocumentBlock;
  blockId: string;
  depth: number;
  session: BlockSessionApi;
}

/**
 * Text-entry semantics shared by content-like blocks. It only creates normal
 * document commands through BlockSessionApi; DOM is used solely as the
 * current RichText projection.
 */
export function createContentBehavior({ block, blockId, depth, session }: ContentBehaviorOptions) {
  const list = block.presentation.list;
  const listKind = list?.kind === "ordered" || list?.kind === "bullet" ? list.kind : null;
  return {
    onInput(element: HTMLElement | null) {
      if (element) session.updateContent(blockId, richTextFromHtml(element));
    },
    onKeyDown(event: KeyboardEvent<HTMLDivElement>) {
      if (event.defaultPrevented) return;
      if (event.key === "Enter" && !event.shiftKey) {
        event.preventDefault();
        if (listKind && !block.content?.text.trim()) {
          session.setBlockPresentation(blockId, { listType: null, listLevel: null, indentLevel: null });
          const nextId = session.insertAfter(blockId, { type: "paragraph" });
          requestAnimationFrame(() => focusBlock(nextId ?? blockId));
          return;
        }
        const nextAttrs = listKind && block.content?.text.trim()
          ? { list: { kind: listKind, level: list?.level ?? 0 } }
          : undefined;
        const nextId = session.insertAfter(blockId, { type: "paragraph" }, nextAttrs);
        if (nextId) requestAnimationFrame(() => focusBlock(nextId));
      } else if (event.key === "Backspace" && !event.currentTarget.textContent && depth === 0) {
        event.preventDefault();
        session.deleteBlock(blockId);
      }
    },
  };
}
