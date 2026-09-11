import { useEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties } from "react";
import { flushSync } from "react-dom";

import type { ArtifactPageSetup, HeaderFooterContent, DocumentNote } from "@open-office/schema/artifact";

import type { BlockSessionApi } from "./hooks/useBlockSession.js";
import { useBlockProjectionStructure } from "./store/blockProjectionStore.js";
import type { BlockProjectionStore } from "./store/blockProjectionStore.js";
import { createEditorBlockRegistry } from "./blocks/editorRegistry.js";
import { BlockNode, listOrdinalFor } from "./blocks/BlockNode.js";
import type { BlockRegistry } from "./blocks/registry.js";
import { readDomTextSelection } from "./interaction/domSelection.js";
import { InteractionStore } from "./interaction/interactionStore.js";
import { OverlayCoordinator } from "./interaction/OverlayCoordinator.js";
import { routeEditorPointerDown } from "./interaction/pointerRouter.js";
import { routeEditorKeyDown } from "./interaction/keyboardRouter.js";

interface Props {
  session: BlockSessionApi;
}

/** Block Tree 的 DOM 视图。每个 block 都有稳定 id，行首操作区永远在正文左侧。 */
export function BlockEditor({ session }: Props) {
  const registryRef = useRef<BlockRegistry | null>(null);
  const interactionRef = useRef<InteractionStore | null>(null);
  if (!registryRef.current) registryRef.current = createEditorBlockRegistry();
  if (!interactionRef.current) interactionRef.current= new InteractionStore();
  const store = session.projection;
  const registry = registryRef.current;
  const interaction = interactionRef.current;

  const structure = useBlockProjectionStructure(store);
  const printProjection = session.printProjection();
  const stageRef = useRef<HTMLDivElement>(null);
  const measuredHeightsRef = useRef(new Map<string, number>());
  const [visibleRootIds, setVisibleRootIds] = useState<ReadonlySet<string>>(
    () => new Set(structure.root.slice(0, INITIAL_MOUNTED_ROOTS)),
  );
  const [renderAllForPrint, setRenderAllForPrint] = useState(false);
  const activeRootId = useMemo(
    () => findContainingRoot(store, structure.root, session.state.activeBlockId),
    [session.state.activeBlockId, store, structure.root],
  );

  // Native DOM selection is a renderer concern, but its semantic projection
  // is session-scoped. Restrict this bridge to ordinary text blocks: table
  // cells and object blocks have their own adapters and must not be flattened
  // into a fake paragraph selection.
  useEffect(() => {
    const syncDomSelection = () => {
      const active = document.activeElement;
      if (!(active instanceof HTMLElement) || !active.matches(".block-row__content[contenteditable='true']")) return;
      const row = active.closest<HTMLElement>("[data-block-id]");
      const blockId = row?.dataset.blockId;
      if (!blockId) return;
      const selection = readDomTextSelection(active, blockId);
      if (selection) interaction.select(selection);
    };
    document.addEventListener("selectionchange", syncDomSelection);
    return () => document.removeEventListener("selectionchange", syncDomSelection);
  }, [interaction]);

  useEffect(() => {
    const stage = stageRef.current;
    const scrollRoot = stage?.closest<HTMLElement>(".editor__surface--blocks") ?? null;
    if (!stage || !scrollRoot || typeof IntersectionObserver === "undefined") {
      setVisibleRootIds(new Set(structure.root));
      return;
    }
    const intersectionObserver = new IntersectionObserver((entries) => {
      setVisibleRootIds((current) => {
        const next = new Set(current);
        let changed = false;
        for (const entry of entries) {
          const id = (entry.target as HTMLElement).dataset.virtualBlockId;
          if (!id) continue;
          if (entry.isIntersecting && !next.has(id)) {
            next.add(id);
            changed = true;
          } else if (!entry.isIntersecting && next.delete(id)) {
            changed = true;
          }
        }
        return changed ? next : current;
      });
    }, { root: scrollRoot, rootMargin: "1000px 0px", threshold: 0 });
    const resizeObserver = typeof ResizeObserver === "undefined" ? null : new ResizeObserver((entries) => {
      for (const entry of entries) {
        const element = entry.target as HTMLElement;
        const id = element.dataset.virtualBlockId;
        if (id && entry.contentRect.height > 0) measuredHeightsRef.current.set(id, entry.contentRect.height);
      }
    });
    const slots = stage.querySelectorAll<HTMLElement>("[data-virtual-block-id]");
    slots.forEach((slot) => {
      intersectionObserver.observe(slot);
      resizeObserver?.observe(slot);
    });
    return () => {
      intersectionObserver.disconnect();
      resizeObserver?.disconnect();
    };
  }, [structure.root]);

  useEffect(() => {
    const beforePrint = () => flushSync(() => setRenderAllForPrint(true));
    const afterPrint = () => setRenderAllForPrint(false);
    window.addEventListener("beforeprint", beforePrint);
    window.addEventListener("afterprint", afterPrint);
    return () => {
      window.removeEventListener("beforeprint", beforePrint);
      window.removeEventListener("afterprint", afterPrint);
    };
  }, []);

  if (session.state.loading) return <div className="block-editor__loading">正在加载文档…</div>;
  if (structure.root.length === 0) return <div className="block-editor__loading">{session.state.error ?? "文档不可用"}</div>;

  return (
    <OverlayCoordinator interaction={interaction}>
    <div ref={stageRef} className="block-editor__stage" role="document" aria-label="文档正文">
      {(printProjection?.sections ?? [{ sectionId: null, rootBlockIds: structure.root, pageSetup: structure.pageSetup, header: null, footer: null, pageNumbering: null }]).map((section) => (
        <article
          key={section.sectionId ?? "default-section"}
          className="block-editor__page"
          data-section-id={section.sectionId ?? undefined}
          aria-label={section.sectionId ? `文档节 ${section.sectionId}` : "文档编辑区"}
          style={pageStyle(section.pageSetup)}
          onContextMenu={(event) => event.preventDefault()}
          onPointerDownCapture={(event) => routeEditorPointerDown(event, interaction)}
          onKeyDownCapture={(event) => routeEditorKeyDown(event, { interaction, registry, session })}
        >
          {section.header && <PageBand kind="header" content={section.header.default} pageNumber={section.pageNumbering?.startAt ?? 1} />}
          {section.rootBlockIds.map((id, siblingIndex) => {
            const mounted = renderAllForPrint || visibleRootIds.has(id) || activeRootId === id;
            return (
              <div
                key={id}
                className="block-editor__virtual-slot"
                data-virtual-block-id={id}
                data-virtual-mounted={mounted ? "true" : "false"}
                aria-hidden={mounted ? undefined : true}
                style={!mounted ? { height: measuredHeightsRef.current.get(id) ?? DEFAULT_ROOT_HEIGHT } : undefined}
              >
                {mounted && (
                  <BlockNode
                    blockId={id}
                    store={store}
                    registry={registry}
                    session={session}
                    sessionKey={session.snapshot?.artifact.artifactId ?? ""}
                    interaction={interaction}
                    activeBlockId={session.state.activeBlockId}
                    depth={0}
                    listOrdinal={listOrdinalFor(section.rootBlockIds, siblingIndex, store)}
                  />
                )}
              </div>
            );
          })}
          {section.footer && <PageBand kind="footer" content={section.footer.default} pageNumber={section.pageNumbering?.startAt ?? 1} />}
        </article>
      ))}
      {printProjection && <DocumentNotes title="脚注" notes={printProjection.footnotes} />}
      {printProjection && <DocumentNotes title="尾注" notes={printProjection.endnotes} />}
    </div>
    </OverlayCoordinator>
  );
}

