import { useRef } from "react";
import type { CSSProperties } from "react";

import type { ArtifactPageSetup } from "@open-office/schema/artifact";

import type { BlockSessionApi } from "./hooks/useBlockSession.js";
import { useBlockProjectionStructure } from "./store/blockProjectionStore.js";
import { createEditorBlockRegistry } from "./blocks/editorRegistry.js";
import { BlockNode, listOrdinalFor } from "./blocks/BlockNode.js";
import type { BlockRegistry } from "./blocks/registry.js";

interface Props {
  session: BlockSessionApi;
}

/** Block Tree 的 DOM 视图。每个 block 都有稳定 id，行首操作区永远在正文左侧。 */
export function BlockEditor({ session }: Props) {
  const registryRef = useRef<BlockRegistry | null>(null);
  if (!registryRef.current) registryRef.current = createEditorBlockRegistry();
  const store = session.projection;
  const registry = registryRef.current;

  const structure = useBlockProjectionStructure(store);

  if (session.state.loading) return <div className="block-editor__loading">正在加载文档…</div>;
  if (structure.root.length === 0) return <div className="block-editor__loading">{session.state.error ?? "文档不可用"}</div>;

  return (
    <div className="block-editor__stage">
      <article className="block-editor__page" aria-label="文档编辑区" style={pageStyle(structure.pageSetup)} onContextMenu={(event) => event.preventDefault()}>
        {structure.root.map((id, siblingIndex) => (
          <BlockNode
            key={id}
            blockId={id}
            store={store}
            registry={registry}
            session={session}
            sessionKey={session.snapshot?.artifact.artifactId ?? ""}
            activeBlockId={session.state.activeBlockId}
            depth={0}
            listOrdinal={listOrdinalFor(structure.root, siblingIndex, store)}
          />
        ))}
      </article>
    </div>
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
