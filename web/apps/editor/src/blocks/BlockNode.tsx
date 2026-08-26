import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { KeyboardEvent } from "react";

import type { BlockSessionApi } from "../hooks/useBlockSession.js";
import { useBlockProjection } from "../store/blockProjectionStore.js";
import type { BlockProjectionStore } from "../store/blockProjectionStore.js";
import { BlockContextMenu } from "./BlockContextMenu.js";
import { BlockGutter } from "./BlockGutter.js";
import { focusBlock } from "./focus.js";
import type { InteractionStore } from "../interaction/interactionStore.js";
import { useEditorSelection } from "../interaction/interactionStore.js";
import { richTextFromHtml, richTextToDom } from "./richText.js";
import { createContentBehavior } from "./behaviors/contentBehavior.js";
import { useBlockContextMenu } from "./behaviors/useBlockContextMenu.js";
import type { TableSelection } from "./table/model.js";
import { readDomTextSelection } from "../interaction/domSelection.js";
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
  interaction: InteractionStore;
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
  interaction,
  activeBlockId,
  depth,
  listOrdinal,
}: BlockNodeProps) {
  const block = useBlockProjection(store, blockId);
  const contentRef = useRef<HTMLDivElement>(null!);
  const [menuOpen, setMenuOpen] = useState(false);
  const interactionSelection = useEditorSelection(interaction);
  const focused = activeBlockId === blockId;
  const activeListType = block?.presentation.list?.kind === "ordered" || block?.presentation.list?.kind === "bullet"
    ? block.presentation.list.kind
    : null;
  const { contextMenu, setContextMenu, openContextMenu, copySelection, cutSelection } = useBlockContextMenu({ blockId, session, contentRef });

  useEffect(() => {
    const element = contentRef.current;
    if (!block || !element || block.data.type !== "none") return;
    // Text entry is browser/IME-owned and must not be rebuilt on every
    // character. A plain text input command produces the same RichText as
    // the active DOM, while a semantic inline-format command changes runs
    // without changing text. Comparing runs lets us preserve the former and
    // visibly apply the latter even while the editor remains focused.
    if (richTextEquals(richTextFromHtml(element), block.content)) return;
    element.replaceChildren(richTextToDom(block.content));
  }, [block?.content, block?.data.type]);

  const contentBehavior = useMemo(
    () => block ? createContentBehavior({ block, blockId, depth, session }) : null,
    [block, blockId, depth, session],
  );

  const onInput = useCallback(() => {
    contentBehavior?.onInput(contentRef.current);
    const selection = contentRef.current ? readDomTextSelection(contentRef.current, blockId) : null;
    if (selection) interaction.select(selection);
  }, [blockId, contentBehavior, interaction]);

  const onContentFocus = useCallback(() => {
    // `selectionchange` refines the caret offset immediately after this focus
    // event. This first transition makes focus deterministic even in browsers
    // that delay selectionchange until the next key press.
    interaction.select((contentRef.current ? readDomTextSelection(contentRef.current, blockId) : null) ?? {
      kind: "text",
      blockId,
      range: { start: 0, end: 0 },
      affinity: "forward",
    });
    session.setActiveBlock(blockId);
  }, [blockId, interaction, session]);

  const onSelectObject = useCallback(() => {
    interaction.select({ kind: "object", blockId, objectType: "image" });
    session.setActiveBlock(blockId);
  }, [blockId, interaction, session]);

  const onTableSelection = useCallback((tableSelection: TableSelection | null) => {
    const currentBlock = store.getBlock(blockId);
    if (!currentBlock) return;
    registry.resolve(currentBlock).behavior?.tableSelection?.(tableSelection, {
      block: currentBlock,
      blockId,
      session,
      interaction,
    });
  }, [blockId, interaction, registry, session, store]);

  const onKeyDown = useCallback((event: KeyboardEvent<HTMLDivElement>) => {
    contentBehavior?.onKeyDown(event);
  }, [contentBehavior]);

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
  const selected = definition.behavior?.selection === "object"
    ? interactionSelection.kind === "object" && interactionSelection.blockId === block.id
    : focused;

  return (
    <section
      className={`block-row block-row--${kindClass}${block.kind.type === "todo" && block.data.type === "todo" && block.data.data.checked ? " is-todo-done" : ""}${focused ? " is-focused" : ""}${menuOpen ? " is-menu-open" : ""}`}
      data-block-id={block.id}
      data-block-menu-open={menuOpen ? "true" : undefined}
      onContextMenu={openContextMenu}
    >
      <BlockGutter
        block={block}
        emptyTextBlock={emptyTextBlock}
        menuOpen={menuOpen}
        session={session}
        align={align}
        listType={listType}
        onMenuOpenChange={setMenuOpen}
        onInsert={() => {
          const nextId = session.insertAfter(block.id);
          setMenuOpen(false);
          if (nextId) requestAnimationFrame(() => focusBlock(nextId));
        }}
        onDelete={() => { session.deleteBlock(block.id); setMenuOpen(false); }}
        onKind={(kind) => { session.convertBlock(block.id, kind); setMenuOpen(false); }}
        onAlignment={(nextAlign) => { session.setBlockPresentation(block.id, { align: nextAlign }); setMenuOpen(false); }}
        onList={(type) => { session.setBlockPresentation(block.id, { listType: listType === type ? null : type }); setMenuOpen(false); }}
        onLink={() => {
          const currentUrl = block.data.type === "link" ? block.data.data.url : "";
          const url = window.prompt("链接地址", currentUrl || "https://");
          if (!url?.trim()) return;
          if (block.kind.type === "link") session.setLinkTarget(block.id, url.trim());
          else session.convertToLink(block.id, url.trim());
          setMenuOpen(false);
        }}
        onInsertTable={(rows, columns) => { session.insertTableAfter(block.id, rows, columns); setMenuOpen(false); }}
        onInsertImage={async (file) => {
          setMenuOpen(false);
          const imageId = await session.insertPastedImage(block.id, file);
          if (imageId) session.setActiveBlock(imageId);
        }}
        onInsertQuote={() => { session.insertAfter(block.id, { type: "quote" }); setMenuOpen(false); }}
        onInsertCallout={() => { session.insertAfter(block.id, { type: "callout" }); setMenuOpen(false); }}
        onInsertTodo={() => { session.insertAfter(block.id, { type: "todo" }); setMenuOpen(false); }}
        onInsertCode={() => { session.insertAfter(block.id, { type: "code" }); setMenuOpen(false); }}
        onDivider={() => { session.insertAfter(block.id, { type: "divider" }); setMenuOpen(false); }}
      />
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
            selected={selected}
            contentRef={contentRef}
            empty={empty}
            align={align}
            lineHeight={lineHeight}
            placeholder={definition.placeholder?.(block) ?? contentPlaceholder(block.kind)}
            onInput={onInput}
            onKeyDown={onKeyDown}
            onFocus={onContentFocus}
            onSelectObject={onSelectObject}
            onTableSelection={onTableSelection}
            editorSelection={interactionSelection}
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
            interaction={interaction}
            activeBlockId={activeBlockId}
            depth={depth + 1}
            listOrdinal={listOrdinalFor(block.children, childIndex, store)}
          />
        ) : null)}
      </div>
      {contextMenu && <BlockContextMenu target={contextMenu} onDismiss={() => setContextMenu(null)} onCut={cutSelection} onCopy={() => void copySelection()} />}
    </section>
  );
}

function richTextEquals(left: NonNullable<ReturnType<typeof richTextFromHtml>>, right: typeof left | null): boolean {
  return right !== null
    && left.text === right.text
    && JSON.stringify(left.runs) === JSON.stringify(right.runs);
}

/** Stable block subscription boundary; unrelated blocks do not re-render. */
export const BlockNode = memo(BlockNodeImpl, (previous, next) => (
  previous.blockId === next.blockId
  && previous.store === next.store
  && previous.registry === next.registry
  && previous.sessionKey === next.sessionKey
  && previous.interaction === next.interaction
  && previous.activeBlockId === next.activeBlockId
  && previous.depth === next.depth
  && previous.listOrdinal === next.listOrdinal
));
