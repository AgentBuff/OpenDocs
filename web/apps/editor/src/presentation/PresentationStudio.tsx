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
  Button,
  Checkbox,
  ColorPalette,
  Icon,
  IconButton,
  Input,
  Popover,
  Select,
  Textarea,
  Toolbar,
  ToolbarButton,
  ToolbarGroup,
  ToolbarSeparator,
  type IconName,
} from "@open-office/ui";
import {
  createBuiltinPresentationNodeRegistry,
  resolveMultiSelectionControls,
  type BuiltinPresentationNodeAction,
  type PresentationMultiSelectionAction,
  type PresentationMultiSelectionControl,
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
  ConnectorEndpoint,
  Paint,
  PresentationV5Node,
  PresentationV5NodeKind,
  PresentationV5ChartSpec,
  PresentationV5Deck,
  PresentationV5Layout,
  PresentationV5Master,
  PresentationV5RichText,
  PresentationV5SlideBackground,
  PresentationV5SlideTransition,
  PresentationV5TimelineEntry,
  PresentationV5Transform,
  ThemeColorToken,
} from "@open-office/schema";

import {
  createSlideCommand,
  createLayoutCommand,
  createMasterCommand,
  createChartNode,
  createImageNode,
  createConnectorNode,
  deckPageSpecCommand,
  deckThemeCommand,
  deleteLayoutCommand,
  deleteMasterCommand,
  deleteAnimationCommand,
  deleteSlideCommand,
  createShapeNode,
  createTextNode,
  duplicateSlideCommand,
  duplicatePresentationNode,
  insertNodeCommand,
  moveAnimationCommand,
  moveSlideCommand,
  multiNodeArrangeCommands,
  presentationHistoryTransaction,
  presentationSemanticInputs,
  registerPresentationAssetCommand,
  slideBackgroundCommand,
  slideLayoutCommand,
  slideNotesCommand,
  slideTransitionCommand,
  upsertAnimationCommand,
  updateLayoutCommand,
  updateMasterCommand,
} from "./commands.js";
import { PresentationCommandBar } from "./PresentationCommandBar.js";
import { api } from "../api.js";
import { deriveCanvasRenderPlan, type CanvasRenderSnapshot } from "./canvas-render-plan.js";
import { PresentationThumbnailNavigator, thumbnailInvalidationIds } from "./PresentationThumbnails.js";
import { PresentationPlayback } from "./PresentationPlayback.js";
import { TimelinePanel } from "./TimelinePanel.js";
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

/** Ephemeral endpoint preview. It is intentionally cleared before a semantic
 * transaction is submitted, so dragging never mutates the Deck in React. */
type ConnectorPreview = Record<string, { start: ConnectorEndpoint; end: ConnectorEndpoint }>;
type ConnectorDragState = { nodeId: string; endpoint: "start" | "end"; pointerId: number };

type PresentationTextNode = PresentationV5Node & { kind: Extract<PresentationV5NodeKind, { type: "text" }> };
type PresentationShapeNode = PresentationV5Node & { kind: Extract<PresentationV5NodeKind, { type: "shape" }> };
type PresentationImageNode = PresentationV5Node & { kind: Extract<PresentationV5NodeKind, { type: "image" }> };
type PresentationChartNode = PresentationV5Node & { kind: Extract<PresentationV5NodeKind, { type: "chart" }> };
type PresentationTableNode = PresentationV5Node & { kind: Extract<PresentationV5NodeKind, { type: "table" }> };
type PresentationConnectorNode = PresentationV5Node & { kind: Extract<PresentationV5NodeKind, { type: "connector" }> };
type TableCellAddress = { row: number; column: number };
type TableSelection = { nodeId: string; anchor: TableCellAddress; focus: TableCellAddress };

function tableRange(selection: Pick<TableSelection, "anchor" | "focus">) {
  return {
    start: { row: Math.min(selection.anchor.row, selection.focus.row), column: Math.min(selection.anchor.column, selection.focus.column) },
    end: { row: Math.max(selection.anchor.row, selection.focus.row), column: Math.max(selection.anchor.column, selection.focus.column) },
  };
}

function tableAnchorAt(node: PresentationTableNode, address: TableCellAddress) {
  return node.kind.data.cells.find((cell) =>
    address.row >= cell.row && address.row < cell.row + cell.rowSpan
    && address.column >= cell.column && address.column < cell.column + cell.columnSpan,
  ) ?? null;
}

/** Returns canonical top-left anchors that intersect the ephemeral grid range. */
function tableAnchorsInSelection(node: PresentationTableNode, selection: Pick<TableSelection, "anchor" | "focus">) {
  const range = tableRange(selection);
  return node.kind.data.cells.filter((cell) =>
    cell.row <= range.end.row && cell.row + cell.rowSpan - 1 >= range.start.row
    && cell.column <= range.end.column && cell.column + cell.columnSpan - 1 >= range.start.column,
  );
}