const INITIAL_MOUNTED_ROOTS = 16;
const DEFAULT_ROOT_HEIGHT = 32;

/** Keep the active root subtree mounted even when its slot leaves the viewport. */
function findContainingRoot(
  store: BlockProjectionStore,
  roots: readonly string[],
  activeBlockId: string | null,
): string | null {
  if (!activeBlockId) return null;
  const contains = (id: string): boolean => {
    if (id === activeBlockId) return true;
    return store.getBlock(id)?.children.some(contains) ?? false;
  };
  return roots.find(contains) ?? null;
}

function PageBand({ kind, content, pageNumber }: { kind: "header" | "footer"; content: HeaderFooterContent; pageNumber: number }) {
  return (
    <div className={`block-editor__${kind}`} aria-label={kind === "header" ? "页眉" : "页脚"}>
      {content.segments.map((segment, index) => {
        if (segment.type === "text") return <span key={index}>{segment.content.text}</span>;
        if (segment.type === "pageNumber") return <span key={index} aria-label="页码">{pageNumber}</span>;
        return <span key={index} aria-label="总页数">1</span>;
      })}
    </div>
  );
}

function DocumentNotes({ title, notes }: { title: string; notes: DocumentNote[] }) {
  if (notes.length === 0) return null;
  return (
    <aside className="block-editor__notes" aria-label={title}>
      <strong>{title}</strong>
      <ol>
        {notes.map((note) => <li key={note.id}>{note.content.map((content) => content.text).join("\n")}</li>)}
      </ol>
    </aside>
  );
}

function pageStyle(pageSetup: ArtifactPageSetup | null): CSSProperties | undefined {
  if (!pageSetup) return undefined;
  const toPx = (points: number) => `${points * 96 / 72}px`;
  return {
    width: toPx(pageSetup.width),
    minHeight: toPx(pageSetup.height),
    paddingTop: toPx(pageSetup.marginTop),
    paddingRight: toPx(pageSetup.marginRight),
    paddingBottom: toPx(pageSetup.marginBottom),
    paddingLeft: toPx(pageSetup.marginLeft),
  };
}
