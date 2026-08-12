import { memo, useCallback, useEffect, useRef, useState } from "react";
import type { KeyboardEvent, MouseEvent as ReactMouseEvent } from "react";

import { Icon, Popover } from "@open-office/ui";

import type { BlockSessionApi } from "../hooks/useBlockSession.js";
import { useBlockProjection } from "../store/blockProjectionStore.js";
import type { BlockProjectionStore } from "../store/blockProjectionStore.js";
import { BlockMenu } from "./BlockMenu.js";
import { BlockContextMenu, type BlockContextMenuTarget } from "./BlockContextMenu.js";
import { focusBlock } from "./focus.js";
import { richTextFromHtml, richTextToDom } from "./richText.js";
import {
  contentClassName,
  contentPlaceholder,
  type BlockRegistry,
} from "./registry.js";

interface BlockNodeProps {
  blockId: string;
  store: BlockProjectionStore;
  registry: BlockRegistry;
  session: BlockSessionApi;
  sessionKey: string;
  activeBlockId: string | null;
  depth: number;
  listOrdinal: number;
}

/** Returns the zero-based number within the current contiguous ordered list. */
export function listOrdinalFor(ids: readonly string[], index: number, store: BlockProjectionStore): number {
  const current = store.getBlock(ids[index]);
  if (current?.presentation.list?.kind !== "ordered") return 0;
  let ordinal = 0;
  for (let cursor = index; cursor >= 0; cursor -= 1) {
    if (store.getBlock(ids[cursor])?.presentation.list?.kind !== "ordered") break;
    ordinal += 1;
  }
  return Math.max(0, ordinal - 1);
}

/**
 * Product-level block view. It owns interaction wiring, while the registry
 * owns block-kind rendering. The document engine remains behind session APIs.
 */