function tableSelectionCanMerge(node: PresentationTableNode, selection: Pick<TableSelection, "anchor" | "focus">) {
  const range = tableRange(selection);
  if (range.start.row === range.end.row && range.start.column === range.end.column) return false;
  return tableAnchorsInSelection(node, selection).every((cell) =>
    cell.row >= range.start.row && cell.column >= range.start.column
    && cell.row + cell.rowSpan - 1 <= range.end.row
    && cell.column + cell.columnSpan - 1 <= range.end.column
    && cell.rowSpan === 1 && cell.columnSpan === 1,
  );
}

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
  const [tableSelection, setTableSelection] = useState<TableSelection | null>(null);
  const tableSelectionRef = useRef<TableSelection | null>(null);
  const [preview, setPreview] = useState<Record<string, PresentationV5Transform>>({});
  // React state renders the preview; the ref guarantees pointerup commits the
  // latest pointer sample even when it lands before React schedules a render.
  const previewRef = useRef<Record<string, PresentationV5Transform>>({});
  const [connectorPreview, setConnectorPreview] = useState<ConnectorPreview>({});
  const connectorPreviewRef = useRef<ConnectorPreview>({});
  const [thumbnailDirtyIds, setThumbnailDirtyIds] = useState<readonly string[]>([]);
  const [editingNodeId, setEditingNodeId] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const [availableCapabilities, setAvailableCapabilities] = useState<ReadonlySet<string>>(() => new Set());
  const [capabilitiesLoaded, setCapabilitiesLoaded] = useState(false);
  const [inspectorOpen, setInspectorOpen] = useState(false);
  const [slideInspectorOpen, setSlideInspectorOpen] = useState(false);
  const [deckInspectorOpen, setDeckInspectorOpen] = useState(false);
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
  const imageInput = useRef<HTMLInputElement>(null);
  const drag = useRef<DragState | null>(null);
  const connectorDrag = useRef<ConnectorDragState | null>(null);

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
    connectorPreviewRef.current = {};
    setConnectorPreview({});
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
    preferredSlideId: string | null = activeSlideId,
  ) => {
    if (!data || commands.length === 0) return false;
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
      await refresh(preferredSlideId);
      setData((current) => current ? {
        ...current,
        history: { canUndo: result.canUndo, canRedo: result.canRedo },
      } : current);
      return true;
    } catch (reason) {
      if (isVersionConflict(reason)) {
        await refresh(activeSlideId);
        setError("此演示文稿已更新，已按最新 revision/ETag 刷新；请重新执行操作。");
      } else {
        setError(message(reason));
      }
      return false;
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
    tableSelectionRef.current = null;
    setTableSelection(null);
    setEditingNodeId(null);
    setSlideInspectorOpen(false);
    setDeckInspectorOpen(false);
    setInspectorOpen(true);
  }, []);

  const selectTableCell = useCallback((node: PresentationTableNode, address: TableCellAddress, extend = false) => {
    const canonical = tableAnchorAt(node, address);
    if (!canonical) return;
    const current = tableSelectionRef.current;
    const next: TableSelection = extend && current?.nodeId === node.id
      ? { ...current, focus: { row: canonical.row, column: canonical.column } }
      : { nodeId: node.id, anchor: { row: canonical.row, column: canonical.column }, focus: { row: canonical.row, column: canonical.column } };
    tableSelectionRef.current = next;
    setTableSelection(next);
    setSelectedNodeId(node.id);
    setSelectedNodeIds([node.id]);
    setEditingNodeId(null);
    setSlideInspectorOpen(false);
    setDeckInspectorOpen(false);
    setInspectorOpen(true);
  }, []);

  const handleNodeToolbarAction = useCallback((action: BuiltinPresentationNodeAction) => {
    if (!selectedNode) return;
    if (action === "node.duplicate") {
      if (!slide) return;
      const id = randomId(selectedNode.kind.type);
      const node = duplicatePresentationNode(selectedNode, id, `ui-${Date.now()}-${id}`);
      submitNodeAction(selectedNode, action, {
        type: "insertNode",
        slideId: slide.slideId,
        node,
        index: slide.nodes?.length ?? 0,
      });
      return;
    }
    if (action === "text.content") {
      setEditingNodeId(selectedNode.id);
      return;
    }
    if (action === "node.lock") {
      submitNodeAction(selectedNode, action, !selectedNode.locked);
      return;
    }
    if (action === "node.bringForward" || action === "node.sendBackward" || action === "node.bringToFront" || action === "node.sendToBack") {
      if (!slide) return;
      const siblings = (slide.nodes ?? [])
        .filter((node) => node.parentId === selectedNode.parentId)
        .sort((left, right) => left.orderKey.localeCompare(right.orderKey));
      const currentIndex = siblings.findIndex((node) => node.id === selectedNode.id);
      if (currentIndex < 0) return;
      const index = action === "node.bringForward"
        ? Math.min(siblings.length - 1, currentIndex + 1)
        : action === "node.sendBackward"
          ? Math.max(0, currentIndex - 1)
          : action === "node.bringToFront"
            ? siblings.length - 1
            : 0;
      if (index === currentIndex) return;
      submitNodeAction(selectedNode, action, { type: "reorderNode", slideId: slide.slideId, nodeId: selectedNode.id, index });
      return;
    }
    if (action === "node.delete" || action === "group.ungroup") {
      submitNodeAction(selectedNode, action);
      return;
    }
    setInspectorOpen(true);
  }, [selectedNode, slide, submitNodeAction]);

  const submitTableAction = useCallback((action: BuiltinPresentationNodeAction, value?: unknown) => {
    if (!selectedNode || selectedNode.kind.type !== "table") return;
    // Structural commands can move or remove the selected anchor. Clear the
    // ephemeral range before the authoritative projection is refreshed rather
    // than trying to repair it in the renderer.
    if (action === "table.insertRows" || action === "table.insertColumns" || action === "table.deleteRow" || action === "table.deleteColumn" || action === "table.mergeCells" || action === "table.splitCell") {
      tableSelectionRef.current = null;
      setTableSelection(null);
    }
    submitNodeAction(selectedNode, action, value);
  }, [selectedNode, submitNodeAction]);

  const handleMultiSelectionAction = useCallback((action: PresentationMultiSelectionAction) => {
    if (!slide || selectedNodeIds.length < 2) return;
    const commands = multiNodeArrangeCommands(slide.slideId, renderedNodes, selectedNodeIds, action);
    if (commands.length > 0) void submit(presentationSemanticInputs(commands));
  }, [renderedNodes, selectedNodeIds, slide, submit]);

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
      tableSelectionRef.current = null;
      setTableSelection(null);
    } else {
      selectNodes(node.id, true);
      // Modifier clicks change the selected set; they must not unexpectedly
      // begin an object drag while the user is building that set.
      return;
    }
    setEditingNodeId(null);
    setSlideInspectorOpen(false);
    setDeckInspectorOpen(false);
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

  const connectorPointerDown = useCallback((
    event: ReactPointerEvent<SVGCircleElement>,
    nodeId: string,
    endpoint: "start" | "end",
  ) => {
    const node = renderedNodes.find((candidate) => candidate.id === nodeId);
    if (!node || node.kind.type !== "connector" || node.locked || !availableCapabilities.has("presentation.setConnectorEndpoints")) return;
    event.preventDefault();
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    setSelectedNodeId(nodeId);
    setSelectedNodeIds([nodeId]);
    setEditingNodeId(null);
    setSlideInspectorOpen(false);
    setDeckInspectorOpen(false);
    setInspectorOpen(true);
    connectorDrag.current = { nodeId, endpoint, pointerId: event.pointerId };
  }, [availableCapabilities, renderedNodes]);

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

  const connectorPointerMove = useCallback((event: ReactPointerEvent<HTMLElement>) => {
    const current = connectorDrag.current;
    if (!current || current.pointerId !== event.pointerId || !pageSpec) return;
    const node = renderedNodes.find((candidate) => candidate.id === current.nodeId);
    if (!node || node.kind.type !== "connector") return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const x = clamp((event.clientX - bounds.left) / scale, 0, pageSpec.width);
    const y = clamp((event.clientY - bounds.top) / scale, 0, pageSpec.height);
    const previous = connectorPreviewRef.current[node.id] ?? node.kind.data;
    const next = {
      start: current.endpoint === "start" ? { type: "free" as const, value: { x, y } } : previous.start,
      end: current.endpoint === "end" ? { type: "free" as const, value: { x, y } } : previous.end,
    };
    connectorPreviewRef.current = { ...connectorPreviewRef.current, [node.id]: next };
    setConnectorPreview(connectorPreviewRef.current);
  }, [pageSpec, renderedNodes, scale]);

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

  const connectorPointerUp = useCallback((event: ReactPointerEvent<HTMLElement>) => {
    const current = connectorDrag.current;
    connectorDrag.current = null;
    if (!current || current.pointerId !== event.pointerId || !slide) return;
    const node = renderedNodes.find((candidate) => candidate.id === current.nodeId);
    if (!node || node.kind.type !== "connector") return;
    const draft = connectorPreviewRef.current[node.id] ?? node.kind.data;
    const endpoint = current.endpoint === "start" ? draft.start : draft.end;
    const snapped = endpoint.type === "free"
      ? snapConnectorEndpoint(endpoint, node.id, renderedNodes, previewRef.current)
      : endpoint;
    const next = current.endpoint === "start" ? { start: snapped, end: draft.end } : { start: draft.start, end: snapped };
    connectorPreviewRef.current = withoutConnectorPreview(connectorPreviewRef.current, node.id);
    setConnectorPreview(connectorPreviewRef.current);
    if (sameConnectorEndpoints(next, node.kind.data)) return;
    void submit(presentationSemanticInputs([{
      type: "setConnectorEndpoints",
      slideId: slide.slideId,
      nodeId: node.id,
      start: next.start,
      end: next.end,
    }]));
  }, [renderedNodes, slide, submit]);

  const cancelConnectorPointer = useCallback(() => {
    const current = connectorDrag.current;
    connectorDrag.current = null;
    if (!current) return;
    connectorPreviewRef.current = withoutConnectorPreview(connectorPreviewRef.current, current.nodeId);
    setConnectorPreview(connectorPreviewRef.current);
  }, []);

  const stagePointerDown = useCallback((event: ReactPointerEvent<HTMLElement>) => {
    // Canvas is the stage background; a direct click must dismiss object
    // selection without swallowing any text-editor event.
    if (event.target !== event.currentTarget && !(event.target instanceof HTMLCanvasElement)) return;
    setSelectedNodeId(null);
    setSelectedNodeIds([]);
    tableSelectionRef.current = null;
    setTableSelection(null);
    setEditingNodeId(null);
    setInspectorOpen(false);
    setSlideInspectorOpen(Boolean(slide));
    setDeckInspectorOpen(false);
  }, [slide]);

  const createText = useCallback(() => {
    if (!slide) return;
    const id = randomId("text");
    const key = `ui-${Date.now()}-${id}`;
    void submit([insertNodeCommand(slide.slideId, createTextNode(id, key), slide.nodes?.length ?? 0)]);
  }, [slide, submit]);

  const createShape = useCallback((geometry: "rectangle" | "ellipse" | "line" | "arrow") => {
    if (!slide) return;
    const id = randomId("shape");
    const key = `ui-${Date.now()}-${id}`;
    void submit([insertNodeCommand(slide.slideId, createShapeNode(id, key, geometry), slide.nodes?.length ?? 0)]);
  }, [slide, submit]);

  const createConnector = useCallback(() => {
    if (!slide || !availableCapabilities.has("presentation.insertNode")) return;
    const connectorId = randomId("connector");
    void submit([insertNodeCommand(
      slide.slideId,
      createConnectorNode(connectorId, `ui-${Date.now()}-${connectorId}`),
      slide.nodes?.length ?? 0,
    )]);
  }, [availableCapabilities, slide, submit]);

  const createChart = useCallback(() => {
    if (!slide || !availableCapabilities.has("presentation.insertNode") || !availableCapabilities.has("presentation.setChartSpec")) return;
    const chartId = randomId("chart");
    void submit([insertNodeCommand(slide.slideId, createChartNode(chartId, `ui-${Date.now()}-${chartId}`), slide.nodes?.length ?? 0)]);
  }, [availableCapabilities, slide, submit]);

  const insertImageFile = useCallback(async (file: File) => {
    if (!slide || !data || saving) return;
    if (!availableCapabilities.has("presentation.registerAsset") || !availableCapabilities.has("presentation.insertNode")) {
      setError("当前服务尚未声明图片插入能力。");
      return;
    }
    if (!file.type.startsWith("image/")) {
      setError("请选择标准图片文件。");
      return;
    }
    setSaving(true);
    setError(null);
    let uploaded: Awaited<ReturnType<typeof sdk.api.uploadAsset>> | null = null;
    try {
      uploaded = await sdk.api.uploadAsset(id, file, file.name);
      const asset = {
        assetId: uploaded.assetId,
        digest: uploaded.checksum,
        mimeType: uploaded.contentType,
        width: null,
        height: null,
        originalAssetId: null,
      };
      const nodeId = randomId("image");
      const node = createImageNode(nodeId, `ui-${Date.now()}-${nodeId}`, asset);
      const committed = await submit([
        registerPresentationAssetCommand(asset),
        insertNodeCommand(slide.slideId, node, slide.nodes?.length ?? 0),
      ], "local", slide.slideId);
      if (!committed) await sdk.api.deleteAsset(id, uploaded.assetId).catch(() => undefined);
    } catch (reason) {
      if (uploaded) await sdk.api.deleteAsset(id, uploaded.assetId).catch(() => undefined);
      setError(message(reason));
    } finally {
      setSaving(false);
      if (imageInput.current) imageInput.current.value = "";
    }
  }, [availableCapabilities, data, id, saving, slide, submit]);

  const requestImageInsert = useCallback(() => imageInput.current?.click(), []);

  const createSlide = useCallback(() => {
    const index = data?.slides.length ?? 0;
    const slideId = randomId("slide");
    void submit([createSlideCommand(slideId, `ui-${Date.now()}-${slideId}`, index)]);
  }, [data?.slides.length, submit]);

  /** These operate on complete schema entities read from the Deck projection. */
  const createMaster = useCallback(() => {
    const id = randomId("master");
    const master: PresentationV5Master = { id, name: "新建母版", background: { type: "none" }, placeholders: [] };
    void submit([createMasterCommand(master)]);
  }, [submit]);

  const updateMaster = useCallback((master: PresentationV5Master) => {
    void submit([updateMasterCommand(master)]);
  }, [submit]);

  const deleteMaster = useCallback((masterId: string) => {
    void submit([deleteMasterCommand(masterId)]);
  }, [submit]);

  const createLayout = useCallback((masterId: string) => {
    const id = randomId("layout");
    const layout: PresentationV5Layout = { id, masterId, name: "新建版式", placeholders: [] };
    void submit([createLayoutCommand(layout)]);
  }, [submit]);

  const updateLayout = useCallback((layout: PresentationV5Layout) => {
    void submit([updateLayoutCommand(layout)]);
  }, [submit]);

  const deleteLayout = useCallback((layoutId: string) => {
    void submit([deleteLayoutCommand(layoutId)]);
  }, [submit]);

  const duplicateActiveSlide = useCallback(() => {
    if (!slide || !data) return;
    const sourceIndex = data.slides.findIndex((candidate) => candidate.slideId === slide.slideId);
    if (sourceIndex < 0) return;
    const slideId = randomId("slide");
    const nodeIdMap = (slide.nodes ?? []).map((node) => ({
      sourceId: node.id,
      targetId: randomId("node"),
    }));
    const animationIdMap = (slide.timeline?.entries ?? []).map((entry) => ({
      sourceId: entry.id,
      targetId: randomId("animation"),
    }));
    const name = `${slide.name || "未命名幻灯片"} 副本`;
    void submit([
      duplicateSlideCommand(
        slide.slideId,
        slideId,
        `ui-${Date.now()}-${slideId}`,
        name,
        nodeIdMap,
        animationIdMap,
        sourceIndex + 1,
      ),
    ], "local", slideId);
  }, [data, slide, submit]);

  const moveActiveSlide = useCallback((offset: -1 | 1) => {
    if (!slide || !data) return;
    const index = data.slides.findIndex((candidate) => candidate.slideId === slide.slideId);
    const nextIndex = index + offset;
    if (index < 0 || nextIndex < 0 || nextIndex >= data.slides.length) return;
    void submit([moveSlideCommand(slide.slideId, nextIndex)], "local", slide.slideId);
  }, [data, slide, submit]);

  const deleteActiveSlide = useCallback(() => {
    if (!slide || !data) return;
    const index = data.slides.findIndex((candidate) => candidate.slideId === slide.slideId);
    if (index < 0) return;
    const fallbackSlideId = data.slides[index + 1]?.slideId ?? data.slides[index - 1]?.slideId ?? null;
    setSlideInspectorOpen(false);
    void submit([deleteSlideCommand(slide.slideId)], "local", fallbackSlideId);
  }, [data, slide, submit]);

  const submitSlideProperty = useCallback((command: SemanticCommandInput) => {
    if (!slide || saving) return;
    void submit([command]);
  }, [saving, slide, submit]);

  const openSlideInspector = useCallback(() => {
    if (!slide) return;
    setSelectedNodeId(null);
    setSelectedNodeIds([]);
    setEditingNodeId(null);
    setInspectorOpen(false);
    setDeckInspectorOpen(false);
    setSlideInspectorOpen(true);
  }, [slide]);

  const openDeckInspector = useCallback(() => {
    if (!data) return;
    setSelectedNodeId(null);
    setSelectedNodeIds([]);
    setEditingNodeId(null);
    setInspectorOpen(false);
    setSlideInspectorOpen(false);
    setDeckInspectorOpen(true);
  }, [data]);

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
          <PresentationCommandBar
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
            onPlay={() => setPlaying(true)}
            onOpenDeckInspector={openDeckInspector}
            onMoveSlideBackward={() => moveActiveSlide(-1)}
            onMoveSlideForward={() => moveActiveSlide(1)}
            onDeleteSlide={deleteActiveSlide}
            canMoveSlideBackward={activeSlideIndex > 0}
            canMoveSlideForward={activeSlideIndex >= 0 && activeSlideIndex < (data?.slides.length ?? 0) - 1}
          />
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
                style={{ width: stageWidth, height: stageHeight }}
                onPointerMove={(event) => { connectorPointerMove(event); pointerMove(event); recordPresenceCursor(event); }}
                onPointerUp={(event) => { connectorPointerUp(event); pointerUp(); }}
                onPointerCancel={() => { cancelConnectorPointer(); pointerUp(); }}
                onPointerDown={stagePointerDown}
              >
                <CanvasLayer nodes={renderedNodes} preview={preview} connectorPreview={connectorPreview} scale={scale} width={stageWidth} height={stageHeight} />
                <ConnectorOverlay
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
  tableSelection,
  editing,
  availableCapabilities,
  ...props
}: Omit<ComponentProps<typeof SlideNode>, "artifactId" | "adornments" | "unsupportedReason"> & {
  artifactId: string;
  revision: number;
  slide: PresentationSlideProjection;
  selectedNodeIds: readonly string[];
  tableSelection: TableSelection | null;
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
  return <SlideNode {...props} artifactId={artifactId} node={node} editing={editing} adornments={ui.adornments} unsupportedReason={unsupportedReason} tableSelection={tableSelection} />;
}

