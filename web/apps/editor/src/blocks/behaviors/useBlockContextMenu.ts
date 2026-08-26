import { useCallback, useState, type MouseEvent as ReactMouseEvent, type RefObject } from "react";

import type { BlockSessionApi } from "../../hooks/useBlockSession.js";
import { richTextFromHtml } from "../richText.js";
import type { BlockContextMenuTarget } from "../BlockContextMenu.js";

/** Native text context-menu wiring for ordinary content blocks. */
export function useBlockContextMenu({
  blockId,
  session,
  contentRef,
}: {
  blockId: string;
  session: BlockSessionApi;
  contentRef: RefObject<HTMLDivElement>;
}) {
  const [contextMenu, setContextMenu] = useState<BlockContextMenuTarget | null>(null);

  const openContextMenu = useCallback((event: ReactMouseEvent<HTMLElement>) => {
    if ((event.target as Element).closest(".block-row__gutter, .block-row__menu")) return;
    event.preventDefault();
    event.stopPropagation();
    session.setActiveBlock(blockId);
    const selection = window.getSelection();
    const hasSelection = Boolean(selection?.toString().trim());
    const canCut = Boolean(
      hasSelection
      && contentRef.current
      && selection?.anchorNode
      && selection?.focusNode
      && contentRef.current.contains(selection.anchorNode)
      && contentRef.current.contains(selection.focusNode),
    );
    setContextMenu({ x: event.clientX, y: event.clientY, hasSelection, canCut });
  }, [blockId, contentRef, session]);

  const copySelection = useCallback(async () => {
    const text = window.getSelection()?.toString() ?? "";
    if (text) await navigator.clipboard?.writeText(text);
    setContextMenu(null);
  }, []);

  const cutSelection = useCallback(() => {
    const selection = window.getSelection();
    if (!selection?.rangeCount || !contentRef.current) return;
    const range = selection.getRangeAt(0);
    if (!contentRef.current.contains(range.startContainer) || !contentRef.current.contains(range.endContainer)) return;
    range.deleteContents();
    selection.collapseToStart();
    session.updateContent(blockId, richTextFromHtml(contentRef.current));
    setContextMenu(null);
  }, [blockId, contentRef, session]);

  return { contextMenu, setContextMenu, openContextMenu, copySelection, cutSelection };
}
