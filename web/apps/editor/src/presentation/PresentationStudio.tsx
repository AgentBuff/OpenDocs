import {
  type ComponentProps,
  type CSSProperties,
  type PointerEvent as ReactPointerEvent,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";

import { OpenOfficeSdk, isVersionConflict, type SemanticCommandInput } from "@open-office/sdk";
import {
  createBuiltinPresentationNodeRegistry,
  type BuiltinPresentationNodeAction,
  type PresentationNodeContext,
  type PresentationNodeAdornment,
  type ResolvedPresentationNodeUi,
} from "@open-office/presentation-ui";
import type {
  PresentationDeckProjection,
  ArtifactTransactionHistoryState,
  PresentationSlideOutlineItem,
  PresentationSlideProjection,
  PresentationPresenceParticipant,
} from "@open-office/schema/api";
import type {
  ColorRef,
  Paint,
  PresentationV5Node,
  PresentationV5NodeKind,
  PresentationV5RichText,
  PresentationV5Transform,
  ThemeColorToken,
} from "@open-office/schema";

import {
  createSlideCommand,
  createTextNode,
  insertTextNodeCommand,
  presentationHistoryTransaction,
  presentationSemanticInputs,
} from "./commands.js";
import { api } from "../api.js";
import { deriveCanvasRenderPlan, type CanvasRenderSnapshot } from "./canvas-render-plan.js";
import { PresentationThumbnailNavigator, thumbnailInvalidationIds } from "./PresentationThumbnails.js";
import { PresentationPlayback } from "./PresentationPlayback.js";
import {
  changedNodeTransforms,
  dragPreview,
  MIN_PRESENTATION_NODE_SIZE,
  nextNodeSelection,
} from "./interactions.js";

const sdk = new OpenOfficeSdk();
const ACTOR_ID = "presentation-web";
const nodeUiRegistry = createBuiltinPresentationNodeRegistry();

type StudioData = {
  deck: PresentationDeckProjection;
  slides: PresentationSlideOutlineItem[];
  activeSlide: PresentationSlideProjection | null;
  history: ArtifactTransactionHistoryState;
  revision: number;
};

type PresentationTransactionOrigin = "local" | "undo" | "redo";

type DragState = {
  /** A drag is a view gesture only; the final transforms are committed as one semantic batch. */
  nodeIds: readonly string[];
  mode: "move" | "resize";
  originClientX: number;
  originClientY: number;
  origins: Readonly<Record<string, PresentationV5Transform>>;
};

type PresentationTextNode = PresentationV5Node & { kind: Extract<PresentationV5NodeKind, { type: "text" }> };
type PresentationShapeNode = PresentationV5Node & { kind: Extract<PresentationV5NodeKind, { type: "shape" }> };
type PresentationImageNode = PresentationV5Node & { kind: Extract<PresentationV5NodeKind, { type: "image" }> };

export interface PresentationStudioProps {
  id: string;
  title: string;
  onBack: () => void;
}

/**
 * A Projection-first Presentation shell.
 *
 * The renderer owns only view state (selection, viewport scale and a pointer
 * preview). Deck facts are re-read from the v5 projection after every commit;
 * Canvas and the DOM text layer never become a second document model.
 */
export function PresentationStudio({ id, title, onBack }: PresentationStudioProps) {
  const [data, setData] = useState<StudioData | null>(null);
  const [activeSlideId, setActiveSlideId] = useState<string | null>(null);
  const [selectedNodeId, setSelectedNodeId] = useState<string | null>(null);
  // Selection is deliberately view state.  The Deck remains server-owned and
  // selection never leaks into a persisted node attribute.
  const [selectedNodeIds, setSelectedNodeIds] = useState<readonly string[]>([]);
  const [preview, setPreview] = useState<Record<string, PresentationV5Transform>>({});
  // React state renders the preview; the ref guarantees pointerup commits the
  // latest pointer sample even when it lands before React schedules a render.
  const previewRef = useRef<Record<string, PresentationV5Transform>>({});
  const [thumbnailDirtyIds, setThumbnailDirtyIds] = useState<readonly string[]>([]);
  const [editingNodeId, setEditingNodeId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [availableCapabilities, setAvailableCapabilities] = useState<ReadonlySet<string>>(() => new Set());
  const [capabilitiesLoaded, setCapabilitiesLoaded] = useState(false);
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const [playing, setPlaying] = useState(false);
  const [stageWidth, setStageWidth] = useState(920);
  // Collaboration state is a short-lived renderer projection. It never feeds
  // a semantic command, Deck refresh, undo stack, or persisted snapshot.
  const [remotePresence, setRemotePresence] = useState<readonly PresentationPresenceParticipant[]>([]);
  const presenceSessionId = useRef(createPresenceSessionId());
  const presenceCursor = useRef<{ x: number; y: number } | undefined>(undefined);
  const presenceState = useRef<{ slideId: string | null; selectedNodeIds: readonly string[] }>({ slideId: null, selectedNodeIds: [] });
  const presenceTimer = useRef<number | null>(null);
  const stageFrame = useRef<HTMLDivElement>(null);
  const drag = useRef<DragState | null>(null);

  presenceState.current = { slideId: activeSlideId, selectedNodeIds };

  const publishPresence = useCallback(() => {
    if (presenceTimer.current !== null) window.clearTimeout(presenceTimer.current);
    presenceTimer.current = window.setTimeout(() => {
      presenceTimer.current = null;
      const current = presenceState.current;
      // Best-effort only: a network failure must never block a local edit.
      void sdk.updatePresentationPresence(id, presenceSessionId.current, {
        ...(current.slideId ? { slideId: current.slideId } : {}),
        selectedNodeIds: [...current.selectedNodeIds],
        ...(presenceCursor.current ? { cursor: presenceCursor.current } : {}),
      }).catch(() => undefined);
    }, 100);
  }, [id]);

  useEffect(() => {
    publishPresence();
    return () => {
      if (presenceTimer.current !== null) window.clearTimeout(presenceTimer.current);
    };
  }, [activeSlideId, publishPresence, selectedNodeIds]);

  useEffect(() => {
    let disposed = false;
    const read = () => {
      void sdk.presentationPresence(id).then((page) => {
        if (!disposed) setRemotePresence(page.participants.filter((participant) => participant.sessionId !== presenceSessionId.current));
      }).catch(() => {
        // Presence is advisory. Keep the last projection while a peer or the
        // transient endpoint is unavailable; the editor itself remains usable.
      });
    };
    read();
    const timer = window.setInterval(read, 2_000);
    return () => { disposed = true; window.clearInterval(timer); };
  }, [id]);

  const refresh = useCallback(async (preferredSlideId?: string | null) => {
    const [deckEnvelope, outlineEnvelope, history] = await Promise.all([
      sdk.presentation(id),
      sdk.presentationOutline(id, { limit: 200, maxBytes: 256_000 }),
      sdk.history(id),
    ]);
    const slides = outlineEnvelope.data.items;
    const nextSlideId = preferredSlideId ?? activeSlideId ?? slides[0]?.slideId ?? null;
    const activeSlide = nextSlideId
      ? (await sdk.presentationSlide(id, nextSlideId, { include: ["nodes", "notes", "timeline"], maxBytes: 512_000 })).data
      : null;
    setData({
      deck: deckEnvelope.data,
      slides,
      activeSlide,
      // History is a compact canonical Artifact projection. It must be read on
      // refresh so a page reload never fabricates disabled controls.
      history,
      revision: Math.max(deckEnvelope.revision, outlineEnvelope.revision),
    });
    setActiveSlideId(nextSlideId);
    previewRef.current = {};
    setPreview({});
    setSelectedNodeId((current) => activeSlide?.nodes?.some((node) => node.id === current) ? current : null);
    setSelectedNodeIds((current) => current.filter((nodeId) => activeSlide?.nodes?.some((node) => node.id === nodeId)));
  }, [activeSlideId, id]);

  useEffect(() => {
    void refresh().catch((reason: unknown) => setError(message(reason)));
  }, [refresh]);

  useEffect(() => {
    let disposed = false;
    void sdk.capabilities().then((catalog) => {
      if (disposed) return;
      const presentation = catalog.artifacts.find((artifact) => artifact.kind === "presentation");
      setAvailableCapabilities(new Set(presentation?.commands.map((command) => command.typeId) ?? []));
      setCapabilitiesLoaded(true);
    }).catch((reason: unknown) => {
      if (!disposed) {
        setCapabilitiesLoaded(true);
        setError(message(reason));
      }
    });
    return () => { disposed = true; };
  }, []);

  useLayoutEffect(() => {
    const target = stageFrame.current;
    if (!target) return undefined;
    const observer = new ResizeObserver(([entry]) => setStageWidth(Math.max(320, entry.contentRect.width - 64)));
    observer.observe(target);
    return () => observer.disconnect();
  }, []);

  const openSlide = useCallback((slideId: string) => {
    setSelectedNodeId(null);
    setSelectedNodeIds([]);
    setEditingNodeId(null);
    void refresh(slideId).catch((reason: unknown) => setError(message(reason)));
  }, [refresh]);

  const submit = useCallback(async (
    commands: readonly SemanticCommandInput[],
    origin: PresentationTransactionOrigin = "local",
  ) => {
    if (!data || commands.length === 0) return;
    setSaving(true);
    setError(null);
    try {
      const result = await sdk.submit({
        artifactId: id,
        baseRevision: data.revision,
        actorId: ACTOR_ID,
        origin,
        commands,
      });
      // The engine emits thumbnail invalidation as a read-only domain event.
      // Do not infer it from toolbar actions or keep a Deck clone in this UI.
      setThumbnailDirtyIds(thumbnailInvalidationIds(result.events));
      await refresh(activeSlideId);
      setData((current) => current ? {
        ...current,
        history: { canUndo: result.canUndo, canRedo: result.canRedo },
      } : current);
    } catch (reason) {
      if (isVersionConflict(reason)) {
        await refresh(activeSlideId);
        setError("此演示文稿已更新，已按最新 revision/ETag 刷新；请重新执行操作。");
      } else {
        setError(message(reason));
      }
    } finally {
      setSaving(false);
    }
  }, [activeSlideId, data, id, refresh]);

  const submitHistory = useCallback((action: "undo" | "redo") => {
    if (!availableCapabilities.has("presentation.history") || !data || saving) return;
    const enabled = action === "undo" ? data.history.canUndo : data.history.canRedo;
    if (!enabled) return;
    const transaction = presentationHistoryTransaction(action);
    void submit(transaction.commands, transaction.origin);
  }, [availableCapabilities, data, saving, submit]);

  const slide = data?.activeSlide ?? null;
  const pageSpec = data?.deck.pageSpec;
  const stageHeight = pageSpec ? stageWidth * pageSpec.height / pageSpec.width : stageWidth * 9 / 16;
  const scale = pageSpec ? stageWidth / pageSpec.width : 1;
  const renderedNodes = slide?.nodes ?? [];

  const selectedNode = useMemo(
    () => renderedNodes.find((node) => node.id === selectedNodeId) ?? null,
    [renderedNodes, selectedNodeId],
  );

  const selectedNodeUi = useMemo(() => {
    if (!slide || !selectedNode || !data) return null;
    return nodeUiRegistry.resolve(createNodeContext({
      artifactId: id,
      revision: data.revision,
      slide,
      node: selectedNode,
      selectedNodeIds,
      mode: editingNodeId === selectedNode.id ? "text" : "node",
      availableCapabilities,
    }));
  }, [availableCapabilities, data, editingNodeId, id, selectedNode, selectedNodeIds, slide]);

  const effectiveTransform = useCallback((node: PresentationV5Node) => preview[node.id] ?? node.transform, [preview]);

  const submitNodeAction = useCallback((
    node: PresentationV5Node,
    action: BuiltinPresentationNodeAction,
    value?: unknown,
  ) => {
    if (!slide || !data) return;
    const context = createNodeContext({
      artifactId: id,
      revision: data.revision,
      slide,
      node,
      selectedNodeIds,
      mode: editingNodeId === node.id ? "text" : "node",
      availableCapabilities,
    });
    try {
      const commands = nodeUiRegistry.mapAction(action, { context, value });
      if (commands.length === 0) return;
      void submit(presentationSemanticInputs(commands));
    } catch (reason) {
      setError(message(reason));
    }
  }, [availableCapabilities, data, editingNodeId, id, selectedNodeIds, slide, submit]);

  const selectNodes = useCallback((nodeId: string, extend = false) => {
    setSelectedNodeId(nodeId);
    setSelectedNodeIds((current) => nextNodeSelection(current, nodeId, extend));
    setEditingNodeId(null);
    setInspectorOpen(true);
  }, []);

  const handleNodeToolbarAction = useCallback((action: BuiltinPresentationNodeAction) => {
    if (!selectedNode) return;
    if (action === "text.content") {
      setEditingNodeId(selectedNode.id);
      return;
    }
    if (action === "node.delete" || action === "group.ungroup") {
      submitNodeAction(selectedNode, action);
      return;
    }
    setInspectorOpen(true);
  }, [selectedNode, submitNodeAction]);

  const pointerDown = useCallback((event: ReactPointerEvent<HTMLElement>, node: PresentationV5Node, mode: DragState["mode"]) => {
    const extend = event.shiftKey || event.metaKey || event.ctrlKey;
    const nodeIds = mode === "resize"
      ? [node.id]
      : extend
      ? (selectedNodeIds.includes(node.id) ? selectedNodeIds : [...selectedNodeIds, node.id])
      : (selectedNodeIds.includes(node.id) ? selectedNodeIds : [node.id]);
    if (!extend) {
      setSelectedNodeId(node.id);
      setSelectedNodeIds(nodeIds);
    } else {
      selectNodes(node.id, true);
      // Modifier clicks change the selected set; they must not unexpectedly
      // begin an object drag while the user is building that set.
      return;
    }
    setEditingNodeId(null);
    setInspectorOpen(true);
    const canTransform = Boolean(slide && data && nodeUiRegistry.resolve(createNodeContext({
      artifactId: id,
      revision: data.revision,
      slide,
      node,
      selectedNodeIds: nodeIds,
      mode: "node",
      availableCapabilities,
    })).toolbar.some((item) => item.action === "node.transform"));
    if (node.locked || !pageSpec || !canTransform) return;
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    const origins = Object.fromEntries(renderedNodes
      .filter((candidate) => nodeIds.includes(candidate.id))
      .map((candidate) => [candidate.id, effectiveTransform(candidate)]));
    drag.current = {
      nodeIds,
      mode,
      originClientX: event.clientX,
      originClientY: event.clientY,
      origins,
    };
  }, [availableCapabilities, data, effectiveTransform, id, pageSpec, renderedNodes, selectNodes, selectedNodeIds, slide]);

  const pointerMove = useCallback((event: ReactPointerEvent<HTMLElement>) => {
    const current = drag.current;
    if (!current || !pageSpec) return;
    const nextPreview = dragPreview({
      nodeIds: current.nodeIds,
      mode: current.mode,
      origins: current.origins,
      deltaClientX: event.clientX - current.originClientX,
      deltaClientY: event.clientY - current.originClientY,
      scale,
    });
    previewRef.current = nextPreview;
    setPreview(nextPreview);
  }, [pageSpec, scale]);

  const recordPresenceCursor = useCallback((event: ReactPointerEvent<HTMLElement>) => {
    if (!pageSpec) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const x = (event.clientX - bounds.left) / scale;
    const y = (event.clientY - bounds.top) / scale;
    if (x < 0 || y < 0 || x > pageSpec.width || y > pageSpec.height) return;
    const previous = presenceCursor.current;
    // Do not turn pointer movement into a request stream.  Four canonical
    // slide units is visually stable while preserving a useful remote cursor.
    if (previous && Math.abs(previous.x - x) < 4 && Math.abs(previous.y - y) < 4) return;
    presenceCursor.current = { x, y };
    publishPresence();
  }, [pageSpec, publishPresence, scale]);

  const pointerUp = useCallback(() => {
    const current = drag.current;
    drag.current = null;
    if (!current || !slide) return;
    const latestPreview = previewRef.current;
    const commands = changedNodeTransforms(slide.nodes ?? [], latestPreview)
      .map(({ nodeId, transform }) => ({ type: "setNodeTransform" as const, slideId: slide.slideId, nodeId, transform }));
    if (commands.length === 0) {
      previewRef.current = {};
      setPreview({});
      return;
    }
    previewRef.current = {};
    void submit(presentationSemanticInputs(commands));
  }, [slide, submit]);

  const stagePointerDown = useCallback((event: ReactPointerEvent<HTMLElement>) => {
    // Canvas is the stage background; a direct click must dismiss object
    // selection without swallowing any text-editor event.
    if (event.target !== event.currentTarget && !(event.target instanceof HTMLCanvasElement)) return;
    setSelectedNodeId(null);
    setSelectedNodeIds([]);
    setEditingNodeId(null);
    setInspectorOpen(false);
  }, []);

  const createText = useCallback(() => {
    if (!slide) return;
    const id = randomId("text");
    const key = `ui-${Date.now()}-${id}`;
    void submit([insertTextNodeCommand(slide.slideId, createTextNode(id, key), slide.nodes?.length ?? 0)]);
  }, [slide, submit]);

  const createSlide = useCallback(() => {
    const index = data?.slides.length ?? 0;
    const slideId = randomId("slide");
    void submit([createSlideCommand(slideId, `ui-${Date.now()}-${slideId}`, index)]);
  }, [data?.slides.length, submit]);

  const saveText = useCallback((node: PresentationV5Node, text: string) => {
    if (!slide || node.kind.type !== "text") return;
    setEditingNodeId(null);
    const current = node.kind.data.frame.body.text;
    if (current !== text) submitNodeAction(node, "text.content", { text, runs: [] });
  }, [slide, submitNodeAction]);

  const remotePresenceOnSlide = useMemo(() => remotePresence.filter((participant) => participant.slideId === slide?.slideId), [remotePresence, slide?.slideId]);

  if (playing) return <PresentationPlayback artifactId={id} title={title} onExit={() => setPlaying(false)} />;

  return (
    <main className="presentation-studio" aria-label="演示文稿编辑器">
      <header className="presentation-studio__header">
        <button className="presentation-studio__back" type="button" onClick={onBack}>‹ 所有文件</button>
        <div className="presentation-studio__document-title">
          <span className="presentation-studio__eyebrow">演示文稿</span>
          <strong>{title || "未命名演示文稿"}</strong>
        </div>
        <div className="presentation-studio__status" aria-live="polite">
          {saving ? "正在保存…" : data ? `revision ${data.revision} · ${capabilitiesLoaded ? "已同步" : "正在读取能力…"}` : "正在加载…"}
        </div>
        <button type="button" className="presentation-studio__refresh" onClick={() => void refresh(activeSlideId)}>刷新</button>
      </header>

      {error && <div className="presentation-studio__error" role="alert">{error}</div>}

      <section className={`presentation-studio__workspace ${inspectorOpen && selectedNodeUi && selectedNode ? "is-inspector-open" : ""}`}>
        <nav className="presentation-studio__navigator" aria-label="幻灯片列表">
          <div className="presentation-studio__navigator-title">
            <span>幻灯片</span>
            <span>{data?.slides.length ?? 0}</span>
          </div>
          <div className="presentation-studio__slide-list">
            {data && <PresentationThumbnailNavigator
              artifactId={id}
              deck={data.deck}
              slides={data.slides}
              activeSlideId={activeSlideId}
              dirtySlideIds={thumbnailDirtyIds}
              onOpen={openSlide}
            />}
          </div>
        </nav>

        <section className="presentation-studio__canvas-area">
          <div className="presentation-studio__toolbar" aria-label="演示文稿工具栏">
            {capabilitiesLoaded && availableCapabilities.has("presentation.history") && (
              <div className="presentation-studio__history-actions" aria-label="历史操作">
                <button
                  type="button"
                  className="presentation-studio__history-action"
                  onClick={() => submitHistory("undo")}
                  disabled={!data?.history.canUndo || saving}
                  aria-label="撤销上一步"
                  title="撤销上一步"
                >
                  撤销
                </button>
                <button
                  type="button"
                  className="presentation-studio__history-action"
                  onClick={() => submitHistory("redo")}
                  disabled={!data?.history.canRedo || saving}
                  aria-label="重做上一步"
                  title="重做上一步"
                >
                  重做
                </button>
              </div>
            )}
            {slide ? (
              <button type="button" onClick={createText} disabled={saving}>添加文本</button>
            ) : (
              <button type="button" onClick={createSlide} disabled={!data || saving}>创建首张幻灯片</button>
            )}
            {data && <button type="button" onClick={() => setPlaying(true)}>播放</button>}
            <span className="presentation-studio__toolbar-divider" />
            <span>{slide?.name || "选择一张幻灯片"}</span>
            {selectedNodeUi && selectedNode && (
              <NodeToolbar
                ui={selectedNodeUi}
                disabled={saving}
                downloadUrl={selectedNode.kind.type === "image" ? sdk.assetUrl(id, selectedNode.kind.data.assetId) : null}
                onAction={handleNodeToolbarAction}
                onOpenInspector={() => setInspectorOpen(true)}
              />
            )}
            <span className="presentation-studio__toolbar-hint">
              {selectedNode
                ? selectedNodeUi?.toolbar.length ? "已选择对象 · 可在右侧检查器调整" : "此节点仅可读取；尚无可执行编辑能力"
                : "拖动移动 · 右下角调整大小 · 双击编辑文字"}
            </span>
          </div>

          <div className="presentation-studio__stage-frame" ref={stageFrame}>
            {pageSpec && slide ? (
              <div
                className="presentation-studio__stage"
                style={{ width: stageWidth, height: stageHeight }}
                onPointerMove={(event) => { pointerMove(event); recordPresenceCursor(event); }}
                onPointerUp={pointerUp}
                onPointerCancel={pointerUp}
                onPointerDown={stagePointerDown}
              >
                <CanvasLayer nodes={renderedNodes} preview={preview} scale={scale} width={stageWidth} height={stageHeight} />
                <PresenceOverlay participants={remotePresenceOnSlide} nodes={renderedNodes} scale={scale} />
                <div className="presentation-studio__dom-layer" aria-label="可编辑文本层">
                  {renderedNodes.map((node) => (
                    <SlideNodeWithUi
                      key={node.id}
                      artifactId={id}
                      revision={data.revision}
                      slide={slide}
                      node={node}
                      transform={effectiveTransform(node)}
                      scale={scale}
                      selectedNodeIds={selectedNodeIds}
                      editing={node.id === editingNodeId}
                      availableCapabilities={availableCapabilities}
                      onSelect={(extend) => selectNodes(node.id, extend)}
                      onEdit={() => node.kind.type === "text" && setEditingNodeId(node.id)}
                      onPointerDown={pointerDown}
                      onTextSave={saveText}
                    />
                  ))}
                </div>
              </div>
            ) : (
              <div className="presentation-studio__empty-stage">此演示文稿还没有幻灯片。请通过语义命令创建首张幻灯片。</div>
            )}
          </div>
        </section>
        <aside className={`presentation-studio__inspector ${inspectorOpen && selectedNodeUi && selectedNode ? "is-open" : ""}`} aria-label="对象检查器">
          {inspectorOpen && selectedNodeUi && selectedNode ? (
            <NodeInspector
              ui={selectedNodeUi}
              node={selectedNode}
              artifactId={id}
              disabled={saving}
              onClose={() => setInspectorOpen(false)}
              onAction={(action, value) => submitNodeAction(selectedNode, action, value)}
            />
          ) : (
            <div className="presentation-studio__inspector-empty">选择文本、形状、图片或组合对象以查看属性。</div>
          )}
        </aside>
      </section>
    </main>
  );
}

function PresenceOverlay({
  participants,
  nodes,
  scale,
}: {
  participants: readonly PresentationPresenceParticipant[];
  nodes: readonly PresentationV5Node[];
  scale: number;
}) {
  const nodeById = new Map(nodes.map((node) => [node.id, node]));
  return <div className="presentation-studio__presence-layer" aria-live="polite" aria-label="协作者状态">
    {participants.flatMap((participant) => participant.selectedNodeIds.map((nodeId) => {
      const node = nodeById.get(nodeId);
      if (!node) return null;
      const transform = node.transform;
      return <div
        key={`${participant.sessionId}:${nodeId}`}
        className="presentation-studio__remote-selection"
        style={{ left: transform.x * scale, top: transform.y * scale, width: transform.width * scale, height: transform.height * scale }}
        title={`${participant.displayName} 正在选择此对象`}
      />;
    }))}
    {participants.map((participant) => participant.cursor && (
      <div
        key={`${participant.sessionId}:cursor`}
        className="presentation-studio__remote-cursor"
        style={{ left: participant.cursor.x * scale, top: participant.cursor.y * scale }}
      >
        <span>{participant.displayName}</span>
      </div>
    ))}
  </div>;
}

function SlideNodeWithUi({
  artifactId,
  revision,
  slide,
  node,
  selectedNodeIds,
  editing,
  availableCapabilities,
  ...props
}: Omit<ComponentProps<typeof SlideNode>, "artifactId" | "adornments" | "unsupportedReason"> & {
  artifactId: string;
  revision: number;
  slide: PresentationSlideProjection;
  selectedNodeIds: readonly string[];
  editing: boolean;
  availableCapabilities: ReadonlySet<string>;
}) {
  const ui = nodeUiRegistry.resolve(createNodeContext({
    artifactId,
    revision,
    slide,
    node,
    selectedNodeIds,
    mode: editing ? "text" : "node",
    availableCapabilities,
  }));
  const unsupportedReason = ui.renderModel.kind === "unsupported" ? ui.renderModel.reason : null;
  return <SlideNode {...props} artifactId={artifactId} node={node} editing={editing} adornments={ui.adornments} unsupportedReason={unsupportedReason} />;
}

function NodeToolbar({
  ui,
  disabled,
  downloadUrl,
  onAction,
  onOpenInspector,
}: {
  ui: ResolvedPresentationNodeUi<BuiltinPresentationNodeAction>;
  disabled: boolean;
  /** Asset downloads are a read-only resource operation, not a document command. */
  downloadUrl: string | null;
  onAction: (action: BuiltinPresentationNodeAction) => void;
  onOpenInspector: () => void;
}) {
  return (
    <div className="presentation-studio__node-toolbar" aria-label={`${ui.inspector.title}工具栏`}>
      {ui.toolbar.map((item) => item.action && (
        <button
          type="button"
          className="presentation-studio__node-tool"
          key={item.id}
          aria-label={item.ariaLabel ?? item.label}
          title={item.label}
          disabled={disabled || !item.enabled}
          onClick={() => onAction(item.action as BuiltinPresentationNodeAction)}
        >
          {item.label}
        </button>
      ))}
      {downloadUrl && <a className="presentation-studio__node-tool" href={downloadUrl} download title="下载原始图片">下载</a>}
      <button
        className="presentation-studio__node-tool presentation-studio__node-tool--inspector"
        type="button"
        disabled={disabled}
        onClick={onOpenInspector}
      >属性</button>
    </div>
  );
}

function NodeInspector({
  ui,
  node,
  disabled,
  artifactId,
  onClose,
  onAction,
}: {
  ui: ResolvedPresentationNodeUi<BuiltinPresentationNodeAction>;
  node: PresentationV5Node;
  disabled: boolean;
  artifactId: string;
  onClose: () => void;
  onAction: (action: BuiltinPresentationNodeAction, value?: unknown) => void;
}) {
  const hasAction = (action: BuiltinPresentationNodeAction) => ui.inspector.fields.some((field) => field.action === action);
  const unavailable = ui.inspector.fields.filter((field) => field.kind === "readonly" && field.unavailableReason);
  return (
    <div className="presentation-studio__inspector-card">
      <header className="presentation-studio__inspector-header">
        <div><span>对象属性</span><strong>{ui.inspector.title}</strong></div>
        <button type="button" aria-label="关闭对象检查器" onClick={onClose}>×</button>
      </header>
      {node.kind.type === "text" && (
        <TextInspector node={node as PresentationTextNode} disabled={disabled} canEditContent={hasAction("text.content")} canEditFrame={hasAction("text.frame")} onAction={onAction} />
      )}
      {node.kind.type === "shape" && (
        <ShapeInspector node={node as PresentationShapeNode} disabled={disabled} canEditStyle={hasAction("shape.style")} onAction={onAction} />
      )}
      {node.kind.type === "image" && (
        <ImageInspector node={node as PresentationImageNode} disabled={disabled} canEditImage={hasAction("image.config")} artifactId={artifactId} onAction={onAction} />
      )}
      {node.kind.type === "group" && (
        <section className="presentation-studio__inspector-section">
          <h3>组合</h3>
          {hasAction("group.ungroup") ? (
            <button type="button" className="presentation-studio__inspector-danger" disabled={disabled} onClick={() => onAction("group.ungroup")}>取消组合</button>
          ) : <p>当前服务未声明取消组合能力。</p>}
        </section>
      )}
      {node.kind.type === "extension" && (
        <section className="presentation-studio__inspector-section">
          <h3>扩展节点</h3>
          <p>“{node.kind.data.namespace}” 以只读数据保留；未注册专属编辑器。</p>
        </section>
      )}
      {hasAction("node.transform") && (
        <TransformInspector node={node} disabled={disabled} onApply={(transform) => onAction("node.transform", transform)} />
      )}
      {unavailable.map((field) => (
        <p className="presentation-studio__inspector-unavailable" key={field.id}>{field.label}：{field.unavailableReason}</p>
      ))}
    </div>
  );
}

function TextInspector({ node, disabled, canEditContent, canEditFrame, onAction }: {
  node: PresentationTextNode;
  disabled: boolean;
  canEditContent: boolean;
  canEditFrame: boolean;
  onAction: (action: BuiltinPresentationNodeAction, value?: unknown) => void;
}) {
  const [text, setText] = useState(node.kind.data.frame.body.text);
  const [verticalAlign, setVerticalAlign] = useState(node.kind.data.frame.verticalAlign);
  const [style, setStyle] = useState(() => textStyleForInspector(node.kind.data.frame.body));
  useEffect(() => {
    setText(node.kind.data.frame.body.text);
    setVerticalAlign(node.kind.data.frame.verticalAlign);
    setStyle(textStyleForInspector(node.kind.data.frame.body));
  }, [node.id, node.kind.data]);
  const styledBody = (): PresentationV5RichText => ({
    text,
    // The inspector is a whole-text formatter.  It deliberately writes one
    // complete run instead of a partial range or an `attrs` patch, so the
    // canonical schema can validate coverage independently of this UI.
    runs: text.length ? [{ start: 0, end: [...text].length, style }] : [],
  });
  return <section className="presentation-studio__inspector-section">
    <h3>文字</h3>
    {canEditContent ? <label>内容<textarea value={text} disabled={disabled} onChange={(event) => setText(event.target.value)} /></label> : null}
    {canEditContent ? <fieldset className="presentation-studio__text-style" disabled={disabled}>
      <legend>整段样式</legend>
      <label><input type="checkbox" checked={style.bold} onChange={(event) => setStyle((current) => ({ ...current, bold: event.target.checked }))} /> 加粗</label>
      <label><input type="checkbox" checked={style.italic} onChange={(event) => setStyle((current) => ({ ...current, italic: event.target.checked }))} /> 倾斜</label>
      <label><input type="checkbox" checked={style.underline} onChange={(event) => setStyle((current) => ({ ...current, underline: event.target.checked }))} /> 下划线</label>
      <label><input type="checkbox" checked={style.strikethrough} onChange={(event) => setStyle((current) => ({ ...current, strikethrough: event.target.checked }))} /> 删除线</label>
      <label>字体<input value={style.fontFamily ?? ""} placeholder="默认字体" onChange={(event) => setStyle((current) => ({ ...current, fontFamily: event.target.value.trim() || null }))} /></label>
      <label>字号<input type="number" min="1" max="512" value={style.fontSize ?? ""} onChange={(event) => setStyle((current) => ({ ...current, fontSize: positiveNumberOrNull(event.target.value) }))} /></label>
      <label>文字颜色<input type="color" value={colorRefInputValue(style.color)} onChange={(event) => setStyle((current) => ({ ...current, color: solidColor(event.target.value) }))} /></label>
    </fieldset> : null}
    {canEditContent ? <button type="button" disabled={disabled} onClick={() => onAction("text.content", styledBody())}>应用文字与样式</button> : null}
    {canEditFrame ? <label>垂直对齐<select value={verticalAlign} disabled={disabled} onChange={(event) => setVerticalAlign(event.target.value as typeof verticalAlign)}><option value="top">顶端</option><option value="middle">居中</option><option value="bottom">底端</option></select></label> : null}
    {canEditFrame ? <button type="button" disabled={disabled} onClick={() => onAction("text.frame", { ...node.kind.data.frame, verticalAlign })}>应用文本框</button> : null}
  </section>;
}

function ShapeInspector({ node, disabled, canEditStyle, onAction }: {
  node: PresentationShapeNode;
  disabled: boolean;
  canEditStyle: boolean;
  onAction: (action: BuiltinPresentationNodeAction, value?: unknown) => void;
}) {
  const [fill, setFill] = useState(colorInputValue(node.kind.data.style.fill));
  const [fillEnabled, setFillEnabled] = useState(node.kind.data.style.fill.type === "solid");
  const [strokeEnabled, setStrokeEnabled] = useState(node.kind.data.style.stroke !== null);
  const [stroke, setStroke] = useState(colorRefInputValue(node.kind.data.style.stroke?.color ?? null));
  const [strokeWidth, setStrokeWidth] = useState(node.kind.data.style.stroke?.width ?? 1);
  useEffect(() => {
    setFill(colorInputValue(node.kind.data.style.fill));
    setFillEnabled(node.kind.data.style.fill.type === "solid");
    setStrokeEnabled(node.kind.data.style.stroke !== null);
    setStroke(colorRefInputValue(node.kind.data.style.stroke?.color ?? null));
    setStrokeWidth(node.kind.data.style.stroke?.width ?? 1);
  }, [node.id, node.kind.data]);
  const validStrokeWidth = Number.isFinite(strokeWidth) && strokeWidth > 0;
  return <section className="presentation-studio__inspector-section">
    <h3>形状样式</h3>
    {canEditStyle ? <label><input type="checkbox" checked={fillEnabled} disabled={disabled} onChange={(event) => setFillEnabled(event.target.checked)} /> 填充颜色<input type="color" value={fill} disabled={disabled || !fillEnabled} onChange={(event) => setFill(event.target.value)} /></label> : null}
    {canEditStyle ? <fieldset disabled={disabled}><legend><input type="checkbox" checked={strokeEnabled} onChange={(event) => setStrokeEnabled(event.target.checked)} /> 轮廓</legend><label>颜色<input type="color" value={stroke} disabled={!strokeEnabled} onChange={(event) => setStroke(event.target.value)} /></label><label>宽度<input type="number" min="0.1" step="0.1" value={strokeWidth} disabled={!strokeEnabled} onChange={(event) => setStrokeWidth(Number(event.target.value))} /></label></fieldset> : null}
    {canEditStyle ? <button type="button" disabled={disabled || (strokeEnabled && !validStrokeWidth)} onClick={() => onAction("shape.style", { fill: fillEnabled ? solidPaint(fill) : { type: "none" }, stroke: strokeEnabled ? { color: solidColor(stroke), width: strokeWidth } : null })}>应用样式</button> : null}
  </section>;
}

function ImageInspector({ node, disabled, canEditImage, artifactId, onAction }: {
  node: PresentationImageNode;
  disabled: boolean;
  canEditImage: boolean;
  artifactId: string;
  onAction: (action: BuiltinPresentationNodeAction, value?: unknown) => void;
}) {
  const [caption, setCaption] = useState(node.kind.data.caption ?? "");
  const [flipH, setFlipH] = useState(node.kind.data.flipH);
  const [flipV, setFlipV] = useState(node.kind.data.flipV);
  const [crop, setCrop] = useState(node.kind.data.crop);
  useEffect(() => {
    setCaption(node.kind.data.caption ?? "");
    setFlipH(node.kind.data.flipH);
    setFlipV(node.kind.data.flipV);
    setCrop(node.kind.data.crop);
  }, [node.id, node.kind.data]);
  const cropIsValid = crop.left >= 0 && crop.right >= 0 && crop.top >= 0 && crop.bottom >= 0
    && crop.left + crop.right < 1 && crop.top + crop.bottom < 1;
  const apply = () => onAction("image.config", { ...node.kind.data, caption: caption || null, crop, flipH, flipV });
  const restoreOriginal = () => {
    const originalAssetId = node.kind.data.originalAssetId;
    if (!originalAssetId) return;
    onAction("image.config", {
      ...node.kind.data,
      assetId: originalAssetId,
      originalAssetId: null,
      crop: { top: 0, right: 0, bottom: 0, left: 0 },
      flipH: false,
      flipV: false,
    });
  };
  return <section className="presentation-studio__inspector-section">
    <h3>图片</h3>
    <a className="presentation-studio__inspector-link" href={api.assetUrl(artifactId, node.kind.data.assetId)} download>下载原始图片</a>
    {canEditImage ? <label>题注<input value={caption} disabled={disabled} onChange={(event) => setCaption(event.target.value)} /></label> : null}
    {canEditImage ? <label><input type="checkbox" checked={flipH} disabled={disabled} onChange={(event) => setFlipH(event.target.checked)} /> 水平翻转</label> : null}
    {canEditImage ? <label><input type="checkbox" checked={flipV} disabled={disabled} onChange={(event) => setFlipV(event.target.checked)} /> 垂直翻转</label> : null}
    {canEditImage ? <fieldset className="presentation-studio__image-crop" disabled={disabled}>
      <legend>裁剪（百分比）</legend>
      {(["top", "right", "bottom", "left"] as const).map((edge) => <label key={edge}>{edge}<input type="number" min="0" max="0.99" step="0.01" value={crop[edge]} onChange={(event) => setCrop((current) => ({ ...current, [edge]: Number(event.target.value) }))} /></label>)}
    </fieldset> : null}
    {canEditImage ? <button type="button" disabled={disabled || !cropIsValid} onClick={apply}>应用图片设置</button> : null}
    {canEditImage && node.kind.data.originalAssetId ? <button type="button" disabled={disabled} onClick={restoreOriginal}>恢复原图</button> : null}
    <p>图片压缩：当前服务未注册不可逆的图片转码命令，因此不会显示伪功能。</p>
  </section>;
}

function TransformInspector({ node, disabled, onApply }: { node: PresentationV5Node; disabled: boolean; onApply: (transform: PresentationV5Transform) => void }) {
  const [transform, setTransform] = useState(node.transform);
  const update = (key: keyof PresentationV5Transform, value: string) => setTransform((current) => ({ ...current, [key]: Number(value) }));
  return <section className="presentation-studio__inspector-section">
    <h3>位置与大小</h3>
    <div className="presentation-studio__transform-grid">
      {(["x", "y", "width", "height", "rotation"] as const).map((key) => <label key={key}>{key}<input type="number" value={transform[key]} disabled={disabled} min={key === "width" || key === "height" ? MIN_PRESENTATION_NODE_SIZE : undefined} onChange={(event) => update(key, event.target.value)} /></label>)}
    </div>
    <button type="button" disabled={disabled || !isUsableTransform(transform)} onClick={() => onApply(transform)}>应用位置与大小</button>
  </section>;
}

export function SlideNode({
  artifactId,
  node,
  transform,
  scale,
  editing,
  adornments,
  unsupportedReason,
  onSelect,
  onEdit,
  onPointerDown,
  onTextSave,
}: {
  artifactId: string;
  node: PresentationV5Node;
  transform: PresentationV5Transform;
  scale: number;
  editing: boolean;
  adornments: readonly PresentationNodeAdornment[];
  unsupportedReason: string | null;
  onSelect: (extend?: boolean) => void;
  onEdit: () => void;
  onPointerDown: (event: ReactPointerEvent<HTMLElement>, node: PresentationV5Node, mode: "move" | "resize") => void;
  onTextSave: (node: PresentationV5Node, text: string) => void;
}) {
  const style = nodeStyle(transform, scale, node.opacity);
  const text = node.kind.type === "text" ? node.kind.data.frame.body.text : null;
  const outline = adornments.find((adornment): adornment is Extract<PresentationNodeAdornment, { kind: "outline" }> => adornment.kind === "outline");
  const selected = Boolean(outline);
  if (node.kind.type !== "text") {
    return (
      <div
        className={`presentation-studio__node presentation-studio__node--hit-target ${selected ? "is-selected" : ""}`}
        style={style}
        data-node-id={node.id}
        role="button"
        tabIndex={0}
        aria-label={`${node.name || node.kind.type}${selected ? "，已选择" : ""}`}
        aria-pressed={selected}
        onPointerDown={(event) => onPointerDown(event, node, "move")}
        // Pointer selection happens on down so drag starts from the same
        // stable target.  Do not toggle a second time on click.
        onClick={(event) => event.stopPropagation()}
        onKeyDown={(event) => {
          if (event.key === "Enter" || event.key === " ") {
            event.preventDefault();
            onSelect(event.shiftKey || event.metaKey || event.ctrlKey);
          }
        }}
      >
        {selected && outline?.handles.includes("southEast") && (
          <button
            className="presentation-studio__resize-handle"
            type="button"
            aria-label="调整对象大小"
            onPointerDown={(event) => { event.stopPropagation(); onPointerDown(event, node, "resize"); }}
          />
        )}
        {node.kind.type === "image" && <PresentationImage assetId={node.kind.data.assetId} crop={node.kind.data.crop} flipH={node.kind.data.flipH} flipV={node.kind.data.flipV} artifactId={artifactId} caption={node.kind.data.caption} />}
        {(node.kind.type === "video" || node.kind.type === "audio") && <PresentationMedia artifactId={artifactId} mediaType={node.kind.type} assetId={node.kind.data.assetId} posterAssetId={node.kind.data.posterAssetId} />}
        {unsupportedReason && <span className="presentation-studio__unsupported-node" title={unsupportedReason}>暂不支持</span>}
      </div>
    );
  }
  return (
    <div
      className={`presentation-studio__node ${selected ? "is-selected" : ""}`}
      style={style}
      data-node-id={node.id}
      role={editing ? undefined : "button"}
      tabIndex={editing ? undefined : 0}
      aria-label={`${node.name || "文本对象"}${selected ? "，已选择" : ""}`}
      aria-pressed={editing ? undefined : selected}
      onPointerDown={(event) => onPointerDown(event, node, "move")}
      onDoubleClick={(event) => { event.stopPropagation(); onEdit(); }}
      onClick={(event) => event.stopPropagation()}
      onKeyDown={(event) => {
        if (editing) return;
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onSelect(event.shiftKey || event.metaKey || event.ctrlKey);
        }
        if (event.key === "F2") {
          event.preventDefault();
          onEdit();
        }
      }}
    >
      {editing ? (
        <PresentationTextEditor node={node} initialText={text ?? ""} onSave={onTextSave} />
      ) : (
        <div className="presentation-studio__text-content">{text}</div>
      )}
      {selected && !editing && outline?.handles.includes("southEast") && (
        <button
          className="presentation-studio__resize-handle"
          type="button"
          aria-label="调整对象大小"
          onPointerDown={(event) => { event.stopPropagation(); onPointerDown(event, node, "resize"); }}
        />
      )}
    </div>
  );
}

/** DOM image layer intentionally consumes immutable asset URLs only. It never owns image data or edits. */
function PresentationImage({
  artifactId,
  assetId,
  crop,
  flipH,
  flipV,
  caption,
}: {
  artifactId: string;
  assetId: string;
  crop: PresentationImageNode["kind"]["data"]["crop"];
  flipH: boolean;
  flipV: boolean;
  caption: string | null;
}) {
  const visibleWidth = 1 - crop.left - crop.right;
  const visibleHeight = 1 - crop.top - crop.bottom;
  return <div className="presentation-studio__image-frame">
    <img
      className="presentation-studio__image-content"
      draggable={false}
      src={api.assetUrl(artifactId, assetId)}
      alt={caption ?? "演示文稿图片"}
      style={{
        width: `${100 / visibleWidth}%`,
        height: `${100 / visibleHeight}%`,
        left: `${-crop.left / visibleWidth * 100}%`,
        top: `${-crop.top / visibleHeight * 100}%`,
        transform: `scale(${flipH ? -1 : 1}, ${flipV ? -1 : 1})`,
      }}
    />
    {caption && <span className="presentation-studio__image-caption">{caption}</span>}
  </div>;
}

/** Media bytes remain server-owned immutable assets. DOM media elements only render their URL. */
function PresentationMedia({ artifactId, mediaType, assetId, posterAssetId }: {
  artifactId: string;
  mediaType: "video" | "audio";
  assetId: string;
  posterAssetId: string | null;
}) {
  const source = api.assetUrl(artifactId, assetId);
  if (mediaType === "audio") {
    return <audio className="presentation-studio__media presentation-studio__media--audio" controls preload="metadata" src={source} onPointerDown={(event) => event.stopPropagation()} />;
  }
  return <video className="presentation-studio__media presentation-studio__media--video" controls preload="metadata" poster={posterAssetId ? api.assetUrl(artifactId, posterAssetId) : undefined} src={source} onPointerDown={(event) => event.stopPropagation()} />;
}

/**
 * Native textarea editing deliberately owns its own draft while composition is
 * active.  A composition input event is provisional (not a semantic edit), so
 * it cannot submit until `compositionend` and blur have both settled.
 */
function PresentationTextEditor({ node, initialText, onSave }: {
  node: PresentationV5Node;
  initialText: string;
  onSave: (node: PresentationV5Node, text: string) => void;
}) {
  const [value, setValue] = useState(initialText);
  const valueRef = useRef(initialText);
  const composing = useRef(false);
  const blurred = useRef(false);
  const cancelled = useRef(false);

  useEffect(() => {
    setValue(initialText);
    valueRef.current = initialText;
  }, [initialText, node.id]);

  const commit = useCallback(() => {
    if (!cancelled.current) onSave(node, valueRef.current);
  }, [node, onSave]);

  return <textarea
    className="presentation-studio__text-editor"
    autoFocus
    aria-label="编辑文本对象"
    value={value}
    onPointerDown={(event) => event.stopPropagation()}
    onChange={(event) => {
      valueRef.current = event.currentTarget.value;
      setValue(event.currentTarget.value);
    }}
    onCompositionStart={() => { composing.current = true; }}
    onCompositionEnd={() => {
      composing.current = false;
      if (blurred.current) commit();
    }}
    onBlur={() => {
      blurred.current = true;
      if (!composing.current) commit();
    }}
    onKeyDown={(event) => {
      // Command/Ctrl+A and native clipboard shortcuts intentionally stay in
      // the textarea.  Their browser selection is more complete than a
      // synthetic Deck selection and the final text is submitted on blur.
      if (event.key === "Escape") {
        cancelled.current = true;
        event.currentTarget.blur();
      }
      if (event.key === "Enter" && (event.metaKey || event.ctrlKey)) event.currentTarget.blur();
    }}
  />;
}

function createNodeContext({
  artifactId,
  revision,
  slide,
  node,
  selectedNodeIds,
  mode,
  availableCapabilities,
}: {
  artifactId: string;
  revision: number;
  slide: PresentationSlideProjection;
  node: PresentationV5Node;
  selectedNodeIds: readonly string[];
  mode: "node" | "text";
  availableCapabilities: ReadonlySet<string>;
}): PresentationNodeContext {
  const nodes = slide.nodes ?? [];
  return {
    artifactId,
    revision,
    slideId: slide.slideId,
    node,
    childNodeIds: nodes
      .filter((candidate) => candidate.parentId === node.id)
      .sort((left, right) => left.orderKey.localeCompare(right.orderKey))
      .map((candidate) => candidate.id),
    selection: selectedNodeIds.length
      ? {
        refs: selectedNodeIds.map((nodeId) => ({ slideId: slide.slideId, nodeId })),
        primary: selectedNodeIds.length ? { slideId: slide.slideId, nodeId: selectedNodeIds[selectedNodeIds.length - 1]! } : null,
        mode,
      }
      : { refs: [], primary: null, mode: "node" },
    availableCapabilities,
  };
}

function colorInputValue(paint: Paint): string {
  if (paint.type !== "solid") return "#ffffff";
  const css = colorCss(paint.value);
  if (!css) return "#ffffff";
  if (/^#[\da-f]{6}$/i.test(css)) return css;
  const rgba = /^rgba\((\d+), (\d+), (\d+),/.exec(css);
  if (!rgba) return "#ffffff";
  return `#${[rgba[1], rgba[2], rgba[3]].map((part) => Number(part).toString(16).padStart(2, "0")).join("")}`;
}

function colorRefInputValue(color: ColorRef | null): string {
  return color ? colorInputValue({ type: "solid", value: color }) : "#000000";
}

function solidColor(hex: string): ColorRef {
  const normalized = /^#[\da-f]{6}$/i.test(hex) ? hex.slice(1) : "000000";
  return {
    type: "rgba",
    value: {
      r: Number.parseInt(normalized.slice(0, 2), 16),
      g: Number.parseInt(normalized.slice(2, 4), 16),
      b: Number.parseInt(normalized.slice(4, 6), 16),
      a: 255,
    },
  };
}

function positiveNumberOrNull(value: string): number | null {
  if (!value.trim()) return null;
  const parsed = Number(value);
  return Number.isFinite(parsed) && parsed > 0 && parsed <= 512 ? parsed : null;
}

function textStyleForInspector(body: PresentationV5RichText): PresentationV5RichText["runs"][number]["style"] {
  return body.runs[0]?.style ?? {
    bold: false,
    italic: false,
    underline: false,
    strikethrough: false,
    fontFamily: null,
    fontSize: null,
    color: null,
  };
}

function solidPaint(hex: string): Paint {
  return {
    type: "solid",
    value: solidColor(hex),
  };
}

function isUsableTransform(transform: PresentationV5Transform): boolean {
  return Number.isFinite(transform.x)
    && Number.isFinite(transform.y)
    && Number.isFinite(transform.rotation)
    && Number.isFinite(transform.width)
    && Number.isFinite(transform.height)
    && transform.width >= MIN_PRESENTATION_NODE_SIZE
    && transform.height >= MIN_PRESENTATION_NODE_SIZE;
}

function CanvasLayer({ nodes, preview, scale, width, height }: { nodes: readonly PresentationV5Node[]; preview: Readonly<Record<string, PresentationV5Transform>>; scale: number; width: number; height: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const lastSnapshot = useRef<CanvasRenderSnapshot | null>(null);
  useEffect(() => {
    const element = canvas.current;
    if (!element) return;
    const ratio = window.devicePixelRatio || 1;
    const plan = deriveCanvasRenderPlan({ previous: lastSnapshot.current, nodes, preview, width, height, scale });
    if (element.width !== Math.floor(width * ratio)) element.width = Math.floor(width * ratio);
    if (element.height !== Math.floor(height * ratio)) element.height = Math.floor(height * ratio);
    element.style.width = `${width}px`;
    element.style.height = `${height}px`;
    const context = element.getContext("2d");
    if (!context) return;
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    if (plan.kind === "noop") return;
    if (plan.kind === "full") {
      context.clearRect(0, 0, width, height);
      for (const node of plan.nodes) drawNode(context, node, preview[node.id] ?? node.transform, scale);
    } else if (plan.dirtyRect) {
      context.clearRect(plan.dirtyRect.x, plan.dirtyRect.y, plan.dirtyRect.width, plan.dirtyRect.height);
      context.save();
      context.beginPath();
      context.rect(plan.dirtyRect.x, plan.dirtyRect.y, plan.dirtyRect.width, plan.dirtyRect.height);
      context.clip();
      for (const node of plan.nodes) drawNode(context, node, preview[node.id] ?? node.transform, scale);
      context.restore();
    }
    lastSnapshot.current = plan.snapshot;
  }, [height, nodes, preview, scale, width]);
  return <canvas className="presentation-studio__canvas" ref={canvas} aria-hidden="true" />;
}

function drawNode(context: CanvasRenderingContext2D, node: PresentationV5Node, transform: PresentationV5Transform, scale: number) {
  // Images are rendered by the DOM image layer, which can load the immutable
  // Asset endpoint without turning Canvas into a second asset cache.
  if (!node.visible || node.kind.type === "text" || node.kind.type === "image") return;
  const { x, y, width, height, rotation } = transform;
  context.save();
  context.globalAlpha = node.opacity;
  context.translate((x + width / 2) * scale, (y + height / 2) * scale);
  context.rotate(rotation * Math.PI / 180);
  context.translate(-width * scale / 2, -height * scale / 2);
  if (node.kind.type === "shape") {
    context.fillStyle = paintColor(node.kind.data.style.fill) ?? "transparent";
    context.strokeStyle = colorCss(node.kind.data.style.stroke?.color) ?? "#5d6b82";
    context.lineWidth = Math.max(1, (node.kind.data.style.stroke?.width ?? 1) * scale);
    if (node.kind.data.geometry === "ellipse") {
      context.beginPath();
      context.ellipse(width * scale / 2, height * scale / 2, width * scale / 2, height * scale / 2, 0, 0, Math.PI * 2);
      context.fill();
      context.stroke();
    } else {
      context.fillRect(0, 0, width * scale, height * scale);
      context.strokeRect(0, 0, width * scale, height * scale);
    }
  } else if (node.kind.type === "table") {
    context.fillStyle = "rgba(255,255,255,.9)";
    context.fillRect(0, 0, width * scale, height * scale);
    context.strokeStyle = "#9eacc0";
    for (let row = 0; row <= node.kind.data.rows; row += 1) {
      const line = height * scale * row / node.kind.data.rows;
      context.beginPath(); context.moveTo(0, line); context.lineTo(width * scale, line); context.stroke();
    }
    for (let column = 0; column <= node.kind.data.columns; column += 1) {
      const line = width * scale * column / node.kind.data.columns;
      context.beginPath(); context.moveTo(line, 0); context.lineTo(line, height * scale); context.stroke();
    }
  } else {
    context.fillStyle = "#dce4ef";
    context.fillRect(0, 0, width * scale, height * scale);
    context.strokeStyle = "#9eacc0";
    context.strokeRect(0, 0, width * scale, height * scale);
  }
  context.restore();
}

function nodeStyle(transform: PresentationV5Transform, scale: number, opacity: number): CSSProperties {
  return {
    left: transform.x * scale,
    top: transform.y * scale,
    width: transform.width * scale,
    height: transform.height * scale,
    opacity,
    transform: `rotate(${transform.rotation}deg)`,
  };
}

function paintColor(paint: PresentationV5Node["kind"] extends never ? never : unknown): string | null {
  if (!paint || typeof paint !== "object") return null;
  const candidate = paint as { type?: string; value?: ColorRef };
  return candidate.type === "solid" ? colorCss(candidate.value) : null;
}

function colorCss(color: ColorRef | undefined | null): string | null {
  if (!color) return null;
  if (color.type === "rgba") return `rgba(${color.value.r}, ${color.value.g}, ${color.value.b}, ${color.value.a / 255})`;
  const themes: Record<ThemeColorToken, string> = {
    background: "#ffffff", text: "#192033", accent1: "#2458d3", accent2: "#17a88b", accent3: "#ef9f28", accent4: "#8b5cf6", accent5: "#ef5e8d", accent6: "#40a9ff", hyperlink: "#2458d3", followedHyperlink: "#7c4ec2",
  };
  return themes[color.value];
}

function randomId(prefix: string) {
  return `${prefix}-${globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`}`;
}

function createPresenceSessionId(): string {
  // Server validation intentionally accepts only this small portable alphabet.
  return randomId("presence").replace(/[^A-Za-z0-9_-]/g, "_");
}

function message(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}