function MultiNodeToolbar({
  controls,
  disabled,
  onAction,
}: {
  controls: readonly PresentationMultiSelectionControl[];
  disabled: boolean;
  onAction: (action: PresentationMultiSelectionAction) => void;
}) {
  const alignment = controls.filter((control) => control.action.startsWith("selection.align"));
  const distribution = controls.filter((control) => control.action.startsWith("selection.distribute"));
  return <Toolbar className="presentation-studio__node-toolbar" aria-label="多对象排列工具栏">
    <ToolbarGroup aria-label="对齐对象">
      {alignment.map((control) => <ToolbarButton key={control.action} aria-label={control.label} title={control.label} disabled={disabled} onClick={() => onAction(control.action)}>
        <Icon name={control.icon as IconName} />
      </ToolbarButton>)}
    </ToolbarGroup>
    {distribution.length > 0 && <>
      <ToolbarSeparator />
      <ToolbarGroup aria-label="分布对象">
        {distribution.map((control) => <ToolbarButton key={control.action} aria-label={control.label} title={control.label} disabled={disabled} onClick={() => onAction(control.action)}>
          <Icon name={control.icon as IconName} />
        </ToolbarButton>)}
      </ToolbarGroup>
    </>}
  </Toolbar>;
}

/**
 * The contextual toolbar is intentionally compact; this companion inspector
 * makes the exact same capability-filtered arrangement surface discoverable
 * without inventing a second object-order state model.
 */
function MultiSelectionInspector({
  controls,
  disabled,
  onAction,
  onClose,
}: {
  controls: readonly PresentationMultiSelectionControl[];
  disabled: boolean;
  onAction: (action: PresentationMultiSelectionAction) => void;
  onClose: () => void;
}) {
  const alignment = controls.filter((control) => control.action.startsWith("selection.align"));
  const distribution = controls.filter((control) => control.action.startsWith("selection.distribute"));
  const ordering = controls.filter((control) => control.action.startsWith("selection.bring") || control.action.startsWith("selection.send"));
  const Section = ({ title, items }: { title: string; items: readonly PresentationMultiSelectionControl[] }) => items.length > 0 ? <section className="presentation-studio__inspector-section">
    <h3>{title}</h3>
    <div className="presentation-studio__arrange-actions">
      {items.map((control) => <Button key={control.action} type="button" size="sm" variant="secondary" disabled={disabled} onClick={() => onAction(control.action)}>
        <Icon name={control.icon as IconName} />{control.label}
      </Button>)}
    </div>
  </section> : null;
  return <div className="presentation-studio__inspector-card">
    <header className="presentation-studio__inspector-header">
      <div><span>已选择对象</span><strong>{controls[0] ? "排列" : ""}</strong></div>
      <IconButton type="button" variant="ghost" size="sm" aria-label="关闭多对象排列" title="关闭多对象排列" onClick={onClose}><Icon name="close" /></IconButton>
    </header>
    <Section title="对齐" items={alignment} />
    <Section title="分布" items={distribution} />
    <Section title="层级" items={ordering} />
  </div>;
}

function SlideToolbar({
  disabled,
  availableCapabilities,
  onOpenInspector,
}: {
  disabled: boolean;
  availableCapabilities: ReadonlySet<string>;
  onOpenInspector: () => void;
}) {
  const canConfigure = [
    "presentation.setSlideBackground",
    "presentation.setSlideNotes",
    "presentation.setSlideTransition",
  ].some((capability) => availableCapabilities.has(capability));
  if (!canConfigure) return null;
  return <Toolbar className="presentation-studio__node-toolbar" aria-label="幻灯片工具栏">
    <ToolbarGroup aria-label="幻灯片属性">
      <ToolbarButton aria-label="打开幻灯片属性" title="背景、备注与切换" disabled={disabled} onClick={onOpenInspector}>
        <Icon name="settings" />
      </ToolbarButton>
    </ToolbarGroup>
  </Toolbar>;
}

/** Table-only controls consume the transient grid range, while their effects
 * remain registry-owned semantic commands. This is intentionally separate
 * from the generic node toolbar: a table range is not a second scene-node
 * selection model. */
function TableToolbar({
  node,
  selection,
  disabled,
  availableCapabilities,
  onAction,
  onOpenInspector,
}: {
  node: PresentationTableNode;
  selection: TableSelection;
  disabled: boolean;
  availableCapabilities: ReadonlySet<string>;
  onAction: (action: BuiltinPresentationNodeAction, value?: unknown) => void;
  onOpenInspector: () => void;
}) {
  const range = tableRange(selection);
  const anchors = tableAnchorsInSelection(node, selection);
  const focused = tableAnchorAt(node, selection.focus);
  const can = (capability: string) => availableCapabilities.has(capability);
  const canMerge = can("presentation.mergeTableCells") && tableSelectionCanMerge(node, selection);
  const canSplit = can("presentation.splitTableCell") && anchors.length === 1 && Boolean(focused && (focused.rowSpan > 1 || focused.columnSpan > 1));
  return <Toolbar className="presentation-studio__node-toolbar" aria-label="表格工具栏">
    <ToolbarGroup aria-label="单元格">
      {can("presentation.setTableCellContent") && <ToolbarButton aria-label="编辑单元格" title="编辑单元格" disabled={disabled || anchors.length !== 1} onClick={onOpenInspector}><Icon name="text" /></ToolbarButton>}
      {can("presentation.setTableCellStyle") && <ToolbarButton aria-label="单元格样式" title="单元格样式" disabled={disabled} onClick={onOpenInspector}><Icon name="table" /></ToolbarButton>}
    </ToolbarGroup>
    {(can("presentation.insertTableRows") || can("presentation.insertTableColumns") || canMerge || canSplit) && <>
      <ToolbarSeparator />
      <ToolbarGroup aria-label="表格结构">
        {can("presentation.insertTableRows") && <ToolbarButton aria-label="在下方插入行" title="在下方插入行" disabled={disabled} onClick={() => onAction("table.insertRows", { index: range.end.row + 1, count: 1 })}><Icon name="insert-row-column" /></ToolbarButton>}
        {can("presentation.insertTableColumns") && <ToolbarButton aria-label="在右侧插入列" title="在右侧插入列" disabled={disabled} onClick={() => onAction("table.insertColumns", { index: range.end.column + 1, count: 1 })}><Icon name="insert-row-column" /></ToolbarButton>}
        {canMerge && <ToolbarButton aria-label="合并单元格" title="合并单元格" disabled={disabled} onClick={() => onAction("table.mergeCells", range)}><Icon name="merge-cells" /></ToolbarButton>}
        {canSplit && focused && <ToolbarButton aria-label="拆分单元格" title="拆分单元格" disabled={disabled} onClick={() => onAction("table.splitCell", { row: focused.row, column: focused.column })}><Icon name="split-cells" /></ToolbarButton>}
      </ToolbarGroup>
    </>}
    <ToolbarSeparator />
    <ToolbarGroup aria-label="对象属性">
      <ToolbarButton aria-label="打开表格属性" title="打开表格属性" disabled={disabled} onClick={onOpenInspector}><Icon name="settings" /></ToolbarButton>
    </ToolbarGroup>
  </Toolbar>;
}

function NodeToolbar({
  ui,
  locked,
  disabled,
  downloadUrl,
  onAction,
  onOpenInspector,
}: {
  ui: ResolvedPresentationNodeUi<BuiltinPresentationNodeAction>;
  locked: boolean;
  disabled: boolean;
  /** Asset downloads are a read-only resource operation, not a document command. */
  downloadUrl: string | null;
  onAction: (action: BuiltinPresentationNodeAction) => void;
  onOpenInspector: () => void;
}) {
  const iconForAction: Record<BuiltinPresentationNodeAction, IconName> = {
    "node.duplicate": "copy",
    "node.delete": "delete",
    "node.transform": "settings",
    "node.lock": locked ? "unlock" : "lock",
    "node.bringForward": "bring-forward",
    "node.sendBackward": "send-backward",
    "node.bringToFront": "bring-front",
    "node.sendToBack": "send-back",
    "shape.style": "shape",
    "shape.geometry": "shape",
    "chart.spec": "table",
    "connector.endpoints": "arrow-right",
    "table.cellContent": "text",
    "table.cellStyle": "table",
    "table.insertRows": "insert-row-column",
    "table.insertColumns": "insert-row-column",
    "table.deleteRow": "delete",
    "table.deleteColumn": "delete",
    "table.mergeCells": "merge-cells",
    "table.splitCell": "split-cells",
    "text.content": "text",
    "text.frame": "text",
    "image.config": "crop",
    "media.config": "settings",
    "group.ungroup": "split-cells",
  };
  return (
    <Toolbar className="presentation-studio__node-toolbar" aria-label={`${ui.inspector.title}工具栏`}>
      <ToolbarGroup aria-label={`${ui.inspector.title}操作`}>
        {ui.toolbar.map((item) => item.action && (
          <ToolbarButton
            key={item.id}
            aria-label={item.action === "node.lock" ? locked ? "解除锁定" : "锁定对象" : item.ariaLabel ?? item.label}
            title={item.action === "node.lock" ? locked ? "解除锁定" : "锁定对象" : item.label}
            disabled={disabled || !item.enabled}
            onClick={() => onAction(item.action as BuiltinPresentationNodeAction)}
          >
            <Icon name={iconForAction[item.action as BuiltinPresentationNodeAction]} />
          </ToolbarButton>
        ))}
        {downloadUrl && <a className="presentation-studio__node-tool presentation-studio__node-tool--download" href={downloadUrl} download title="下载原始图片" aria-label="下载原始图片"><Icon name="download" /></a>}
      </ToolbarGroup>
      <ToolbarSeparator />
      <ToolbarGroup aria-label="对象属性">
        <ToolbarButton aria-label="打开对象属性" title="对象属性" disabled={disabled} onClick={onOpenInspector}><Icon name="settings" /></ToolbarButton>
      </ToolbarGroup>
    </Toolbar>
  );
}

