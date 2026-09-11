import { withPresentationFont } from "./PresentationRichText.js";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { OpenOfficeSdk } from "@open-office/sdk";
import { resolveMultiSelectionControls } from "@open-office/presentation-ui";
import {
  deckPageSpecCommand,
  deckThemeCommand,
  deleteAnimationCommand,
  moveAnimationCommand,
  slideBackgroundCommand,
  slideLayoutCommand,
  slideNotesCommand,
  slideTransitionCommand,
  upsertAnimationCommand,
} from "./commands.js";
import { PresentationCommandBar } from "./PresentationCommandBar.js";
import { PresentationCanvasLayer } from "./PresentationCanvasLayer.js";
import { PresentationThumbnailNavigator } from "./PresentationThumbnails.js";
import { PresentationPlayback } from "./PresentationPlayback.js";
import { PresentationConnectorOverlay } from "./PresentationConnectorOverlay.js";
import { usePresentationSession } from "./usePresentationSession.js";
export { SlideNode } from "./PresentationStageNodes.js";
import { PresenceOverlay, PresentationSnapGuides, SlideNodeWithUi } from "./PresentationStageOverlays.js";
import {
  MultiNodeToolbar,
  MultiSelectionInspector,
  NodeToolbar,
  SlideToolbar,
  TableToolbar,
} from "./PresentationToolbars.js";
import { DeckInspector, SlideInspector } from "./PresentationDeckInspectors.js";
import { NodeInspector } from "./PresentationNodeInspectors.js";
import { tableAnchorsInSelection, type PresentationTableNode } from "./presentationTableSelection.js";
import {
  createNodeContext,
  presentationNodeUiRegistry as nodeUiRegistry,
} from "./presentationNodeContext.js";
import { usePresentationGestures } from "./usePresentationGestures.js";
import { usePresentationActions } from "./usePresentationActions.js";
import { usePresentationSelection } from "./usePresentationSelection.js";
import { usePresentationLauncher } from "./usePresentationLauncher.js";
import { api } from "../api.js";

const sdk = new OpenOfficeSdk();
export interface PresentationStudioProps {
  id: string;
  title: string;
  onBack: () => void;
}