function BlockNodeImpl({
  blockId,
  store,
  registry,
  session,
  sessionKey,
  activeBlockId,
  depth,
  listOrdinal,
}: BlockNodeProps) {
  const block = useBlockProjection(store, blockId);
  const contentRef = useRef<HTMLDivElement>(null!);
  const rowRef = useRef<HTMLElement>(null);
  const [menuOpen, setMenuOpen] = useState(false);
  const [contextMenu, setContextMenu] = useState<BlockContextMenuTarget | null>(null);
  const focused = activeBlockId === blockId;
  const activeListType = block?.presentation.list?.kind === "ordered" || block?.presentation.list?.kind === "bullet"
    ? block.presentation.list.kind
    : null;

  useEffect(() => {
    const element = contentRef.current;
    // Input is browser/IME-owned and must not be rebuilt on every character.
    // External transactions refresh the DOM once focus has left the block.
    if (!block || !element || block.data.type !== "none" || document.activeElement === element) return;
    element.replaceChildren(richTextToDom(block.content));
  }, [block?.content, block?.data.type]);

  useEffect(() => {
    if (!contextMenu) return;
    const closeOnOutsidePointer = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Element) || !target.closest(".block-row__context-menu")) setContextMenu(null);
    };
    const closeOnEscape = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") setContextMenu(null);
    };
    window.addEventListener("pointerdown", closeOnOutsidePointer);
    window.addEventListener("keydown", closeOnEscape);
    return () => {
      window.removeEventListener("pointerdown", closeOnOutsidePointer);
      window.removeEventListener("keydown", closeOnEscape);
    };
  }, [contextMenu]);

  // Popovers are rendered through a portal, so keep the close contract local
  // to the block as well.  This covers Escape and pointer transitions even
  // when the menu primitive cannot observe focus changes from the editor.
  useEffect(() => {
    if (!menuOpen) return;
    const closeOnEscape = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") setMenuOpen(false);
    };
    const closeOnOutsidePointer = (event: PointerEvent) => {
      const target = event.target;
      if (!(target instanceof Element)) return;
      if (target.closest(".block-row__handle, .block-row__menu")) return;
      setMenuOpen(false);
    };
    window.addEventListener("keydown", closeOnEscape);
    window.addEventListener("pointerdown", closeOnOutsidePointer);
    return () => {
      window.removeEventListener("keydown", closeOnEscape);
      window.removeEventListener("pointerdown", closeOnOutsidePointer);
    };
  }, [menuOpen]);

  const onInput = useCallback(() => {
    const element = contentRef.current;
    if (element) session.updateContent(blockId, richTextFromHtml(element));
  }, [blockId, session]);

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
  }, [blockId, session]);

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
  }, [blockId, session]);

  const onKeyDown = useCallback(
    (event: KeyboardEvent<HTMLDivElement>) => {
      if (event.defaultPrevented) return;
      if (event.key === "Enter" && !event.shiftKey) {
        event.preventDefault();
        // A second Enter on an empty list item exits the list in place. This
        // avoids creating an extra blank block and matches office behavior.
        if (activeListType && !block?.content?.text.trim()) {
          session.setBlockPresentation(blockId, { listType: null, listLevel: null, indentLevel: null });
          const nextId = session.insertAfter(blockId, { type: "paragraph" });
          requestAnimationFrame(() => focusBlock(nextId ?? blockId));
          return;
        }
        // A non-empty list item continues its list.
        const nextAttrs = activeListType && block?.content?.text.trim()
          ? { listType: activeListType }
          : undefined;
        const nextId = session.insertAfter(blockId, { type: "paragraph" }, nextAttrs);
        if (nextId) requestAnimationFrame(() => focusBlock(nextId));
      } else if (event.key === "Backspace" && !event.currentTarget.textContent && depth === 0) {
        event.preventDefault();
        session.deleteBlock(blockId);
      }
    },
    [activeListType, block?.content?.text, blockId, depth, session],
  );

  if (!block) return null;

  const empty = !block.content?.text;
  const emptyTextBlock = empty && (block.kind.type === "paragraph" || block.kind.type === "heading");
  const definition = registry.resolve(block);
  const kindClass = definition.className?.(block) ?? contentClassName(block.kind);
  const align = block.presentation.align === "left"
    || block.presentation.align === "center"
    || block.presentation.align === "right"
    || block.presentation.align === "justify"
    ? block.presentation.align
    : undefined;
  const indentLevel = typeof block.presentation.indentStart === "number" && Number.isFinite(block.presentation.indentStart)
    ? Math.max(0, Math.min(20, block.presentation.indentStart))
    : 0;
  const lineHeight = typeof block.presentation.lineHeight === "number" && Number.isFinite(block.presentation.lineHeight)
    ? block.presentation.lineHeight
    : undefined;
  const spacingBefore = typeof block.presentation.spacingBefore === "number" && Number.isFinite(block.presentation.spacingBefore)
    ? Math.max(0, block.presentation.spacingBefore)
    : 0;
  const spacingAfter = typeof block.presentation.spacingAfter === "number" && Number.isFinite(block.presentation.spacingAfter)
    ? Math.max(0, block.presentation.spacingAfter)
    : 0;
  const listType = activeListType;
  const marker = listType === "bullet" ? "•" : listType === "ordered" ? `${listOrdinal + 1}.` : null;
  const Renderer = definition.renderer;

  return (
    <section
      ref={rowRef}
      className={`block-row block-row--${kindClass}${block.kind.type === "todo" && block.data.type === "todo" && block.data.data.checked ? " is-todo-done" : ""}${focused ? " is-focused" : ""}${menuOpen ? " is-menu-open" : ""}`}
      data-block-id={block.id}
      data-block-menu-open={menuOpen ? "true" : undefined}
      onContextMenu={openContextMenu}
    >
      <div className="block-row__gutter">
        <Popover
          open={menuOpen}
          onOpenChange={(open) => {
            setMenuOpen(open);
            if (open) session.setActiveBlock(block.id);
          }}
          placement="left-start"
          offset={8}
          role="presentation"
          popupClassName="oo-overlay--block-menu"
          content={(
            <BlockMenu
              block={block}
              onClose={() => setMenuOpen(false)}
              onInsert={() => {
                const nextId = session.insertAfter(block.id);
                setMenuOpen(false);
                if (nextId) requestAnimationFrame(() => focusBlock(nextId));
              }}
              onDelete={() => {
                session.deleteBlock(block.id);
                setMenuOpen(false);
              }}
              onKind={(kind) => {
                session.convertBlock(block.id, kind);
                setMenuOpen(false);
              }}
              activeAlign={align}
              activeList={listType}
              onAlignment={(nextAlign) => {
                session.setBlockPresentation(block.id, { align: nextAlign });
                setMenuOpen(false);
              }}
              onList={(type) => {
                session.setBlockPresentation(block.id, { listType: listType === type ? null : type });
                setMenuOpen(false);
              }}
              onLink={() => {
                const currentUrl = block.data.type === "link" ? block.data.data.url : "";
                const url = window.prompt("链接地址", currentUrl || "https://");
                if (!url?.trim()) return;
                if (block.kind.type === "link") session.setLinkTarget(block.id, url.trim());
                else session.convertToLink(block.id, url.trim());
                setMenuOpen(false);
              }}
              onInsertTable={(rows, columns) => {
                session.insertTableAfter(block.id, rows, columns);
                setMenuOpen(false);
              }}
              onInsertQuote={() => {
                session.insertAfter(block.id, { type: "quote" });
                setMenuOpen(false);
              }}
              onInsertCode={() => {
                session.insertAfter(block.id, { type: "code" });
                setMenuOpen(false);
              }}
              onDivider={() => {
                session.insertAfter(block.id, { type: "divider" });
                setMenuOpen(false);
              }}
            />
          )}
        >
          <button
            className="block-row__handle"
            type="button"
            aria-label={emptyTextBlock ? "插入块" : "打开块菜单"}
            aria-haspopup="menu"
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => session.setActiveBlock(block.id)}
          >
            {emptyTextBlock ? <Icon name="insert" /> : <Icon name="block-handle" />}
          </button>
        </Popover>
      </div>
      <div
        className="block-row__body"
        style={{
          marginLeft: depth * 20 + indentLevel * 24,
          marginRight: typeof block.presentation.indentEnd === "number" ? block.presentation.indentEnd * 24 : undefined,
          paddingTop: spacingBefore || undefined,
          paddingBottom: spacingAfter || undefined,
        }}
      >
        <div className={`block-row__content-shell${marker ? " block-row__content-shell--list" : ""}${menuOpen ? " is-menu-open" : ""}`}>
          {marker && <span className="block-row__list-marker" aria-hidden="true">{marker}</span>}
          <Renderer
            block={block}
            session={session}
            selected={focused}
            contentRef={contentRef}
            empty={empty}
            align={align}
            lineHeight={lineHeight}
            placeholder={definition.placeholder?.(block) ?? contentPlaceholder(block.kind)}
            onInput={onInput}
            onKeyDown={onKeyDown}
          />
        </div>
        {block.children.map((childId, childIndex) => store.getBlock(childId) ? (
          <BlockNode
            key={childId}
            blockId={childId}
            store={store}
            registry={registry}
            session={session}
            sessionKey={sessionKey}
            activeBlockId={activeBlockId}
            depth={depth + 1}
            listOrdinal={listOrdinalFor(block.children, childIndex, store)}
          />
        ) : null)}
      </div>
      {contextMenu && <BlockContextMenu target={contextMenu} onCut={cutSelection} onCopy={() => void copySelection()} />}
    </section>
  );
}

/** Stable block subscription boundary; unrelated blocks do not re-render. */
export const BlockNode = memo(BlockNodeImpl, (previous, next) => (
  previous.blockId === next.blockId
  && previous.store === next.store
  && previous.registry === next.registry
  && previous.sessionKey === next.sessionKey
  && previous.activeBlockId === next.activeBlockId
  && previous.depth === next.depth
  && previous.listOrdinal === next.listOrdinal
));