function SlideInspector({
  slide,
  deck,
  disabled,
  availableCapabilities,
  onClose,
  onNotesChange,
  onBackgroundChange,
  onLayoutChange,
  onTransitionChange,
  onAnimationUpsert,
  onAnimationDelete,
  onAnimationMove,
}: {
  slide: PresentationSlideProjection;
  deck: PresentationDeckProjection;
  disabled: boolean;
  availableCapabilities: ReadonlySet<string>;
  onClose: () => void;
  onNotesChange: (notes: string | null) => void;
  onBackgroundChange: (background: PresentationV5SlideBackground) => void;
  onLayoutChange: (layoutId: string | null) => void;
  onTransitionChange: (transition: PresentationV5SlideTransition | null) => void;
  onAnimationUpsert: (animation: PresentationV5TimelineEntry) => void;
  onAnimationDelete: (animationId: string) => void;
  onAnimationMove: (animationId: string, index: number) => void;
}) {
  const initialBackground = asSlideBackground(slide.background);
  const [notes, setNotes] = useState(slide.notes ?? "");
  const [backgroundEnabled, setBackgroundEnabled] = useState(initialBackground.type === "solid");
  const [backgroundColor, setBackgroundColor] = useState(colorRefInputValue(initialBackground.type === "solid" ? initialBackground.value : null));
  const [layoutId, setLayoutId] = useState(slide.layoutId ?? "");
  useEffect(() => {
    const nextBackground = asSlideBackground(slide.background);
    setNotes(slide.notes ?? "");
    setBackgroundEnabled(nextBackground.type === "solid");
    setBackgroundColor(colorRefInputValue(nextBackground.type === "solid" ? nextBackground.value : null));
    setLayoutId(slide.layoutId ?? "");
  }, [slide.background, slide.layoutId, slide.notes, slide.slideId]);
  const canNotes = availableCapabilities.has("presentation.setSlideNotes");
  const canBackground = availableCapabilities.has("presentation.setSlideBackground");
  const canLayout = availableCapabilities.has("presentation.setSlideLayout") && deck.layouts.length > 0;
  return <div className="presentation-studio__inspector-card">
    <header className="presentation-studio__inspector-header">
      <div><span>幻灯片属性</span><strong>{slide.name || "未命名幻灯片"}</strong></div>
      <IconButton type="button" variant="ghost" size="sm" aria-label="关闭幻灯片检查器" title="关闭幻灯片检查器" onClick={onClose}><Icon name="close" /></IconButton>
    </header>
    {canLayout && <section className="presentation-studio__inspector-section">
      <h3>版式</h3>
      <label>幻灯片版式<Select aria-label="幻灯片版式" disabled={disabled} value={layoutId} onChange={(event) => setLayoutId(event.target.value)}>
        <option value="">空白</option>
        {deck.masters.map((master) => {
          const layouts = deck.layouts.filter((layout) => layout.masterId === master.id);
          return layouts.length > 0 ? <optgroup key={master.id} label={master.name || "未命名母版"}>
            {layouts.map((layout) => <option key={layout.id} value={layout.id}>{layout.name || "未命名版式"}</option>)}
          </optgroup> : null;
        })}
      </Select></label>
      <Button type="button" size="sm" disabled={disabled} onClick={() => onLayoutChange(layoutId || null)}>应用版式</Button>
    </section>}
    {canBackground && <section className="presentation-studio__inspector-section">
      <h3>背景</h3>
      <div className="presentation-studio__inspector-control-group">
        <Checkbox checked={backgroundEnabled} disabled={disabled} onChange={(event) => setBackgroundEnabled(event.target.checked)}>使用纯色背景</Checkbox>
        <ColorPickerField ariaLabel="背景颜色" role="fill" value={backgroundColor} disabled={disabled || !backgroundEnabled} compact onValueChange={(value) => setBackgroundColor(value ?? "#ffffff")} />
      </div>
      <Button type="button" size="sm" disabled={disabled} onClick={() => onBackgroundChange(backgroundEnabled ? { type: "solid", value: solidColor(backgroundColor) } : { type: "none" })}>应用背景</Button>
    </section>}
    <TimelinePanel
      slide={slide}
      disabled={disabled}
      availableCapabilities={availableCapabilities}
      onTransitionChange={onTransitionChange}
      onAnimationUpsert={onAnimationUpsert}
      onAnimationDelete={onAnimationDelete}
      onAnimationMove={onAnimationMove}
    />
    {canNotes && <section className="presentation-studio__inspector-section">
      <h3>演讲者备注</h3>
      <Textarea value={notes} disabled={disabled} placeholder="仅在编辑与演讲者视图中可见" onChange={(event) => setNotes(event.target.value)} />
      <Button type="button" size="sm" disabled={disabled} onClick={() => onNotesChange(notes.trim() || null)}>保存备注</Button>
    </section>}
  </div>;
}

function DeckInspector({
  deck,
  disabled,
  availableCapabilities,
  onClose,
  onPageSpecChange,
  onThemeChange,
}: {
  deck: PresentationDeckProjection;
  disabled: boolean;
  availableCapabilities: ReadonlySet<string>;
  onClose: () => void;
  onPageSpecChange: (pageSpec: PresentationV5Deck["pageSpec"]) => void;
  onThemeChange: (theme: PresentationV5Deck["theme"]) => void;
}) {
  const initialFormat = deck.pageSpec.width / deck.pageSpec.height > 1.55 ? "wide" : "standard";
  const [format, setFormat] = useState(initialFormat);
  const [themeName, setThemeName] = useState(deck.themeName);
  useEffect(() => {
    setFormat(deck.pageSpec.width / deck.pageSpec.height > 1.55 ? "wide" : "standard");
    setThemeName(deck.themeName);
  }, [deck.pageSpec.height, deck.pageSpec.width, deck.themeName]);
  const canSetPageSpec = availableCapabilities.has("presentation.setPageSpec");
  const canSetTheme = availableCapabilities.has("presentation.setTheme");
  const formatSpec = format === "wide"
    ? { width: 12_192_000, height: 6_858_000 }
    : { width: 9_144_000, height: 6_858_000 };
  return <div className="presentation-studio__inspector-card">
    <header className="presentation-studio__inspector-header">
      <div><span>演示文稿</span><strong>设计</strong></div>
      <IconButton type="button" variant="ghost" size="sm" aria-label="关闭设计检查器" title="关闭设计检查器" onClick={onClose}><Icon name="close" /></IconButton>
    </header>
    {canSetPageSpec && <section className="presentation-studio__inspector-section">
      <h3>页面比例</h3>
      <label>幻灯片尺寸<Select disabled={disabled} value={format} onChange={(event) => setFormat(event.target.value as typeof format)}>
        <option value="wide">宽屏 16:9</option><option value="standard">标准 4:3</option>
      </Select></label>
      <Button type="button" size="sm" disabled={disabled} onClick={() => onPageSpecChange({ ...formatSpec, unit: deck.pageSpec.unit, safeArea: pageSafeArea(deck.pageSpec.safeArea) })}>应用页面比例</Button>
    </section>}
    {canSetTheme && <section className="presentation-studio__inspector-section">
      <h3>主题</h3>
      <label>主题名称<Input value={themeName} disabled={disabled} onChange={(event) => setThemeName(event.target.value)} /></label>
      <Button type="button" size="sm" disabled={disabled || !themeName.trim()} onClick={() => onThemeChange({ id: themeIdForName(themeName), name: themeName.trim() })}>应用主题</Button>
      <p>主题的字体和配色由 Deck 的严格主题引用解析；此入口不会重写幻灯片或对象样式。</p>
    </section>}
  </div>;
}

function ColorPickerField({
  label,
  ariaLabel,
  role,
  value,
  disabled,
  compact = false,
  onValueChange,
}: {
  label?: string;
  ariaLabel?: string;
  role: "text" | "fill" | "stroke";
  value: string;
  disabled: boolean;
  compact?: boolean;
  onValueChange: (value: string | null) => void;
}) {
  const trigger = <Button type="button" variant="secondary" size="sm" disabled={disabled} className={compact ? "presentation-studio__color-trigger presentation-studio__color-trigger--compact" : "presentation-studio__color-trigger"} aria-label={ariaLabel ?? label}>
    <span className="presentation-studio__color-swatch" style={{ backgroundColor: value }} aria-hidden="true" />
    {!compact && <span>{value.toUpperCase()}</span>}
  </Button>;
  const picker = <Popover placement="bottom-start" content={<ColorPalette role={role} value={value} onValueChange={onValueChange} />}>
    {trigger}
  </Popover>;
  return label ? <label>{label}{picker}</label> : picker;
}