/** Projection-first shell: render state never becomes a second Deck model. */
export function PresentationStudio({ id, title, onBack }: PresentationStudioProps) {
  const {
    selectedNodeId,
    selectedNodeIds,
    tableSelection,
    tableSelectionRef,
    editingNodeId,
    inspectorOpen,
    slideInspectorOpen,
    deckInspectorOpen,
    setSelectedNodeId,
    setSelectedNodeIds,
    setTableSelection,
    setEditingNodeId,
    setInspectorOpen,
    setSlideInspectorOpen,
    setDeckInspectorOpen,
    clearTableSelection,
    clearNodeSelection,
    reconcileSelection,
    selectNodes,
    selectTableCell,
  } = usePresentationSelection();
  const {
    data,
    activeSlideId,
    error,
    saving,
    availableCapabilities,
    capabilitiesLoaded,
    thumbnailDirtyIds,
    remotePresence,
    refresh,
    submit,
    submitHistory,
    setError,
    setSaving,
    updatePresenceCursor,
  } = usePresentationSession(id, selectedNodeIds);
  const playback = usePresentationLauncher(id);
  const [stageWidth, setStageWidth] = useState(920);
  const stageFrame = useRef<HTMLDivElement>(null);

  useLayoutEffect(() => {
    const target = stageFrame.current;
    if (!target) return undefined;
    const observer = new ResizeObserver(([entry]) => setStageWidth(Math.max(320, entry.contentRect.width - 64)));
    observer.observe(target);
    return () => observer.disconnect();
  }, []);

  useEffect(() => {
    const nodes = data?.activeSlide?.nodes ?? [];
    reconcileSelection(nodes);
  }, [data?.activeSlide, reconcileSelection]);

  const openSlide = useCallback((slideId: string) => {
    clearNodeSelection();
    void refresh(slideId).catch((reason: unknown) => setError(message(reason)));
  }, [clearNodeSelection, refresh, setError]);

  const slide = data?.activeSlide ?? null;
  const activeSlideIndex = slide && data ? data.slides.findIndex((candidate) => candidate.slideId === slide.slideId) : -1;
  const pageSpec = data?.deck.pageSpec;
  const stageHeight = pageSpec ? stageWidth * pageSpec.height / pageSpec.width : stageWidth * 9 / 16;
  const scale = pageSpec ? stageWidth / pageSpec.width : 1;
  const renderedNodes = slide?.nodes ?? [];

  const selectedNode = useMemo(
    () => renderedNodes.find((node) => node.id === selectedNodeId) ?? null,
    [renderedNodes, selectedNodeId],
  );

  const selectedNodes = useMemo(
    () => renderedNodes.filter((node) => selectedNodeIds.includes(node.id)),
    [renderedNodes, selectedNodeIds],
  );

  const activeTableSelection = useMemo(() => (
    selectedNode?.kind.type === "table" && tableSelection?.nodeId === selectedNode.id
      ? tableSelection
      : null
  ), [selectedNode, tableSelection]);

  const fontCells = selectedNode?.kind.type === "table" && activeTableSelection
    ? tableAnchorsInSelection(selectedNode as PresentationTableNode, activeTableSelection).filter(cell => cell.content.text.length > 0) : [];
  const fontBody = selectedNode?.kind.type === "text" ? selectedNode.kind.data.frame.body : fontCells[0]?.content;

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

  const multiSelectionControls = useMemo(
    () => resolveMultiSelectionControls(availableCapabilities, selectedNodes),
    [availableCapabilities, selectedNodes],
  );
  const hasMultiSelectionSurface = selectedNodeIds.length > 1 && multiSelectionControls.length > 0;

  const {
    preview,
    connectorPreview,
    snapGuides,
    effectiveTransform,
    pointerDown,
    connectorPointerDown,
    pointerMove,
    connectorPointerMove,
    recordPresenceCursor,
    pointerUp,
    connectorPointerUp,
    cancelConnectorPointer,
    stagePointerDown,
    keyboardNudge,
    keyboardNudgeCommit,
    resetPreviews,
  } = usePresentationGestures({
    artifactId: id,
    revision: data?.revision ?? null,
    slide,
    pageSpec,
    scale,
    nodes: renderedNodes,
    selectedNodeIds,
    availableCapabilities,
    submit,
    updatePresenceCursor,
    setSelectedNodeId,
    setSelectedNodeIds,
    selectNodes,
    clearTableSelection,
    setEditingNodeId,
    setInspectorOpen,
    setSlideInspectorOpen,
    setDeckInspectorOpen,
  });

  useEffect(() => resetPreviews(), [data?.activeSlide, resetPreviews]);

  const {
    imageInput,
    submitNodeAction,
    handleNodeToolbarAction,
    submitTableAction,
    handleMultiSelectionAction,
    createText,
    createShape,
    createConnector,
    createChart,
    insertImageFile,
    requestImageInsert,
    createSlide,
    createMaster,
    updateMaster,
    deleteMaster,
    createLayout,
    updateLayout,
    deleteLayout,
    duplicateActiveSlide,
    moveActiveSlide,
    deleteActiveSlide,
    submitSlideProperty,
    openSlideInspector,
    openDeckInspector,
    saveText,
  } = usePresentationActions({
    artifactId: id,
    data,
    slide,
    nodes: renderedNodes,
    selectedNode,
    selectedNodeIds,
    editingNodeId,
    availableCapabilities,
    saving,
    submit,
    setError,
    setSaving,
    tableSelectionRef,
    setTableSelection,
    setSelectedNodeId,
    setSelectedNodeIds,
    setEditingNodeId,
    setInspectorOpen,
    setSlideInspectorOpen,
    setDeckInspectorOpen,
  });

  const remotePresenceOnSlide = useMemo(() => remotePresence.filter((participant) => participant.slideId === slide?.slideId), [remotePresence, slide?.slideId]);

  if (playback.launch) return <PresentationPlayback artifactId={id} title={title} launch={playback.launch} onExit={playback.exit} />;

  return (
    <main className="presentation-studio" aria-label="演示文稿编辑器">
      <input
        ref={imageInput}
        className="presentation-studio__file-input"
        type="file"
        accept="image/*"
        tabIndex={-1}
        aria-hidden="true"
        onChange={(event) => {
          const file = event.currentTarget.files?.[0];
          if (file) void insertImageFile(file);
        }}
      />
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

      <PresentationCommandBar
        fontFamily={fontBody?.runs[0]?.style.fontFamily ?? ""}
        canSetFont={selectedNode?.kind.type === "text" ? availableCapabilities.has("presentation.setTextContent") : fontCells.length > 0 && availableCapabilities.has("presentation.setTableCellContent")}
        onFontFamily={fontFamily => {
          if (!selectedNode) return;
          if (selectedNode.kind.type === "text") {
            submitNodeAction(selectedNode, "text.content", withPresentationFont(selectedNode.kind.data.frame.body, fontFamily));
          } else if (slide && fontCells.length) {
            void submit(fontCells.map(cell => ({ typeId: "presentation.setTableCellContent", payload: { type: "setTableCellContent", slideId: slide.slideId, nodeId: selectedNode.id, row: cell.row, column: cell.column, content: withPresentationFont(cell.content, fontFamily) } })));
          }
        }}
        hasSlide={slide !== null}
        saving={saving}
        canUndo={data?.history.canUndo ?? false}
        canRedo={data?.history.canRedo ?? false}
        availableCapabilities={availableCapabilities}
        onUndo={() => submitHistory("undo")}
        onRedo={() => submitHistory("redo")}
        onCreateSlide={createSlide}
        onDuplicateSlide={duplicateActiveSlide}
        onInsertText={createText}
        onInsertShape={createShape}
        onInsertConnector={createConnector}
        onInsertChart={createChart}
        onInsertImage={requestImageInsert}
        onPlay={playback.startSingle}
        onPresenter={playback.startPresenter}
        exportHref={api.exportPptx(id)}
        onOpenDeckInspector={openDeckInspector}
        onMoveSlideBackward={() => moveActiveSlide(-1)}
        onMoveSlideForward={() => moveActiveSlide(1)}
        onDeleteSlide={deleteActiveSlide}
        canMoveSlideBackward={activeSlideIndex > 0}
        canMoveSlideForward={activeSlideIndex >= 0 && activeSlideIndex < (data?.slides.length ?? 0) - 1}
      />

      <section className={`presentation-studio__workspace ${hasMultiSelectionSurface || (inspectorOpen && selectedNodeUi && selectedNode) || (slideInspectorOpen && slide) || deckInspectorOpen ? "is-inspector-open" : ""}`}>
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
          <div className="presentation-studio__context-row">
            <span className="presentation-studio__active-slide-name">{slide?.name || "选择一张幻灯片"}</span>
            {hasMultiSelectionSurface ? (
              <MultiNodeToolbar controls={multiSelectionControls} disabled={saving} onAction={handleMultiSelectionAction} />
            ) : selectedNode?.kind.type === "table" && selectedNodeUi && activeTableSelection ? (
              <TableToolbar
                node={selectedNode as PresentationTableNode}
                selection={activeTableSelection}
                disabled={saving}
                availableCapabilities={availableCapabilities}
                onAction={submitTableAction}
                onOpenInspector={() => setInspectorOpen(true)}
              />
            ) : selectedNodeUi && selectedNode && (
              <NodeToolbar
                ui={selectedNodeUi}
                locked={selectedNode.locked}
                disabled={saving}
                downloadUrl={selectedNode.kind.type === "image" ? sdk.assetUrl(id, selectedNode.kind.data.assetId) : null}
                onAction={handleNodeToolbarAction}
                onOpenInspector={() => setInspectorOpen(true)}
              />
            )}
            {!selectedNode && slide && <SlideToolbar disabled={saving} availableCapabilities={availableCapabilities} onOpenInspector={openSlideInspector} />}
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
                tabIndex={0}
                aria-label="幻灯片舞台"
                style={{ width: stageWidth, height: stageHeight }}
                onPointerMove={(event) => { connectorPointerMove(event); pointerMove(event); recordPresenceCursor(event); }}
                onPointerUp={(event) => { connectorPointerUp(event); pointerUp(); }}
                onPointerCancel={() => { cancelConnectorPointer(); pointerUp(); }}
                onPointerDown={stagePointerDown}
                onKeyDown={keyboardNudge}
                onKeyUp={keyboardNudgeCommit}
              >
                <PresentationCanvasLayer nodes={renderedNodes} preview={preview} connectorPreview={connectorPreview} scale={scale} width={stageWidth} height={stageHeight} />
                <PresentationSnapGuides guides={snapGuides} scale={scale} />
                <PresentationConnectorOverlay
                  nodes={renderedNodes}
                  preview={preview}
                  connectorPreview={connectorPreview}
                  selectedNodeId={selectedNodeId}
                  scale={scale}
                  width={stageWidth}
                  height={stageHeight}
                  canEdit={availableCapabilities.has("presentation.setConnectorEndpoints") && !saving}
                  onSelect={selectNodes}
                  onEndpointPointerDown={connectorPointerDown}
                />
                <PresenceOverlay participants={remotePresenceOnSlide} nodes={renderedNodes} scale={scale} />
                <div className="presentation-studio__dom-layer" aria-label="可编辑文本层">
                  {renderedNodes.filter((node) => node.kind.type !== "connector").map((node) => (
                    <SlideNodeWithUi
                      key={node.id}
                      artifactId={id}
                      revision={data.revision}
                      slide={slide}
                      node={node}
                      transform={effectiveTransform(node)}
                      scale={scale}
                      selectedNodeIds={selectedNodeIds}
                      tableSelection={node.kind.type === "table" && tableSelection?.nodeId === node.id ? tableSelection : null}
                      editing={node.id === editingNodeId}
                      availableCapabilities={availableCapabilities}
                      onSelect={(extend) => selectNodes(node.id, extend)}
                      onEdit={() => node.kind.type === "text" && setEditingNodeId(node.id)}
                      onPointerDown={pointerDown}
                      onTableCellSelect={(address, extend) => {
                        if (node.kind.type === "table") selectTableCell(node as PresentationTableNode, address, extend);
                      }}
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
        <aside className={`presentation-studio__inspector ${hasMultiSelectionSurface || (inspectorOpen && selectedNodeUi && selectedNode) || (slideInspectorOpen && slide) || deckInspectorOpen ? "is-open" : ""}`} aria-label="属性检查器">
          {hasMultiSelectionSurface ? (
            <MultiSelectionInspector
              controls={multiSelectionControls}
              disabled={saving}
              onAction={handleMultiSelectionAction}
              onClose={() => {
                setSelectedNodeId(null);
                setSelectedNodeIds([]);
                setInspectorOpen(false);
              }}
            />
          ) : inspectorOpen && selectedNodeUi && selectedNode && slide ? (
            <NodeInspector
              ui={selectedNodeUi}
              node={selectedNode}
              slide={slide}
              artifactId={id}
              disabled={saving}
              availableCapabilities={availableCapabilities}
              onClose={() => setInspectorOpen(false)}
              onAction={(action, value) => selectedNode.kind.type === "table" ? submitTableAction(action, value) : submitNodeAction(selectedNode, action, value)}
              tableSelection={activeTableSelection}
              onTableSelectionChange={(selection) => {
                tableSelectionRef.current = selection;
                setTableSelection(selection);
              }}
              onAnimationUpsert={(animation) => void submit([upsertAnimationCommand(slide.slideId, animation)])}
              onAnimationDelete={(animationId) => void submit([deleteAnimationCommand(slide.slideId, animationId)])}
            />
          ) : slideInspectorOpen && slide && data ? (
            <SlideInspector
              slide={slide}
              deck={data.deck}
              selectedNodeId={selectedNode?.id ?? null}
              disabled={saving}
              availableCapabilities={availableCapabilities}
              onClose={() => setSlideInspectorOpen(false)}
              onNotesChange={(notes) => submitSlideProperty(slideNotesCommand(slide.slideId, notes))}
              onBackgroundChange={(background) => submitSlideProperty(slideBackgroundCommand(slide.slideId, background))}
              onLayoutChange={(layoutId) => submitSlideProperty(slideLayoutCommand(slide.slideId, layoutId))}
              onTransitionChange={(transition) => submitSlideProperty(slideTransitionCommand(slide.slideId, transition))}
              onAnimationUpsert={(animation) => void submit([upsertAnimationCommand(slide.slideId, animation)])}
              onAnimationDelete={(animationId) => void submit([deleteAnimationCommand(slide.slideId, animationId)])}
              onAnimationMove={(animationId, index) => void submit([moveAnimationCommand(slide.slideId, animationId, index)])}
            />
          ) : deckInspectorOpen && data ? (
            <DeckInspector
              deck={data.deck}
              disabled={saving}
              availableCapabilities={availableCapabilities}
              onClose={() => setDeckInspectorOpen(false)}
              onPageSpecChange={(pageSpec) => void submit([deckPageSpecCommand(pageSpec)])}
              onThemeChange={(theme) => void submit([deckThemeCommand(theme)])}
              onCreateMaster={createMaster}
              onUpdateMaster={updateMaster}
              onDeleteMaster={deleteMaster}
              onCreateLayout={createLayout}
              onUpdateLayout={updateLayout}
              onDeleteLayout={deleteLayout}
            />
          ) : (
            <div className="presentation-studio__inspector-empty">选择文本、形状、图片或组合对象以查看属性。</div>
          )}
        </aside>
      </section>
    </main>
  );
}

function message(reason: unknown) {
  return reason instanceof Error ? reason.message : String(reason);
}
