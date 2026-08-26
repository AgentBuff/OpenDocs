import { useEffect, useRef } from "react";
import type { CSSProperties } from "react";

import type { ArtifactPageSetup } from "@open-office/schema/artifact";

import type { BlockSessionApi } from "./hooks/useBlockSession.js";
import { useBlockProjectionStructure } from "./store/blockProjectionStore.js";
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

  if (session.state.loading) return <div className="block-editor__loading">正在加载文档…</div>;
  if (structure.root.length === 0) return <div className="block-editor__loading">{session.state.error ?? "文档不可用"}</div>;

  return (
    <OverlayCoordinator interaction={interaction}>
    <div className="block-editor__stage">
      <article
        className="block-editor__page"
        aria-label="文档编辑区"
        style={pageStyle(structure.pageSetup)}
        onContextMenu={(event) => event.preventDefault()}
        onPointerDownCapture={(event) => routeEditorPointerDown(event, interaction)}
        onKeyDownCapture={(event) => routeEditorKeyDown(event, { interaction, registry, session })}
      >
        {structure.root.map((id, siblingIndex) => (
          <BlockNode
            key={id}
            blockId={id}
            store={store}
            registry={registry}
            session={session}
            sessionKey={session.snapshot?.artifact.artifactId ?? ""}
            interaction={interaction}
            activeBlockId={session.state.activeBlockId}
            depth={0}
            listOrdinal={listOrdinalFor(structure.root, siblingIndex, store)}
          />
        ))}
      </article>
    </div>
    </OverlayCoordinator>
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