function NodeInspector({
  ui,
  node,
  slide,
  disabled,
  artifactId,
  availableCapabilities,
  onClose,
  onAction,
  tableSelection,
  onTableSelectionChange,
  onAnimationUpsert,
  onAnimationDelete,
}: {
  ui: ResolvedPresentationNodeUi<BuiltinPresentationNodeAction>;
  node: PresentationV5Node;
  slide: PresentationSlideProjection;
  disabled: boolean;
  artifactId: string;
  availableCapabilities: ReadonlySet<string>;
  onClose: () => void;
  onAction: (action: BuiltinPresentationNodeAction, value?: unknown) => void;
  tableSelection: TableSelection | null;
  onTableSelectionChange: (selection: TableSelection | null) => void;
  onAnimationUpsert: (animation: PresentationV5TimelineEntry) => void;
  onAnimationDelete: (animationId: string) => void;
}) {
  const hasAction = (action: BuiltinPresentationNodeAction) => ui.inspector.fields.some((field) => field.action === action);
  const unavailable = ui.inspector.fields.filter((field) => field.kind === "readonly" && field.unavailableReason);
  return (
    <div className="presentation-studio__inspector-card">
      <header className="presentation-studio__inspector-header">
        <div><span>对象属性</span><strong>{ui.inspector.title}</strong></div>
        <IconButton type="button" variant="ghost" size="sm" aria-label="关闭对象检查器" title="关闭对象检查器" onClick={onClose}><Icon name="close" /></IconButton>
      </header>
      {node.kind.type === "text" && (
        <TextInspector node={node as PresentationTextNode} disabled={disabled} canEditContent={hasAction("text.content")} canEditFrame={hasAction("text.frame")} onAction={onAction} />
      )}
      {node.kind.type === "shape" && (
        <ShapeInspector node={node as PresentationShapeNode} disabled={disabled} canEditStyle={hasAction("shape.style")} canEditGeometry={hasAction("shape.geometry")} onAction={onAction} />
      )}
      {node.kind.type === "chart" && (
        <ChartInspector node={node as PresentationChartNode} disabled={disabled} canEdit={hasAction("chart.spec")} onAction={onAction} />
      )}
      {node.kind.type === "connector" && (
        <ConnectorInspector
          node={node as PresentationConnectorNode}
          nodes={slide.nodes ?? []}
          disabled={disabled}
          canEdit={hasAction("connector.endpoints")}
          onAction={onAction}
        />
      )}
      {node.kind.type === "table" && (
        <TableInspector
          node={node as PresentationTableNode}
          selection={tableSelection?.nodeId === node.id ? tableSelection : null}
          disabled={disabled}
          availableCapabilities={availableCapabilities}
          canEditContent={hasAction("table.cellContent")}
          canEditStyle={hasAction("table.cellStyle")}
          onSelectionChange={onTableSelectionChange}
          onAction={onAction}
        />
      )}
      {node.kind.type === "image" && (
        <ImageInspector node={node as PresentationImageNode} disabled={disabled} canEditImage={hasAction("image.config")} artifactId={artifactId} onAction={onAction} />
      )}
      {node.kind.type === "group" && (
        <section className="presentation-studio__inspector-section">
          <h3>组合</h3>
          {hasAction("group.ungroup") ? (
            <Button type="button" variant="danger" size="sm" disabled={disabled} onClick={() => onAction("group.ungroup")}>取消组合</Button>
          ) : <p>当前服务未声明取消组合能力。</p>}
        </section>
      )}
      {availableCapabilities.has("presentation.upsertAnimation") && (
        <NodeAnimationInspector
          nodeId={node.id}
          entries={(slide.timeline?.entries ?? []).filter((entry) => entry.targetNodeId === node.id)}
          disabled={disabled}
          canDelete={availableCapabilities.has("presentation.deleteAnimation")}
          onUpsert={onAnimationUpsert}
          onDelete={onAnimationDelete}
        />
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

function NodeAnimationInspector({
  nodeId,
  entries,
  disabled,
  canDelete,
  onUpsert,
  onDelete,
}: {
  nodeId: string;
  entries: readonly PresentationV5TimelineEntry[];
  disabled: boolean;
  canDelete: boolean;
  onUpsert: (animation: PresentationV5TimelineEntry) => void;
  onDelete: (animationId: string) => void;
}) {
  const existing = entries[0] ?? null;
  const [preset, setPreset] = useState<PresentationV5TimelineEntry["preset"]>(existing?.preset ?? "fade");
  const [trigger, setTrigger] = useState<PresentationV5TimelineEntry["trigger"]>(existing?.trigger ?? "onClick");
  const [durationMs, setDurationMs] = useState(existing?.durationMs ?? 300);
  const [delayMs, setDelayMs] = useState(existing?.delayMs ?? 0);
  useEffect(() => {
    setPreset(existing?.preset ?? "fade");
    setTrigger(existing?.trigger ?? "onClick");
    setDurationMs(existing?.durationMs ?? 300);
    setDelayMs(existing?.delayMs ?? 0);
  }, [existing?.delayMs, existing?.durationMs, existing?.id, existing?.preset, existing?.trigger, nodeId]);
  const boundedMs = (value: string) => Math.max(0, Math.min(600_000, Number(value) || 0));
  const animation: PresentationV5TimelineEntry = {
    id: existing?.id ?? `animation-${nodeId}`,
    targetNodeId: nodeId,
    trigger,
    preset,
    durationMs,
    delayMs,
    orderKey: existing?.orderKey ?? `ui-${Date.now()}-${nodeId}`,
  };
  return <section className="presentation-studio__inspector-section">
    <h3>入场动画</h3>
    <label>效果<Select disabled={disabled} value={preset} onChange={(event) => setPreset(event.target.value as typeof preset)}>
      <option value="appear">出现</option><option value="fade">淡入</option><option value="flyIn">飞入</option><option value="wipe">擦除</option>
    </Select></label>
    <label>触发<Select disabled={disabled} value={trigger} onChange={(event) => setTrigger(event.target.value as typeof trigger)}>
      <option value="onClick">单击时</option><option value="withPrevious">与上一动画同时</option><option value="afterPrevious">上一动画之后</option>
    </Select></label>
    <div className="presentation-studio__transform-grid">
      <label>时长（毫秒）<Input type="number" min="0" max="600000" disabled={disabled} value={durationMs} onChange={(event) => setDurationMs(boundedMs(event.target.value))} /></label>
      <label>延迟（毫秒）<Input type="number" min="0" max="600000" disabled={disabled} value={delayMs} onChange={(event) => setDelayMs(boundedMs(event.target.value))} /></label>
    </div>
    <div className="presentation-studio__inspector-actions">
      <Button type="button" size="sm" disabled={disabled} onClick={() => onUpsert(animation)}>{existing ? "更新动画" : "添加动画"}</Button>
      {existing && canDelete && <Button type="button" size="sm" variant="danger" disabled={disabled} onClick={() => onDelete(existing.id)}>移除动画</Button>}
    </div>
  </section>;
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
    {canEditContent ? <label>内容<Textarea value={text} disabled={disabled} onChange={(event) => setText(event.target.value)} /></label> : null}
    {canEditContent ? <fieldset className="presentation-studio__text-style" disabled={disabled}>
      <legend>整段样式</legend>
      <Checkbox checked={style.bold} onChange={(event) => setStyle((current) => ({ ...current, bold: event.target.checked }))}>加粗</Checkbox>
      <Checkbox checked={style.italic} onChange={(event) => setStyle((current) => ({ ...current, italic: event.target.checked }))}>倾斜</Checkbox>
      <Checkbox checked={style.underline} onChange={(event) => setStyle((current) => ({ ...current, underline: event.target.checked }))}>下划线</Checkbox>
      <Checkbox checked={style.strikethrough} onChange={(event) => setStyle((current) => ({ ...current, strikethrough: event.target.checked }))}>删除线</Checkbox>
      <label>字体<Input value={style.fontFamily ?? ""} placeholder="默认字体" onChange={(event) => setStyle((current) => ({ ...current, fontFamily: event.target.value.trim() || null }))} /></label>
      <label>字号<Input type="number" min="1" max="512" value={style.fontSize ?? ""} onChange={(event) => setStyle((current) => ({ ...current, fontSize: positiveNumberOrNull(event.target.value) }))} /></label>
      <ColorPickerField label="文字颜色" role="text" value={colorRefInputValue(style.color)} disabled={disabled} onValueChange={(value) => setStyle((current) => ({ ...current, color: solidColor(value ?? "#000000") }))} />
    </fieldset> : null}
    {canEditContent ? <Button type="button" size="sm" disabled={disabled} onClick={() => onAction("text.content", styledBody())}>应用文字与样式</Button> : null}
    {canEditFrame ? <label>垂直对齐<Select value={verticalAlign} disabled={disabled} onChange={(event) => setVerticalAlign(event.target.value as typeof verticalAlign)}><option value="top">顶端</option><option value="middle">居中</option><option value="bottom">底端</option></Select></label> : null}
    {canEditFrame ? <Button type="button" size="sm" disabled={disabled} onClick={() => onAction("text.frame", { ...node.kind.data.frame, verticalAlign })}>应用文本框</Button> : null}
  </section>;
}

function ShapeInspector({ node, disabled, canEditStyle, canEditGeometry, onAction }: {
  node: PresentationShapeNode;
  disabled: boolean;
  canEditStyle: boolean;
  canEditGeometry: boolean;
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
    {canEditGeometry ? <label>形状<Select value={node.kind.data.geometry} disabled={disabled} onChange={(event) => onAction("shape.geometry", event.target.value)}>
      <option value="rectangle">矩形</option>
      <option value="ellipse">圆形</option>
      <option value="line">直线</option>
      <option value="arrow">箭头</option>
    </Select></label> : null}
    {canEditStyle ? <div className="presentation-studio__inspector-control-group">
      <Checkbox checked={fillEnabled} disabled={disabled} onChange={(event) => setFillEnabled(event.target.checked)}>填充颜色</Checkbox>
      <ColorPickerField ariaLabel="填充颜色" role="fill" value={fill} disabled={disabled || !fillEnabled} compact onValueChange={(value) => setFill(value ?? "#ffffff")} />
    </div> : null}
    {canEditStyle ? <fieldset className="presentation-studio__shape-stroke" disabled={disabled}>
      <legend>轮廓</legend>
      <Checkbox checked={strokeEnabled} onChange={(event) => setStrokeEnabled(event.target.checked)}>显示轮廓</Checkbox>
      <ColorPickerField label="颜色" role="stroke" value={stroke} disabled={disabled || !strokeEnabled} onValueChange={(value) => setStroke(value ?? "#000000")} />
      <label>宽度<Input type="number" min="0.1" step="0.1" value={strokeWidth} disabled={!strokeEnabled} onChange={(event) => setStrokeWidth(Number(event.target.value))} /></label>
    </fieldset> : null}
    {canEditStyle ? <Button type="button" size="sm" disabled={disabled || (strokeEnabled && !validStrokeWidth)} onClick={() => onAction("shape.style", { fill: fillEnabled ? solidPaint(fill) : { type: "none" }, stroke: strokeEnabled ? { color: solidColor(stroke), width: strokeWidth } : null })}>应用样式</Button> : null}
  </section>;
}

/**
 * Form draft only: the saved value is always one complete ChartSpec command.
 * A compact line format keeps the inspector usable without introducing an
 * unvalidated, renderer-owned chart data table.
 */
function ChartInspector({ node, disabled, canEdit, onAction }: {
  node: PresentationChartNode;
  disabled: boolean;
  canEdit: boolean;
  onAction: (action: BuiltinPresentationNodeAction, value?: unknown) => void;
}) {
  const spec = node.kind.data.spec;
  const [chartType, setChartType] = useState(spec.chartType);
  const [title, setTitle] = useState(spec.title ?? "");
  const [categories, setCategories] = useState(spec.categories.join(", "));
  const [seriesText, setSeriesText] = useState(chartSeriesDraft(spec));
  const [draftError, setDraftError] = useState<string | null>(null);
  useEffect(() => {
    setChartType(spec.chartType);
    setTitle(spec.title ?? "");
    setCategories(spec.categories.join(", "));
    setSeriesText(chartSeriesDraft(spec));
    setDraftError(null);
  }, [node.id, spec]);

  const save = () => {
    try {
      const next = parseChartInspectorDraft({ chartType, title, categories, seriesText, previous: spec });
      setDraftError(null);
      onAction("chart.spec", next);
    } catch (reason) {
      setDraftError(message(reason));
    }
  };

  return <section className="presentation-studio__inspector-section">
    <h3>图表数据</h3>
    <label>类型<Select aria-label="图表类型" disabled={disabled || !canEdit} value={chartType} onChange={(event) => setChartType(event.target.value as PresentationV5ChartSpec["chartType"])}>
      <option value="column">柱状图</option><option value="bar">条形图</option><option value="line">折线图</option><option value="pie">饼图</option>
    </Select></label>
    <label>标题<Input aria-label="图表标题" disabled={disabled || !canEdit} value={title} placeholder="可选" onChange={(event) => setTitle(event.target.value)} /></label>
    <label>分类（逗号分隔）<Input aria-label="图表分类" disabled={disabled || !canEdit} value={categories} placeholder="Q1, Q2, Q3" onChange={(event) => setCategories(event.target.value)} /></label>
    <label>数据系列（每行：名称: 数值, 数值）<Textarea aria-label="图表数据系列" disabled={disabled || !canEdit} value={seriesText} placeholder={"营收: 42, 68, 54\n成本: 20, 32, 28"} onChange={(event) => setSeriesText(event.target.value)} /></label>
    {draftError && <p className="presentation-studio__inspector-unavailable" role="alert">{draftError}</p>}
    {canEdit && <Button type="button" size="sm" disabled={disabled} onClick={save}>应用图表数据</Button>}
  </section>;
}

function chartSeriesDraft(spec: PresentationV5ChartSpec) {
  return spec.series.map((series) => `${series.name}: ${series.values.join(", ")}`).join("\n");
}

function parseChartInspectorDraft(input: {
  chartType: PresentationV5ChartSpec["chartType"];
  title: string;
  categories: string;
  seriesText: string;
  previous: PresentationV5ChartSpec;
}): PresentationV5ChartSpec {
  const categories = input.categories.split(",").map((value) => value.trim()).filter(Boolean);
  if (!categories.length) throw new Error("请至少提供一个分类。");
  const series = input.seriesText.split("\n").map((line, index) => {
    const separator = line.indexOf(":");
    if (separator <= 0) throw new Error(`第 ${index + 1} 个系列必须使用“名称: 数值, 数值”格式。`);
    const name = line.slice(0, separator).trim();
    const values = line.slice(separator + 1).split(",").map((value) => Number(value.trim()));
    if (!name || !values.length || values.some((value) => !Number.isFinite(value))) throw new Error(`第 ${index + 1} 个系列包含无效数值。`);
    if (values.length !== categories.length) throw new Error(`“${name}”的数据数量必须等于分类数量。`);
    if (input.chartType === "pie" && values.some((value) => value < 0)) throw new Error("饼图数值不能为负数。");
    return { name, values, color: input.previous.series[index]?.color ?? null };
  }).filter((series) => series.name);
  if (!series.length) throw new Error("请至少提供一个数据系列。");
  if (new Set(series.map((entry) => entry.name)).size !== series.length) throw new Error("数据系列名称不能重复。");
  if (input.chartType === "pie" && series.length !== 1) throw new Error("饼图只支持一个数据系列。");
  return { chartType: input.chartType, title: input.title.trim() || null, categories, series };
}

/**
 * The inspector edits the pair as one value because endpoint mutations are
 * atomic in the engine. A renderer never mutates `start` or `end` in place.
 */
function ConnectorInspector({ node, nodes, disabled, canEdit, onAction }: {
  node: PresentationConnectorNode;
  nodes: readonly PresentationV5Node[];
  disabled: boolean;
  canEdit: boolean;
  onAction: (action: BuiltinPresentationNodeAction, value?: unknown) => void;
}) {
  const [start, setStart] = useState<ConnectorEndpoint>(node.kind.data.start);
  const [end, setEnd] = useState<ConnectorEndpoint>(node.kind.data.end);
  useEffect(() => { setStart(node.kind.data.start); setEnd(node.kind.data.end); }, [node.id, node.kind.data]);
  const targets = nodes.filter((candidate) => candidate.id !== node.id && candidate.visible);
  const updateTarget = (which: "start" | "end", targetId: string) => {
    const current = which === "start" ? start : end;
    const next: ConnectorEndpoint = targetId
      ? { type: "node", value: { nodeId: targetId, anchor: current.type === "node" ? current.value.anchor : "center" } }
      : current.type === "free" ? current : { type: "free", value: endpointPoint(current, nodes, {}) };
    if (which === "start") setStart(next); else setEnd(next);
  };
  const updateAnchor = (which: "start" | "end", anchor: "top" | "right" | "bottom" | "left" | "center") => {
    const current = which === "start" ? start : end;
    if (current.type !== "node") return;
    const next: ConnectorEndpoint = { type: "node", value: { ...current.value, anchor } };
    if (which === "start") setStart(next); else setEnd(next);
  };
  const endpointControl = (label: string, which: "start" | "end", endpoint: ConnectorEndpoint) => <div className="presentation-studio__connector-endpoint" key={which}>
    <label>{label}<Select aria-label={`${label}目标`} disabled={disabled || !canEdit} value={endpoint.type === "node" ? endpoint.value.nodeId : ""} onChange={(event) => updateTarget(which, event.target.value)}>
      <option value="">自由端点（拖动调整）</option>
      {targets.map((candidate) => <option value={candidate.id} key={candidate.id}>{candidate.name || candidate.kind.type}</option>)}
    </Select></label>
    {endpoint.type === "node" && <label>锚点<Select aria-label={`${label}锚点`} disabled={disabled || !canEdit} value={endpoint.value.anchor} onChange={(event) => updateAnchor(which, event.target.value as "top" | "right" | "bottom" | "left" | "center")}>
      <option value="center">中心</option><option value="top">顶部</option><option value="right">右侧</option><option value="bottom">底部</option><option value="left">左侧</option>
    </Select></label>}
    {endpoint.type === "free" && <p>自由坐标：{Math.round(endpoint.value.x)}，{Math.round(endpoint.value.y)}。可直接拖动端点。</p>}
  </div>;
  return <section className="presentation-studio__inspector-section">
    <h3>连接线</h3>
    {endpointControl("起点", "start", start)}
    {endpointControl("终点", "end", end)}
    {canEdit && <Button type="button" size="sm" disabled={disabled} onClick={() => onAction("connector.endpoints", { start, end })}>应用端点</Button>}
    <p>当前支持直线和五个基础锚点；自动避障路由、连接点自定义与折线路由将在独立路由投影中实现。</p>
  </section>;
}

function TableInspector({ node, selection, disabled, availableCapabilities, canEditContent, canEditStyle, onSelectionChange, onAction }: {
  node: PresentationTableNode;
  selection: TableSelection | null;
  disabled: boolean;
  availableCapabilities: ReadonlySet<string>;
  canEditContent: boolean;
  canEditStyle: boolean;
  onSelectionChange: (selection: TableSelection | null) => void;
  onAction: (action: BuiltinPresentationNodeAction, value?: unknown) => void;
}) {
  const anchors = node.kind.data.cells;
  const fallback = anchors[0] ? { nodeId: node.id, anchor: { row: anchors[0].row, column: anchors[0].column }, focus: { row: anchors[0].row, column: anchors[0].column } } : null;
  const resolvedSelection = selection ?? fallback;
  const selected = resolvedSelection ? tableAnchorAt(node, resolvedSelection.focus) : null;
  const selectedAnchors = resolvedSelection ? tableAnchorsInSelection(node, resolvedSelection) : [];
  const [text, setText] = useState(selected?.content.text ?? "");
  const [fill, setFill] = useState(colorInputValue(selected?.style.fill ?? { type: "none" }));
  const [fillEnabled, setFillEnabled] = useState(selected?.style.fill.type === "solid");
  const [horizontalAlign, setHorizontalAlign] = useState(selected?.style.horizontalAlign ?? "left");
  const [verticalAlign, setVerticalAlign] = useState(selected?.style.verticalAlign ?? "middle");
  useEffect(() => {
    const current = resolvedSelection ? tableAnchorAt(node, resolvedSelection.focus) : anchors[0];
    if (!current) return;
    setText(current.content.text);
    setFill(colorInputValue(current.style.fill));
    setFillEnabled(current.style.fill.type === "solid");
    setHorizontalAlign(current.style.horizontalAlign);
    setVerticalAlign(current.style.verticalAlign);
  }, [anchors, node, resolvedSelection]);
  if (!selected || !resolvedSelection) return null;
  const address = { row: selected.row, column: selected.column };
  const range = tableRange(resolvedSelection);
  const canMerge = availableCapabilities.has("presentation.mergeTableCells") && tableSelectionCanMerge(node, resolvedSelection);
  const canSplit = availableCapabilities.has("presentation.splitTableCell") && selectedAnchors.length === 1 && (selected.rowSpan > 1 || selected.columnSpan > 1);
  const label = selected.rowSpan > 1 || selected.columnSpan > 1
    ? `第 ${selected.row + 1} 行，第 ${selected.column + 1} 列（合并 ${selected.rowSpan}×${selected.columnSpan}）`
    : `第 ${selected.row + 1} 行，第 ${selected.column + 1} 列`;
  return <section className="presentation-studio__inspector-section">
    <h3>单元格</h3>
    <label>目标单元格<Select value={`${selected.row}:${selected.column}`} disabled={disabled} onChange={(event) => {
      const [row, column] = event.target.value.split(":").map(Number);
      onSelectionChange({ nodeId: node.id, anchor: { row, column }, focus: { row, column } });
    }}>
      {anchors.map((cell) => <option key={`${cell.row}:${cell.column}`} value={`${cell.row}:${cell.column}`}>第 {cell.row + 1} 行，第 {cell.column + 1} 列{cell.rowSpan > 1 || cell.columnSpan > 1 ? `（合并 ${cell.rowSpan}×${cell.columnSpan}）` : ""}</option>)}
    </Select></label>
    <p className="presentation-studio__inspector-note">{selectedAnchors.length > 1 ? `已选择 ${selectedAnchors.length} 个单元格锚点。样式会批量应用；内容只对焦点单元格生效。` : `${label}。合并单元格仅可通过左上角锚点编辑。`}</p>
    {canEditContent && <label>内容<Textarea value={text} disabled={disabled || selectedAnchors.length !== 1} onChange={(event) => setText(event.target.value)} /></label>}
    {canEditContent && <Button type="button" size="sm" disabled={disabled || selectedAnchors.length !== 1} onClick={() => onAction("table.cellContent", { ...address, content: { text, runs: [] } })}>应用内容</Button>}
    {canEditStyle && <div className="presentation-studio__inspector-control-group">
      <Checkbox checked={fillEnabled} disabled={disabled} onChange={(event) => setFillEnabled(event.target.checked)}>填充颜色</Checkbox>
      <ColorPickerField ariaLabel="单元格填充颜色" role="fill" value={fill} disabled={disabled || !fillEnabled} compact onValueChange={(value) => setFill(value ?? "#ffffff")} />
    </div>}
    {canEditStyle && <div className="presentation-studio__transform-grid">
      <label>水平对齐<Select value={horizontalAlign} disabled={disabled} onChange={(event) => setHorizontalAlign(event.target.value as typeof horizontalAlign)}><option value="left">左对齐</option><option value="center">居中</option><option value="right">右对齐</option></Select></label>
      <label>垂直对齐<Select value={verticalAlign} disabled={disabled} onChange={(event) => setVerticalAlign(event.target.value as typeof verticalAlign)}><option value="top">顶端</option><option value="middle">居中</option><option value="bottom">底端</option></Select></label>
    </div>}
    {canEditStyle && <Button type="button" size="sm" disabled={disabled} onClick={() => onAction("table.cellStyle", { cells: selectedAnchors.map((cell) => ({ row: cell.row, column: cell.column })), style: { fill: fillEnabled ? solidPaint(fill) : { type: "none" }, horizontalAlign, verticalAlign } })}>应用单元格样式</Button>}
    {(availableCapabilities.has("presentation.insertTableRows") || availableCapabilities.has("presentation.insertTableColumns") || availableCapabilities.has("presentation.mergeTableCells") || availableCapabilities.has("presentation.splitTableCell")) && <section className="presentation-studio__table-structure" aria-label="表格结构">
      <h3>表格结构</h3>
      <div className="presentation-studio__arrange-actions">
        {availableCapabilities.has("presentation.insertTableRows") && <Button type="button" size="sm" variant="secondary" disabled={disabled} onClick={() => onAction("table.insertRows", { index: range.end.row + 1, count: 1 })}>在下方插入行</Button>}
        {availableCapabilities.has("presentation.insertTableColumns") && <Button type="button" size="sm" variant="secondary" disabled={disabled} onClick={() => onAction("table.insertColumns", { index: range.end.column + 1, count: 1 })}>在右侧插入列</Button>}
        {availableCapabilities.has("presentation.deleteTableRow") && <Button type="button" size="sm" variant="secondary" disabled={disabled || node.kind.data.rows <= 1} onClick={() => onAction("table.deleteRow", { index: range.start.row })}>删除当前行</Button>}
        {availableCapabilities.has("presentation.deleteTableColumn") && <Button type="button" size="sm" variant="secondary" disabled={disabled || node.kind.data.columns <= 1} onClick={() => onAction("table.deleteColumn", { index: range.start.column })}>删除当前列</Button>}
        {availableCapabilities.has("presentation.mergeTableCells") && <Button type="button" size="sm" variant="secondary" disabled={disabled || !canMerge} onClick={() => onAction("table.mergeCells", range)}>合并单元格</Button>}
        {availableCapabilities.has("presentation.splitTableCell") && <Button type="button" size="sm" variant="secondary" disabled={disabled || !canSplit} onClick={() => onAction("table.splitCell", address)}>拆分单元格</Button>}
      </div>
    </section>}
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
    {canEditImage ? <label>题注<Input value={caption} disabled={disabled} onChange={(event) => setCaption(event.target.value)} /></label> : null}
    {canEditImage ? <Checkbox checked={flipH} disabled={disabled} onChange={(event) => setFlipH(event.target.checked)}>水平翻转</Checkbox> : null}
    {canEditImage ? <Checkbox checked={flipV} disabled={disabled} onChange={(event) => setFlipV(event.target.checked)}>垂直翻转</Checkbox> : null}
    {canEditImage ? <fieldset className="presentation-studio__image-crop" disabled={disabled}>
      <legend>裁剪（百分比）</legend>
      {(["top", "right", "bottom", "left"] as const).map((edge) => <label key={edge}>{edge}<Input type="number" min="0" max="0.99" step="0.01" value={crop[edge]} onChange={(event) => setCrop((current) => ({ ...current, [edge]: Number(event.target.value) }))} /></label>)}
    </fieldset> : null}
    {canEditImage ? <Button type="button" size="sm" disabled={disabled || !cropIsValid} onClick={apply}>应用图片设置</Button> : null}
    {canEditImage && node.kind.data.originalAssetId ? <Button type="button" variant="secondary" size="sm" disabled={disabled} onClick={restoreOriginal}>恢复原图</Button> : null}
    <p>图片压缩：当前服务未注册不可逆的图片转码命令，因此不会显示伪功能。</p>
  </section>;
}

function TransformInspector({ node, disabled, onApply }: { node: PresentationV5Node; disabled: boolean; onApply: (transform: PresentationV5Transform) => void }) {
  const [transform, setTransform] = useState(node.transform);
  const update = (key: keyof PresentationV5Transform, value: string) => setTransform((current) => ({ ...current, [key]: Number(value) }));
  return <section className="presentation-studio__inspector-section">
    <h3>位置与大小</h3>
    <div className="presentation-studio__transform-grid">
      {(["x", "y", "width", "height", "rotation"] as const).map((key) => <label key={key}>{key}<Input type="number" value={transform[key]} disabled={disabled} min={key === "width" || key === "height" ? MIN_PRESENTATION_NODE_SIZE : undefined} onChange={(event) => update(key, event.target.value)} /></label>)}
    </div>
    <Button type="button" size="sm" disabled={disabled || !isUsableTransform(transform)} onClick={() => onApply(transform)}>应用位置与大小</Button>
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
  tableSelection,
  onTableCellSelect,
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
  tableSelection?: TableSelection | null;
  /** Grid selection is renderer-local and never mutates the TableNode directly. */
  onTableCellSelect?: (address: TableCellAddress, extend: boolean) => void;
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
        {node.kind.type === "table" && <PresentationTable node={node as PresentationTableNode} selection={tableSelection ?? null} onCellSelect={onTableCellSelect} />}
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

/** DOM table is a read-only projection of the canonical grid. Editing always
 * travels back through the table node registry and typed transactions. */
function PresentationTable({ node, selection, onCellSelect }: { node: PresentationTableNode; selection: TableSelection | null; onCellSelect?: (address: TableCellAddress, extend: boolean) => void }) {
  const table = node.kind.data;
  const selectionRange = selection?.nodeId === node.id ? tableRange(selection) : null;
  return <div className="presentation-studio__table" role="grid" aria-label="演示文稿表格" aria-rowcount={table.rows} aria-colcount={table.columns} style={{ gridTemplateColumns: `repeat(${table.columns}, minmax(0, 1fr))`, gridTemplateRows: `repeat(${table.rows}, minmax(0, 1fr))` }}>
    {table.cells.map((cell) => <div
      key={`${cell.row}:${cell.column}`}
      className={`presentation-studio__table-cell${selectionRange && cell.row <= selectionRange.end.row && cell.row + cell.rowSpan - 1 >= selectionRange.start.row && cell.column <= selectionRange.end.column && cell.column + cell.columnSpan - 1 >= selectionRange.start.column ? " is-grid-selected" : ""}`}
      style={{
        gridColumn: `${cell.column + 1} / span ${cell.columnSpan}`,
        gridRow: `${cell.row + 1} / span ${cell.rowSpan}`,
        background: paintColor(cell.style.fill) ?? "transparent",
        textAlign: cell.style.horizontalAlign,
        alignContent: cell.style.verticalAlign,
      }}
      role="gridcell"
      tabIndex={0}
      aria-label={`第 ${cell.row + 1} 行，第 ${cell.column + 1} 列`}
      onPointerDown={(event) => {
        event.stopPropagation();
        event.currentTarget.setPointerCapture(event.pointerId);
        onCellSelect?.({ row: cell.row, column: cell.column }, event.shiftKey || event.metaKey || event.ctrlKey);
      }}
      onPointerEnter={(event) => {
        if (event.buttons === 1) onCellSelect?.({ row: cell.row, column: cell.column }, true);
      }}
      onKeyDown={(event) => {
        if (event.key === "Enter" || event.key === " ") {
          event.preventDefault();
          onCellSelect?.({ row: cell.row, column: cell.column }, event.shiftKey);
        }
      }}
    >{cell.content.text}</div>)}
  </div>;
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

/** Projection payloads are already boundary-validated; keep the UI default explicit for empty backgrounds. */
function asSlideBackground(value: unknown): PresentationV5SlideBackground {
  if (value && typeof value === "object" && (value as { type?: unknown }).type === "solid") {
    return value as PresentationV5SlideBackground;
  }
  return { type: "none" };
}

function pageSafeArea(value: unknown): PresentationV5Deck["pageSpec"]["safeArea"] {
  if (!value || typeof value !== "object") return null;
  const candidate = value as Partial<Record<"top" | "right" | "bottom" | "left", unknown>>;
  const edges = [candidate.top, candidate.right, candidate.bottom, candidate.left];
  if (!edges.every((edge) => typeof edge === "number" && Number.isFinite(edge) && edge >= 0)) return null;
  return { top: candidate.top as number, right: candidate.right as number, bottom: candidate.bottom as number, left: candidate.left as number };
}

function themeIdForName(name: string): string {
  const normalized = name.trim().toLowerCase().replace(/[^a-z0-9]+/g, "-").replace(/^-|-$/g, "");
  return normalized || "custom-theme";
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

function ConnectorOverlay({
  nodes,
  preview,
  connectorPreview,
  selectedNodeId,
  scale,
  width,
  height,
  canEdit,
  onSelect,
  onEndpointPointerDown,
}: {
  nodes: readonly PresentationV5Node[];
  preview: Readonly<Record<string, PresentationV5Transform>>;
  connectorPreview: ConnectorPreview;
  selectedNodeId: string | null;
  scale: number;
  width: number;
  height: number;
  canEdit: boolean;
  onSelect: (nodeId: string, extend?: boolean) => void;
  onEndpointPointerDown: (event: ReactPointerEvent<SVGCircleElement>, nodeId: string, endpoint: "start" | "end") => void;
}) {
  const connectors = nodes.filter((node): node is PresentationConnectorNode => node.visible && node.kind.type === "connector");
  if (connectors.length === 0) return null;
  return <svg className="presentation-studio__connector-overlay" width={width} height={height} viewBox={`0 0 ${width} ${height}`} aria-label="连接线编辑层">
    {connectors.map((node) => {
      const endpoints = connectorPreview[node.id] ?? node.kind.data;
      const start = endpointPoint(endpoints.start, nodes, preview);
      const end = endpointPoint(endpoints.end, nodes, preview);
      const selected = selectedNodeId === node.id;
      return <g key={node.id} className={selected ? "is-selected" : undefined}>
        <line
          className="presentation-studio__connector-hit-target"
          x1={start.x * scale}
          y1={start.y * scale}
          x2={end.x * scale}
          y2={end.y * scale}
          role="button"
          tabIndex={0}
          aria-label={`${node.name || "连接线"}${selected ? "，已选择" : ""}`}
          aria-pressed={selected}
          onPointerDown={(event) => { event.preventDefault(); event.stopPropagation(); onSelect(node.id, event.shiftKey || event.metaKey || event.ctrlKey); }}
          onKeyDown={(event) => {
            if (event.key === "Enter" || event.key === " ") {
              event.preventDefault();
              onSelect(node.id, event.shiftKey || event.metaKey || event.ctrlKey);
            }
          }}
        />
        {selected && canEdit && <>
          <circle className="presentation-studio__connector-handle" cx={start.x * scale} cy={start.y * scale} r="5" role="button" tabIndex={0} aria-label="拖动连接线起点" onPointerDown={(event) => onEndpointPointerDown(event, node.id, "start")} />
          <circle className="presentation-studio__connector-handle" cx={end.x * scale} cy={end.y * scale} r="5" role="button" tabIndex={0} aria-label="拖动连接线终点" onPointerDown={(event) => onEndpointPointerDown(event, node.id, "end")} />
        </>}
      </g>;
    })}
  </svg>;
}

function CanvasLayer({ nodes, preview, connectorPreview, scale, width, height }: { nodes: readonly PresentationV5Node[]; preview: Readonly<Record<string, PresentationV5Transform>>; connectorPreview: ConnectorPreview; scale: number; width: number; height: number }) {
  const canvas = useRef<HTMLCanvasElement>(null);
  const lastSnapshot = useRef<CanvasRenderSnapshot | null>(null);
  useEffect(() => {
    const element = canvas.current;
    if (!element) return;
    const ratio = window.devicePixelRatio || 1;
    // Feed the retained canvas a projection containing the transient endpoint
    // coordinates. This keeps the line repaint incremental while the Deck
    // itself remains untouched until pointerup.
    const renderNodes = nodesWithConnectorPreview(nodes, connectorPreview);
    // Endpoint geometry is not represented by a connector's legacy transform
    // bounds. Until the render-plan has segment-aware dirty rectangles, a
    // slide containing connectors must repaint fully to avoid stale pixels.
    const plan = deriveCanvasRenderPlan({
      previous: renderNodes.some((node) => node.kind.type === "connector") ? null : lastSnapshot.current,
      nodes: renderNodes,
      preview,
      width,
      height,
      scale,
    });
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
      for (const node of plan.nodes) drawNode(context, node, preview[node.id] ?? node.transform, scale, renderNodes, preview, connectorPreview);
    } else if (plan.dirtyRect) {
      context.clearRect(plan.dirtyRect.x, plan.dirtyRect.y, plan.dirtyRect.width, plan.dirtyRect.height);
      context.save();
      context.beginPath();
      context.rect(plan.dirtyRect.x, plan.dirtyRect.y, plan.dirtyRect.width, plan.dirtyRect.height);
      context.clip();
      for (const node of plan.nodes) drawNode(context, node, preview[node.id] ?? node.transform, scale, renderNodes, preview, connectorPreview);
      context.restore();
    }
    lastSnapshot.current = plan.snapshot;
  }, [connectorPreview, height, nodes, preview, scale, width]);
  return <canvas className="presentation-studio__canvas" ref={canvas} aria-hidden="true" />;
}

function drawNode(
  context: CanvasRenderingContext2D,
  node: PresentationV5Node,
  transform: PresentationV5Transform,
  scale: number,
  nodes: readonly PresentationV5Node[],
  preview: Readonly<Record<string, PresentationV5Transform>>,
  connectorPreview: ConnectorPreview,
) {
  // Images are rendered by the DOM image layer, which can load the immutable
  // Asset endpoint without turning Canvas into a second asset cache.
  if (!node.visible || node.kind.type === "text" || node.kind.type === "image") return;
  if (node.kind.type === "connector") {
    const endpoints = connectorPreview[node.id] ?? node.kind.data;
    const start = endpointPoint(endpoints.start, nodes, preview);
    const end = endpointPoint(endpoints.end, nodes, preview);
    context.save();
    context.globalAlpha = node.opacity;
    context.strokeStyle = "#405a7d";
    context.lineWidth = Math.max(1.5, 1.5 * scale);
    context.beginPath();
    context.moveTo(start.x * scale, start.y * scale);
    context.lineTo(end.x * scale, end.y * scale);
    context.stroke();
    context.restore();
    return;
  }
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
    } else if (node.kind.data.geometry === "line" || node.kind.data.geometry === "arrow") {
      const midY = height * scale / 2;
      const endX = width * scale;
      context.beginPath();
      context.moveTo(0, midY);
      context.lineTo(endX, midY);
      context.stroke();
      if (node.kind.data.geometry === "arrow") {
        const head = Math.min(18 * scale, Math.max(6 * scale, endX / 5));
        context.beginPath();
        context.moveTo(endX, midY);
        context.lineTo(endX - head, midY - head * 0.65);
        context.moveTo(endX, midY);
        context.lineTo(endX - head, midY + head * 0.65);
        context.stroke();
      }
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
  } else if (node.kind.type === "chart") {
    drawChart(context, node.kind.data.spec, width * scale, height * scale);
  } else {
    context.fillStyle = "#dce4ef";
    context.fillRect(0, 0, width * scale, height * scale);
    context.strokeStyle = "#9eacc0";
    context.strokeRect(0, 0, width * scale, height * scale);
  }
  context.restore();
}

/** Canvas is only a renderer here. All values come from the immutable chart
 * node projection and the inspector commits an entire validated ChartSpec. */
function drawChart(context: CanvasRenderingContext2D, spec: PresentationV5ChartSpec, width: number, height: number) {
  const palette = ["#165dff", "#00b42a", "#ff7d00", "#722ed1", "#f53f3f", "#14c9c9"];
  const colorFor = (index: number) => colorCss(spec.series[index]?.color) ?? palette[index % palette.length] ?? "#165dff";
  context.fillStyle = "#ffffff";
  context.fillRect(0, 0, width, height);
  const titleHeight = spec.title ? Math.min(28, height * .16) : 0;
  if (spec.title) {
    context.fillStyle = "#1d2129";
    context.font = `${Math.max(11, Math.min(16, width / 25))}px system-ui, sans-serif`;
    context.textAlign = "center";
    context.textBaseline = "middle";
    context.fillText(spec.title, width / 2, titleHeight / 2 + 5);
  }
  const pad = { left: Math.max(30, width * .11), right: Math.max(16, width * .05), top: titleHeight + Math.max(12, height * .06), bottom: Math.max(26, height * .16) };
  const plotWidth = Math.max(1, width - pad.left - pad.right);
  const plotHeight = Math.max(1, height - pad.top - pad.bottom);
  const allValues = spec.series.flatMap((series) => series.values);
  const max = Math.max(1, ...allValues);
  const min = Math.min(0, ...allValues);
  const range = Math.max(1, max - min);
  const yFor = (value: number) => pad.top + (max - value) / range * plotHeight;
  const xFor = (value: number) => pad.left + (value - min) / range * plotWidth;
  const baseY = yFor(0);
  context.strokeStyle = "#e5e6eb";
  context.lineWidth = 1;
  for (let step = 0; step <= 4; step += 1) {
    const y = pad.top + plotHeight * step / 4;
    context.beginPath(); context.moveTo(pad.left, y); context.lineTo(width - pad.right, y); context.stroke();
  }
  context.strokeStyle = "#86909c";
  context.beginPath(); context.moveTo(pad.left, pad.top); context.lineTo(pad.left, pad.top + plotHeight); context.lineTo(width - pad.right, pad.top + plotHeight); context.stroke();
  const count = spec.categories.length;
  if (spec.chartType === "pie") {
    const series = spec.series[0];
    if (!series) return;
    const total = series.values.reduce((sum, value) => sum + Math.max(0, value), 0) || 1;
    const radius = Math.max(8, Math.min(plotWidth, plotHeight) * .36);
    const centerX = pad.left + plotWidth / 2;
    const centerY = pad.top + plotHeight / 2;
    let start = -Math.PI / 2;
    series.values.forEach((value, index) => {
      const end = start + Math.max(0, value) / total * Math.PI * 2;
      context.fillStyle = palette[index % palette.length] ?? "#165dff";
      context.beginPath(); context.moveTo(centerX, centerY); context.arc(centerX, centerY, radius, start, end); context.closePath(); context.fill();
      start = end;
    });
  } else if (spec.chartType === "line") {
    spec.series.forEach((series, seriesIndex) => {
      context.strokeStyle = colorFor(seriesIndex);
      context.lineWidth = Math.max(1.5, Math.min(3, width / 240));
      series.values.forEach((value, index) => {
        const x = pad.left + (index + .5) * plotWidth / count;
        const y = yFor(value);
        if (index === 0) context.beginPath(), context.moveTo(x, y); else context.lineTo(x, y);
      });
      context.stroke();
      series.values.forEach((value, index) => {
        context.fillStyle = colorFor(seriesIndex);
        context.beginPath(); context.arc(pad.left + (index + .5) * plotWidth / count, yFor(value), 2.5, 0, Math.PI * 2); context.fill();
      });
    });
  } else {
    const groupCount = spec.series.length;
    const band = plotWidth / count;
    const gap = Math.max(2, band * .08);
    const barWidth = Math.max(2, (band - gap * 2) / groupCount);
    spec.series.forEach((series, seriesIndex) => {
      context.fillStyle = colorFor(seriesIndex);
      series.values.forEach((value, index) => {
        const length = Math.abs(yFor(value) - baseY);
        if (spec.chartType === "bar") {
          const slot = plotHeight / count;
          const x = xFor(0);
          const y = pad.top + index * slot + gap + seriesIndex * Math.max(2, (slot - gap * 2) / groupCount);
          context.fillRect(Math.min(x, xFor(value)), y, Math.abs(xFor(value) - x), Math.max(2, (slot - gap * 2) / groupCount));
        } else {
          const x = pad.left + index * band + gap + seriesIndex * barWidth;
          context.fillRect(x, Math.min(baseY, yFor(value)), barWidth, length);
        }
      });
    });
  }
  context.fillStyle = "#4e5969";
  context.font = `${Math.max(9, Math.min(12, width / 34))}px system-ui, sans-serif`;
  context.textAlign = "center";
  context.textBaseline = "top";
  spec.categories.forEach((category, index) => {
    if (spec.chartType !== "pie") context.fillText(category, pad.left + (index + .5) * plotWidth / count, height - pad.bottom + 7, Math.max(12, plotWidth / count - 4));
  });
}

/** Resolves a connector endpoint from the immutable scene projection plus an
 * optional pointer preview. No route geometry is persisted in the Deck. */
function endpointPoint(
  endpoint: ConnectorEndpoint,
  nodes: readonly PresentationV5Node[],
  preview: Readonly<Record<string, PresentationV5Transform>>,
): { x: number; y: number } {
  if (endpoint.type === "free") return endpoint.value;
  const target = nodes.find((node) => node.id === endpoint.value.nodeId);
  const transform = target ? (preview[target.id] ?? target.transform) : null;
  if (!transform) return { x: 0, y: 0 };
  const centerX = transform.x + transform.width / 2;
  const centerY = transform.y + transform.height / 2;
  switch (endpoint.value.anchor) {
    case "top": return { x: centerX, y: transform.y };
    case "right": return { x: transform.x + transform.width, y: centerY };
    case "bottom": return { x: centerX, y: transform.y + transform.height };
    case "left": return { x: transform.x, y: centerY };
    case "center": return { x: centerX, y: centerY };
  }
}

/** Snaps a released free endpoint to a visible target only when the pointer
 * actually lands inside it. Routing/nearest-anchor selection remains a future
 * projection feature instead of being guessed into persisted state. */
function snapConnectorEndpoint(
  endpoint: Extract<ConnectorEndpoint, { type: "free" }>,
  connectorId: string,
  nodes: readonly PresentationV5Node[],
  preview: Readonly<Record<string, PresentationV5Transform>>,
): ConnectorEndpoint {
  const target = [...nodes].reverse().find((candidate) => {
    if (candidate.id === connectorId || !candidate.visible) return false;
    const transform = preview[candidate.id] ?? candidate.transform;
    return endpoint.value.x >= transform.x && endpoint.value.x <= transform.x + transform.width
      && endpoint.value.y >= transform.y && endpoint.value.y <= transform.y + transform.height;
  });
  return target ? { type: "node", value: { nodeId: target.id, anchor: "center" } } : endpoint;
}

function sameConnectorEndpoints(
  left: { start: ConnectorEndpoint; end: ConnectorEndpoint },
  right: { start: ConnectorEndpoint; end: ConnectorEndpoint },
) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function withoutConnectorPreview(preview: ConnectorPreview, nodeId: string): ConnectorPreview {
  const { [nodeId]: _removed, ...remaining } = preview;
  return remaining;
}

function nodesWithConnectorPreview(nodes: readonly PresentationV5Node[], preview: ConnectorPreview): readonly PresentationV5Node[] {
  return nodes.map((node) => node.kind.type === "connector" && preview[node.id]
    ? { ...node, kind: { type: "connector", data: preview[node.id] } }
    : node);
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
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
