import { useCallback, useEffect, useState } from "react";

import { Editor } from "./Editor.js";
import { Home } from "./Home.js";
import { api, type ArtifactKind } from "./api.js";
import { PresentationStudio } from "./presentation/PresentationStudio.js";
import { SpreadsheetStudio } from "./spreadsheet/SpreadsheetStudio.js";
import { WhiteboardStudio } from "./whiteboard/WhiteboardStudio.js";
import { MindmapStudio } from "./mindmap/MindmapStudio.js";
import { PresentationPlayback } from "./presentation/PresentationPlayback.js";

/**
 * 当前只有列表和编辑器两个顶层视图，用一个状态切换就够了；增加权限分享、
 * 深层跳转或多 Artifact 工作区时再引入路由层。
 */
type View = { name: "list" } | { name: "viewer"; id: string; title: string; kind: ArtifactKind | null; warnings?: string[] };

/** 从 `?doc=<id>` 恢复视图，这样查看器的链接可以直接分享或刷新。 */
function initialView(): View {
  const id = new URLSearchParams(window.location.search).get("doc");
  return id ? { name: "viewer", id, title: "", kind: null } : { name: "list" };
}

function syncUrl(view: View) {
  const url = new URL(window.location.href);
  if (view.name === "viewer") {
    url.searchParams.set("doc", view.id);
  } else {
    url.searchParams.delete("doc");
  }
  window.history.replaceState(null, "", url);
}

export function App() {
  const [view, setView] = useState<View>(initialView);
  const audienceSession = new URLSearchParams(window.location.search).get("presentationMode") === "audience"
    ? new URLSearchParams(window.location.search).get("presentationSession")
    : null;

  useEffect(() => syncUrl(view), [view]);

  const openDocument = useCallback((id: string, title: string, kind: ArtifactKind, warnings?: string[]) => {
    setView({ name: "viewer", id, title, kind, warnings });
  }, []);

  const backToList = useCallback(() => setView({ name: "list" }), []);

  if (audienceSession && view.name === "viewer") return <PresentationPlayback artifactId={view.id} title={view.title} launch={{ mode: "audience", sessionId: audienceSession }} onExit={() => window.close()} />;
  return view.name === "list" ? <Home onOpen={openDocument} /> : <ArtifactViewer view={view} onBack={backToList} />;
}

function ArtifactViewer({ view, onBack }: { view: Extract<View, { name: "viewer" }>; onBack: () => void }) {
  const [kind, setKind] = useState<ArtifactKind | null>(view.kind);
  const [title, setTitle] = useState(view.title);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (view.kind) return;
    let disposed = false;
    void api.getMeta(view.id).then((meta) => {
      if (!disposed) {
        setKind(meta.kind);
        setTitle(meta.title);
      }
    }).catch((reason: unknown) => {
      if (!disposed) setError(reason instanceof Error ? reason.message : String(reason));
    });
    return () => { disposed = true; };
  }, [view.id, view.kind]);

  if (error) return <main className="viewer-error"><p>{error}</p><button type="button" onClick={onBack}>返回文件列表</button></main>;
  if (!kind) return <main className="viewer-error">正在识别文件类型…</main>;
  if (kind === "presentation") return <PresentationStudio id={view.id} title={title} onBack={onBack} />;
  if (kind === "spreadsheet") return <SpreadsheetStudio id={view.id} title={title} onBack={onBack} />;
  if (kind === "mindmap") return <MindmapStudio id={view.id} title={title} importWarnings={view.warnings} onBack={onBack} />;
  if (kind === "whiteboard") return <WhiteboardStudio id={view.id} title={title} onBack={onBack} />;
  if (kind === "document") return <Editor id={view.id} title={title} onBack={onBack} />;
  return <main className="viewer-error"><p>“{title || "此文件"}”的专用编辑器尚未接入。</p><button type="button" onClick={onBack}>返回文件列表</button></main>;
}
