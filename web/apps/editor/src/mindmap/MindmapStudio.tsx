import { measureMindmapText } from "./measure-text.js";
import { MindmapRichTextEditor } from "./MindmapRichTextEditor.js";
import { MindmapAdvancedInspector, MindmapAdvancedLayer, type MindmapAdvancedSelection } from "./MindmapAdvancedLayer.js";
import { FontPicker } from "../typography/FontPicker.js";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type ButtonHTMLAttributes,
  type DragEvent,
  type PointerEvent as ReactPointerEvent,
} from "react";
import type { MindmapProjection, PresentationPresenceParticipant } from "@open-office/schema/api";
import { updateMindmapProjection, type MindmapProjectionInvalidation } from "@open-office/mindmap-engine";
import type {
  ArtifactCommandEnvelope,
  MindmapConnectorStyle,
  MindmapEdge,
  MindmapConnectorShape,
  MindmapLayoutKind,
  MindmapModel,
  MindmapNode,
  MindmapNodeShape,
  MindmapNodeStyle,
  MindmapNodeSupplement,
  InlineStylePatch,
  RichText,
  TextRange,
  SnapshotEnvelope,
} from "@open-office/schema/artifact";
import { Icon, Popover, type IconName } from "@open-office/ui";
import { MINDMAP_THEMES, mindmapTheme, mindmapBranchColors, nodeThemeColors } from "./appearance.js";
import { createMindmapSpatialIndex, intersects, queryMindmapViewport } from "./viewport-index.js";
import { MindmapProjectionScheduler, ProjectionCancelledError } from "./projection-scheduler.js";
import { parseMindmapPresence, parseMindmapRevisionNotice, type MindmapRevisionNotice } from "./collaboration-stream.js";
import { api, ApiRequestError } from "../api.js";
import {
  buildMindmapIndex,
  buildPasteCommands,
  createClipboardPayload,
  isDescendant,
  parseClipboardPayload,
  selectionRoots,
  type MindmapCommandInput,
} from "./model.js";

interface MindmapStudioProps {
  id: string;
  title: string;
  importWarnings?: string[];
  onBack: () => void;
}

type ViewTheme = "light" | "dark" | "highContrast";
type DropPosition = "before" | "child" | "after";
interface DropTarget { id: string; position: DropPosition }
interface Point { x: number; y: number }
interface Marquee { start: Point; current: Point }
interface EdgeEndpointPreview { endpoint: "source" | "target"; targetNodeId: string | null; point: Point }

const LAYOUT_LABELS: Array<[MindmapLayoutKind, string]> = [
  ["logicalRight", "向右逻辑图"],
  ["logicalLeft", "向左逻辑图"],
  ["mindMap", "经典思维导图"],
  ["organization", "组织结构图"],
  ["catalog", "目录图"],
  ["timelineHorizontal", "水平时间轴"],
  ["timelineVertical", "垂直时间轴"],
  ["fishbone", "鱼骨图"],
];

const SHAPE_LABELS: Array<[MindmapNodeShape, string]> = [
  ["roundedRectangle", "圆角矩形"],
  ["rectangle", "矩形"],
  ["ellipse", "椭圆"],
  ["diamond", "菱形"],
  ["pill", "胶囊"],
  ["underline", "下划线"],
];

let fallbackClipboard = "";

export function MindmapStudio({ id, title, importWarnings = [], onBack }: MindmapStudioProps) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const pendingFocus = useRef<string | null>(null);
  const projectionScheduler = useRef(new MindmapProjectionScheduler());
  const projectionRef = useRef<MindmapProjection | null>(null);
  const loadRequestId = useRef(0);
  const streamedRevision = useRef(0);
  const initialViewId = useRef<string | null>(null);
  const [snapshot, setSnapshot] = useState<SnapshotEnvelope | null>(null);
  const [projection, setProjection] = useState<MindmapProjection | null>(null);
  const [theme, setTheme] = useState<ViewTheme>("light");
  const [zoom, setZoom] = useState(1);
  const [pan, setPan] = useState<Point>({ x: 70, y: 70 });
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [primaryId, setPrimaryId] = useState<string | null>(null);
  const [selectedEdgeId, setSelectedEdgeId] = useState<string | null>(null);
  const [selectedAdvanced, setSelectedAdvanced] = useState<MindmapAdvancedSelection | null>(null);
  const [editingId, setEditingId] = useState<string | null>(null);
  const [canUndo, setCanUndo] = useState(false);
  const [canRedo, setCanRedo] = useState(false);
  const [busy, setBusy] = useState(true);
  const [syncText, setSyncText] = useState("正在载入…");
  const [error, setError] = useState<string | null>(null);
  const [importNotice, setImportNotice] = useState<string | null>(() => importWarnings.length > 0 ? `导入完成，但有内容被降级：${importWarnings.join("；")}` : null);
  const [outlineOpen, setOutlineOpen] = useState(() => window.innerWidth > 900);
  const [inspectorOpen, setInspectorOpen] = useState(() => window.innerWidth > 900);
  const [exportOpen, setExportOpen] = useState(false);
  const [themePickerOpen, setThemePickerOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [searchCursor, setSearchCursor] = useState(-1);
  const [dropTarget, setDropTarget] = useState<DropTarget | null>(null);
  const [contextMenu, setContextMenu] = useState<{ x: number; y: number; nodeId: string } | null>(null);
  const [marquee, setMarquee] = useState<Marquee | null>(null);
  const [edgeEndpointPreview, setEdgeEndpointPreview] = useState<EdgeEndpointPreview | null>(null);
  const edgeEndpointGesture = useRef<{ pointerId: number; endpoint: "source" | "target"; targetNodeId: string | null } | null>(null);
  const panGesture = useRef<{ pointerId: number; origin: Point; pan: Point } | null>(null);
  const presenceSessionId = useRef(`mindmap-${randomId()}`.replace(/[^A-Za-z0-9_-]/g, "_"));
  const presenceCursor = useRef<Point | undefined>(undefined);
  const lastPresenceAt = useRef(0);
  const [remotePresence, setRemotePresence] = useState<readonly PresentationPresenceParticipant[]>([]);
  const [viewportSize, setViewportSize] = useState({ width: 1200, height: 800 });
  const [capabilities, setCapabilities] = useState<ReadonlySet<string>>(new Set());

  const model = snapshot?.artifact.payload.kind === "mindmap" ? snapshot.artifact.payload.data : null;
  const index = useMemo(() => buildMindmapIndex(model), [model]);
  const branchColors = useMemo(() => mindmapBranchColors(model), [model]);
  const layoutById = useMemo(
    () => new Map((projection?.layout.nodes ?? []).map((node) => [node.id, node])),
    [projection],
  );
  const edgeById = useMemo(() => new Map((model?.edges ?? []).map((edge) => [edge.id, edge])), [model]);
  const selectedEdge = selectedEdgeId ? edgeById.get(selectedEdgeId) ?? null : null;
  const selectedAdvancedDeleteCapability = selectedAdvanced ? `mindmap.delete${selectedAdvanced.type === "summary" ? "Summary" : selectedAdvanced.type === "boundary" ? "Boundary" : "Formula"}` : null;
  const selected = primaryId ? index.nodeById.get(primaryId) ?? null : null;
  const matches = useMemo(() => {
    const query = search.trim().toLocaleLowerCase();
    if (!query) return [];
    return (model?.nodes ?? [])
      .filter((node) => node.content?.text.toLocaleLowerCase().includes(query))
      .map((node) => node.id);
  }, [model, search]);
  const spatialIndex = useMemo(
    () => projection && projection.layout.nodes.length > 600 ? createMindmapSpatialIndex(projection) : null,
    [projection],
  );
  const viewportSlice = useMemo(() => {
    if (!spatialIndex) return null;
    const margin = 280;
    return queryMindmapViewport(spatialIndex, {
      x: -pan.x / zoom - margin,
      y: -pan.y / zoom - margin,
      width: viewportSize.width / zoom + margin * 2,
      height: viewportSize.height / zoom + margin * 2,
    });
  }, [pan, spatialIndex, viewportSize, zoom]);
  const renderedLayoutNodes = useMemo(() => {
    const nodes = projection?.layout.nodes ?? [];
    if (!viewportSlice) return nodes;
    const forced = new Set([...selectedIds, ...(editingId ? [editingId] : []), ...(primaryId ? [primaryId] : [])]);
    return nodes.filter((node, indexValue) => viewportSlice.nodeIndexes.has(indexValue) || forced.has(node.id));
  }, [editingId, primaryId, projection, selectedIds, viewportSlice]);
  const renderedRoutes = useMemo(() => (projection?.edges.routes ?? []).filter((route, indexValue) => !viewportSlice || viewportSlice.routeIndexes.has(indexValue) || route.edgeId === selectedEdgeId), [projection, selectedEdgeId, viewportSlice]);
  const renderedAdvanced = useMemo(() => projection ? {
    summaries: projection.advanced.summaries.filter((_, indexValue) => !viewportSlice || viewportSlice.summaryIndexes.has(indexValue) || (selectedAdvanced?.type === "summary" && projection.advanced.summaries[indexValue]?.summaryId === selectedAdvanced.id)),
    boundaries: projection.advanced.boundaries.filter((_, indexValue) => !viewportSlice || viewportSlice.boundaryIndexes.has(indexValue) || (selectedAdvanced?.type === "boundary" && projection.advanced.boundaries[indexValue]?.boundaryId === selectedAdvanced.id)),
    formulas: projection.advanced.formulas.filter((_, indexValue) => !viewportSlice || viewportSlice.formulaIndexes.has(indexValue) || (selectedAdvanced?.type === "formula" && projection.advanced.formulas[indexValue]?.formulaId === selectedAdvanced.id)),
  } : { summaries: [], boundaries: [], formulas: [] }, [projection, selectedAdvanced, viewportSlice]);

  const load = useCallback(async (nextTheme: ViewTheme = theme, notice?: MindmapRevisionNotice) => {
    const requestId = ++loadRequestId.current;
    try {
      setError(null);
      const [nextSnapshot, history] = await Promise.all([
        api.getArtifact(id),
        api.getHistoryState(id),
      ]);
      if (nextSnapshot.artifact.kind !== "mindmap" || nextSnapshot.artifact.payload.kind !== "mindmap") {
        throw new Error("这个文件不是思维脑图");
      }
      const incremental = notice ? incrementalProjectionInvalidation(notice) : null;
      const nextProjection = incremental && projectionRef.current
        ? await updateMindmapProjection(projectionRef.current, nextSnapshot, incremental, nextTheme)
        : await projectionScheduler.current.project(nextSnapshot, nextTheme, await measureMindmapText(nextSnapshot.artifact.payload.data));
      if (requestId !== loadRequestId.current) return;
      setSnapshot(nextSnapshot);
      streamedRevision.current = nextSnapshot.artifact.revision;
      setProjection(nextProjection);
      projectionRef.current = nextProjection;
      setCanUndo(history.canUndo);
      setCanRedo(history.canRedo);
      setSyncText(`已同步 · 版本 ${nextSnapshot.artifact.revision}`);
      setSelectedIds((current) => current.filter((nodeId) => nextSnapshot.artifact.payload.kind === "mindmap" && nextSnapshot.artifact.payload.data.nodes.some((node) => node.id === nodeId)));
      setPrimaryId((current) => current && nextSnapshot.artifact.payload.kind === "mindmap" && nextSnapshot.artifact.payload.data.nodes.some((node) => node.id === current) ? current : null);
      setSelectedEdgeId((current) => current && nextSnapshot.artifact.payload.kind === "mindmap" && nextSnapshot.artifact.payload.data.edges.some((edge) => edge.id === current) ? current : null);
      setSelectedAdvanced((current) => {
        if (!current || nextSnapshot.artifact.payload.kind !== "mindmap") return null;
        const data = nextSnapshot.artifact.payload.data;
        const exists = current.type === "summary" ? data.summaries.some((item) => item.id === current.id)
          : current.type === "boundary" ? data.boundaries.some((item) => item.id === current.id)
            : data.formulas.some((item) => item.id === current.id);
        return exists ? current : null;
      });
    } catch (reason) {
      if (reason instanceof ProjectionCancelledError || requestId !== loadRequestId.current) return;
      setError(errorMessage(reason));
      setSyncText("同步失败");
    } finally {
      if (requestId === loadRequestId.current) setBusy(false);
    }
  }, [id, theme]);

  useEffect(() => { void load(theme); }, [id, theme]); // eslint-disable-line react-hooks/exhaustive-deps

  useEffect(() => () => projectionScheduler.current.dispose(), []);

  useEffect(() => {
    let disposed = false;
    void api.capabilities().then((catalog) => {
      if (disposed) return;
      const mindmap = catalog.artifacts.find((artifact) => artifact.kind === "mindmap");
      setCapabilities(new Set(mindmap?.commands.map((command) => command.typeId) ?? []));
    }).catch((reason) => {
      if (!disposed) setError(errorMessage(reason));
    });
    return () => { disposed = true; };
  }, []);

  const submit = useCallback(async (commands: MindmapCommandInput[], origin: ArtifactCommandEnvelope["origin"] = "local") => {
    if (!snapshot || commands.length === 0) return false;
    const transactionId = randomId();
    setBusy(true);
    setSyncText("正在保存…");
    setError(null);
    try {
      const result = await api.submitTransaction(id, {
        protocolVersion: 1,
        transactionId,
        intentId: randomId(),
        artifactId: id,
        actorId: "dev-user",
        baseRevision: snapshot.artifact.revision,
        origin,
        commands: commands.map((command) => ({ commandId: randomId(), ...command })),
      });
      setCanUndo(result.canUndo);
      setCanRedo(result.canRedo);
      await load(theme);
      return true;
    } catch (reason) {
      setError(errorMessage(reason));
      setSyncText("保存失败");
      if (reason instanceof ApiRequestError && reason.status === 409) await load(theme);
      return false;
    } finally {
      setBusy(false);
    }
  }, [id, load, snapshot, theme]);

  const history = useCallback(async (action: "undo" | "redo") => {
    if (!snapshot) return;
    setBusy(true);
    setSyncText(action === "undo" ? "正在撤销…" : "正在重做…");
    try {
      const result = await api.submitMindmapHistory(id, action, snapshot.artifact.revision);
      setCanUndo(result.canUndo);
      setCanRedo(result.canRedo);
      await load(theme);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  }, [id, load, snapshot, theme]);

  const focusNode = useCallback((nodeId: string) => {
    const node = layoutById.get(nodeId);
    const viewport = viewportRef.current;
    if (!node || !viewport) return;
    const rect = viewport.getBoundingClientRect();
    setPan({
      x: rect.width / 2 - (node.x + node.width / 2) * zoom,
      y: rect.height / 2 - (node.y + node.height / 2) * zoom,
    });
    setSelectedIds([nodeId]);
    setPrimaryId(nodeId);
    setSelectedEdgeId(null);
    setSelectedAdvanced(null);
  }, [layoutById, zoom]);

  const fitView = useCallback(() => {
    const viewport = viewportRef.current;
    if (!viewport || !projection) return;
    const rect = viewport.getBoundingClientRect();
    const nextZoom = clamp(Math.min((rect.width - 48) / Math.max(projection.layout.width, 1), (rect.height - 48) / Math.max(projection.layout.height, 1)), .25, 1.4);
    setZoom(nextZoom);
    setPan({
      x: Math.max(36, (rect.width - projection.layout.width * nextZoom) / 2),
      y: Math.max(36, (rect.height - projection.layout.height * nextZoom) / 2),
    });
  }, [projection]);

  useEffect(() => {
    const nodeId = pendingFocus.current;
    if (nodeId && layoutById.has(nodeId)) {
      pendingFocus.current = null;
      focusNode(nodeId);
    }
  }, [focusNode, layoutById]);

  const revealCommands = useCallback((nodeId: string | null): MindmapCommandInput[] => {
    const commands: MindmapCommandInput[] = [];
    let node = nodeId ? index.nodeById.get(nodeId) : undefined;
    while (node) {
      if (node.collapsed) commands.push({ typeId: "mindmap.setNodeCollapsed", payload: { type: "setNodeCollapsed", nodeId: node.id, collapsed: false } });
      node = node.parentId ? index.nodeById.get(node.parentId) : undefined;
    }
    return commands;
  }, [index]);

  useEffect(() => {
    if (!projection || !model || initialViewId.current === id) return;
    initialViewId.current = id;
    if (model.root) fitView();
  }, [fitView, id, model, projection]);

  const setMapTheme = useCallback(async (themeId: string) => {
    if (!model || busy) return;
    setThemePickerOpen(false);
    if (themeId === (model.settings.themeId ?? "ocean")) return;
    await submit([{ typeId: "mindmap.setSettings", payload: { type: "setSettings", settings: { ...model.settings, themeId } } }]);
  }, [busy, model, submit]);

  const selectNode = useCallback((nodeId: string, additive = false) => {
    setSelectedIds((current) => additive
      ? current.includes(nodeId) ? current.filter((idValue) => idValue !== nodeId) : [...current, nodeId]
      : [nodeId]);
    setPrimaryId((current) => additive && current === nodeId ? null : nodeId);
    setSelectedEdgeId(null);
    setContextMenu(null);
  }, []);

  const selectAdvanced = useCallback((selection: MindmapAdvancedSelection) => {
    setSelectedIds([]);
    setPrimaryId(null);
    setSelectedEdgeId(null);
    setSelectedAdvanced(selection);
    setContextMenu(null);
  }, []);

  const addNode = useCallback(async (mode: "root" | "child" | "sibling", anchorId?: string) => {
    if (!model || busy) return;
    const anchor = anchorId ? index.nodeById.get(anchorId) : selected;
    const nodeId = randomId();
    let parentId: string | null = null;
    let insertionIndex = 0;
    if (!model.root) mode = "root";
    if (mode === "root") {
      if (model.root) return;
    } else if (mode === "child") {
      parentId = anchor?.id ?? model.root;
      if (!parentId) return;
      insertionIndex = index.children.get(parentId)?.length ?? 0;
    } else {
      if (!anchor || !anchor.parentId) return void addNode("child");
      parentId = anchor.parentId;
      const siblings = index.children.get(parentId) ?? [];
      insertionIndex = Math.max(0, siblings.findIndex((node) => node.id === anchor.id) + 1);
    }
    const text = mode === "root" ? "中心主题" : "新主题";
    pendingFocus.current = nodeId;
    const ok = await submit([...revealCommands(parentId), {
      typeId: "mindmap.addNode",
      payload: { type: "addNode", nodeId, parentId, content: richText(text), attrs: {}, index: insertionIndex },
    }]);
    if (ok) {
      setSelectedIds([nodeId]);
      setPrimaryId(nodeId);
      setEditingId(nodeId);
    } else pendingFocus.current = null;
  }, [busy, index, model, revealCommands, selected, submit]);

  const replaceNodeText = useCallback(async (nodeId: string, content: RichText) => {
    setEditingId(null);
    if (JSON.stringify(content) === JSON.stringify(index.nodeById.get(nodeId)?.content)) return;
    await submit([{
      typeId: "mindmap.replaceNodeText",
      payload: { type: "replaceNodeText", nodeId, content },
    }]);
  }, [index, submit]);

  const patchNodeText = useCallback(async (nodeId: string, draft: RichText, range: TextRange, patch: InlineStylePatch) => {
    const current = index.nodeById.get(nodeId)?.content ?? null;
    const commands: MindmapCommandInput[] = [];
    if (JSON.stringify(draft) !== JSON.stringify(current)) {
      commands.push({ typeId: "mindmap.replaceNodeText", payload: { type: "replaceNodeText", nodeId, content: draft } });
    }
    commands.push({ typeId: "mindmap.patchNodeTextRange", payload: { type: "patchNodeTextRange", nodeId, range, patch } });
    await submit(commands);
  }, [index, submit]);

  const deleteSelection = useCallback(async () => {
    if (!model || busy) return;
    const roots = selectionRoots(selectedIds, index);
    if (roots.length === 0) return;
    await submit(roots.map((nodeId) => ({ typeId: "mindmap.deleteNode", payload: { type: "deleteNode", nodeId } })));
    setSelectedIds([]);
    setPrimaryId(null);
  }, [busy, index, model, selectedIds, submit]);

  const copySelection = useCallback(async (cut = false) => {
    if (!model) return;
    const payload = createClipboardPayload(model, selectedIds, index);
    if (!payload) return;
    const text = JSON.stringify(payload);
    fallbackClipboard = text;
    try { await navigator.clipboard?.writeText(text); } catch { /* in-app fallback remains available */ }
    setSyncText(`${payload.nodes.length} 个主题已${cut ? "剪切" : "复制"}`);
    if (cut) await deleteSelection();
  }, [deleteSelection, index, model, selectedIds]);

  const paste = useCallback(async () => {
    if (!model || busy) return;
    let text = fallbackClipboard;
    try { text = await navigator.clipboard?.readText() || text; } catch { /* use fallback */ }
    const payload = parseClipboardPayload(text);
    if (!payload) return setError("剪贴板里没有可粘贴的脑图片段");
    const parentId = selected?.id ?? model.root;
    const insertionIndex = parentId ? (index.children.get(parentId)?.length ?? 0) : 0;
    const built = buildPasteCommands(payload, parentId, insertionIndex, randomId, randomId);
    pendingFocus.current = built.rootIds[0] ?? null;
    if (await submit([...revealCommands(parentId), ...built.commands])) {
      setSelectedIds(built.rootIds);
      setPrimaryId(built.rootIds[0] ?? null);
    } else pendingFocus.current = null;
  }, [busy, index.children, model, revealCommands, selected, submit]);

  const addRelationship = useCallback(async () => {
    if (!model || busy || selectedIds.length < 2 || !capabilities.has("mindmap.addEdge")) return;
    const [sourceId, targetId] = selectedIds.slice(-2);
    const edgeId = randomId();
    if (await submit([{
      typeId: "mindmap.addEdge",
      payload: {
        type: "addEdge",
        edge: { id: edgeId, sourceId, targetId, label: null, style: model.settings.connector, attrs: {} },
      },
    }])) {
      setSelectedIds([]);
      setPrimaryId(null);
      setSelectedEdgeId(edgeId);
      setSelectedAdvanced(null);
    }
  }, [busy, capabilities, model, selectedIds, submit]);

  const addSummary = useCallback(async () => {
    if (!model || busy || selectedIds.length < 2 || !capabilities.has("mindmap.addSummary")) return;
    const [firstId, secondId] = selectedIds.slice(-2);
    const first = index.nodeById.get(firstId);
    const second = index.nodeById.get(secondId);
    if (!first?.parentId || first.parentId !== second?.parentId) return setError("概要只能覆盖同一父主题下的兄弟主题");
    const siblings = index.children.get(first.parentId) ?? [];
    const firstIndex = siblings.findIndex((node) => node.id === firstId);
    const secondIndex = siblings.findIndex((node) => node.id === secondId);
    const [startNodeId, endNodeId] = firstIndex < secondIndex ? [firstId, secondId] : [secondId, firstId];
    const entityId = randomId();
    if (await submit([{ typeId: "mindmap.addSummary", payload: { type: "addSummary", summary: { id: entityId, startNodeId, endNodeId, content: richText("概要") } } }])) selectAdvanced({ type: "summary", id: entityId });
  }, [busy, capabilities, index, model, selectAdvanced, selectedIds, submit]);

  const addBoundary = useCallback(async () => {
    if (!selected || busy || !capabilities.has("mindmap.addBoundary")) return;
    const entityId = randomId();
    if (await submit([{ typeId: "mindmap.addBoundary", payload: { type: "addBoundary", boundary: { id: entityId, rootNodeId: selected.id, label: null } } }])) selectAdvanced({ type: "boundary", id: entityId });
  }, [busy, capabilities, selectAdvanced, selected, submit]);

  const addFormula = useCallback(async () => {
    if (!selected || busy || !capabilities.has("mindmap.addFormula")) return;
    const entityId = randomId();
    if (await submit([{ typeId: "mindmap.addFormula", payload: { type: "addFormula", formula: { id: entityId, nodeId: selected.id, source: "x^2", display: "inline" } } }])) selectAdvanced({ type: "formula", id: entityId });
  }, [busy, capabilities, selectAdvanced, selected, submit]);

  const updateAdvanced = useCallback((patch: Record<string, unknown>) => {
    if (!selectedAdvanced || busy) return;
    const suffix = selectedAdvanced.type === "summary" ? "Summary" : selectedAdvanced.type === "boundary" ? "Boundary" : "Formula";
    if (!capabilities.has(`mindmap.update${suffix}`)) return;
    void submit([{ typeId: `mindmap.update${suffix}`, payload: { type: `update${suffix}`, [`${selectedAdvanced.type}Id`]: selectedAdvanced.id, ...patch } }]);
  }, [busy, capabilities, selectedAdvanced, submit]);

  const deleteAdvanced = useCallback(async () => {
    if (!selectedAdvanced || busy || !selectedAdvancedDeleteCapability || !capabilities.has(selectedAdvancedDeleteCapability)) return;
    const suffix = selectedAdvanced.type === "summary" ? "Summary" : selectedAdvanced.type === "boundary" ? "Boundary" : "Formula";
    if (await submit([{ typeId: `mindmap.delete${suffix}`, payload: { type: `delete${suffix}`, [`${selectedAdvanced.type}Id`]: selectedAdvanced.id } }])) setSelectedAdvanced(null);
  }, [busy, capabilities, selectedAdvanced, selectedAdvancedDeleteCapability, submit]);

  const updateEdge = useCallback((patch: { sourceId?: string; targetId?: string; label?: ReturnType<typeof richText> | null }) => {
    if (!selectedEdge || busy || !capabilities.has("mindmap.updateEdge")) return;
    void submit([{
      typeId: "mindmap.updateEdge",
      payload: { type: "updateEdge", edgeId: selectedEdge.id, ...patch },
    }]);
  }, [busy, capabilities, selectedEdge, submit]);

  const setEdgeStyle = useCallback((patch: Partial<MindmapConnectorStyle>) => {
    if (!selectedEdge || busy || !capabilities.has("mindmap.setEdgeStyle")) return;
    void submit([{
      typeId: "mindmap.setEdgeStyle",
      payload: { type: "setEdgeStyle", edgeId: selectedEdge.id, style: { ...selectedEdge.style, ...patch } },
    }]);
  }, [busy, capabilities, selectedEdge, submit]);

  const deleteSelectedEdge = useCallback(async () => {
    if (!selectedEdge || busy || !capabilities.has("mindmap.deleteEdge")) return;
    if (await submit([{ typeId: "mindmap.deleteEdge", payload: { type: "deleteEdge", edgeId: selectedEdge.id } }])) {
      setSelectedEdgeId(null);
    }
  }, [busy, capabilities, selectedEdge, submit]);

  const moveNode = useCallback(async (sourceId: string, target: DropTarget) => {
    const source = index.nodeById.get(sourceId);
    const destination = index.nodeById.get(target.id);
    if (!source || !destination || source.id === destination.id || source.parentId === null || isDescendant(index, source.id, destination.id)) return;
    let newParentId: string | null;
    let insertionIndex: number;
    if (target.position === "child") {
      newParentId = destination.id;
      insertionIndex = index.children.get(destination.id)?.length ?? 0;
    } else {
      newParentId = destination.parentId;
      if (!newParentId) return;
      const siblings = index.children.get(newParentId) ?? [];
      const targetIndex = siblings.findIndex((node) => node.id === destination.id);
      insertionIndex = targetIndex + (target.position === "after" ? 1 : 0);
    }
    await submit([{
      typeId: "mindmap.moveNode",
      payload: { type: "moveNode", nodeId: sourceId, newParentId, index: insertionIndex },
    }]);
  }, [index, submit]);

  const setNodeStyle = useCallback((patch: Partial<MindmapNodeStyle>) => {
    if (!selected || busy) return;
    void submit([{
      typeId: "mindmap.setNodeStyle",
      payload: { type: "setNodeStyle", nodeId: selected.id, style: { ...selected.style, ...patch } },
    }]);
  }, [busy, selected, submit]);

  const setNodeSupplement = useCallback((patch: Partial<MindmapNodeSupplement>) => {
    if (!selected || busy) return;
    void submit([{
      typeId: "mindmap.setNodeSupplement",
      payload: { type: "setNodeSupplement", nodeId: selected.id, supplement: { ...selected.supplement, ...patch } },
    }]);
  }, [busy, selected, submit]);

  const uploadNodeImage = useCallback(async (file: File) => {
    if (!selected) return;
    setBusy(true);
    try {
      const asset = await api.uploadAsset(id, file, file.name);
      await submit([{
        typeId: "mindmap.setNodeSupplement",
        payload: {
          type: "setNodeSupplement",
          nodeId: selected.id,
          supplement: { ...selected.supplement, image: { assetId: asset.assetId, alt: file.name, width: null, height: null } },
        },
      }]);
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      setBusy(false);
    }
  }, [id, selected, submit]);

  const toggleWholeTextStyle = useCallback((field: "bold" | "italic" | "underline") => {
    if (!selected?.content?.text || busy) return;
    const current = selected.content.runs[0]?.style[field] === true;
    void submit([{
      typeId: "mindmap.patchNodeTextRange",
      payload: { type: "patchNodeTextRange", nodeId: selected.id, range: { start: 0, end: [...selected.content.text].length }, patch: { [field]: !current } },
    }]);
  }, [busy, selected, submit]);

  const setWholeTextFont = useCallback((fontFamily: string) => {
    if (!selected?.content?.text || busy) return;
    void submit([{ typeId: "mindmap.patchNodeTextRange", payload: { type: "patchNodeTextRange", nodeId: selected.id, range: { start: 0, end: [...selected.content.text].length }, patch: { fontFamily } } }]);
  }, [selected, busy, submit]);

  const setLayout = useCallback((layout: MindmapLayoutKind) => {
    if (!model || busy) return;
    void submit([{
      typeId: "mindmap.setSettings",
      payload: { type: "setSettings", settings: { ...model.settings, layout } },
    }]);
  }, [busy, model, submit]);

  const setConnectorShape = useCallback((shape: MindmapConnectorShape) => {
    if (!model || busy) return;
    void submit([{
      typeId: "mindmap.setSettings",
      payload: { type: "setSettings", settings: { ...model.settings, connector: { ...model.settings.connector, shape } } },
    }]);
  }, [busy, model, submit]);

  const navigateSelection = useCallback((direction: "left" | "right" | "up" | "down") => {
    if (!selected) return;
    let next: MindmapNode | undefined;
    if (direction === "left" && selected.parentId) next = index.nodeById.get(selected.parentId);
    if (direction === "right") next = index.children.get(selected.id)?.[0];
    if (direction === "up" || direction === "down") {
      const siblings = selected.parentId ? index.children.get(selected.parentId) ?? [] : [selected];
      const at = siblings.findIndex((node) => node.id === selected.id);
      next = siblings[at + (direction === "up" ? -1 : 1)];
    }
    if (next) focusNode(next.id);
  }, [focusNode, index, selected]);

  const outdent = useCallback(async () => {
    if (!selected?.parentId) return;
    const parent = index.nodeById.get(selected.parentId);
    if (!parent?.parentId) return;
    const grandSiblings = index.children.get(parent.parentId) ?? [];
    const at = grandSiblings.findIndex((node) => node.id === parent.id);
    await submit([{
      typeId: "mindmap.moveNode",
      payload: { type: "moveNode", nodeId: selected.id, newParentId: parent.parentId, index: at + 1 },
    }]);
  }, [index, selected, submit]);

  const nextSearchMatch = useCallback((delta: 1 | -1) => {
    if (busy || matches.length === 0) return;
    const next = searchCursor < 0 ? (delta === 1 ? 0 : matches.length - 1) : (searchCursor + delta + matches.length) % matches.length;
    setSearchCursor(next);
    const nodeId = matches[next];
    const commands = revealCommands(index.nodeById.get(nodeId)?.parentId ?? null);
    if (commands.length) {
      if (busy) return;
      pendingFocus.current = nodeId;
      void submit(commands).then((ok) => { if (!ok) pendingFocus.current = null; });
    } else focusNode(nodeId);
  }, [busy, focusNode, index, matches, revealCommands, searchCursor, submit]);

  useEffect(() => { setSearchCursor(-1); }, [search, matches.join("\u0000")]);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;
    const update = () => setViewportSize({ width: viewport.clientWidth, height: viewport.clientHeight });
    update();
    const observer = new ResizeObserver(update);
    observer.observe(viewport);
    return () => observer.disconnect();
  }, [outlineOpen, !!projection]);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;
    const onWheel = (event: WheelEvent) => {
      if (!event.ctrlKey && !event.metaKey) return;
      event.preventDefault();
      const rect = viewport.getBoundingClientRect();
      const x = event.clientX - rect.left;
      const y = event.clientY - rect.top;
      const nextZoom = clamp(zoom * (event.deltaY > 0 ? .9 : 1.1), .25, 2.5);
      setPan({ x: x - (x - pan.x) * nextZoom / zoom, y: y - (y - pan.y) * nextZoom / zoom });
      setZoom(nextZoom);
    };
    viewport.addEventListener("wheel", onWheel, { passive: false });
    return () => viewport.removeEventListener("wheel", onWheel);
  }, [pan, zoom, !!projection]);

  useEffect(() => {
    void api.updatePresence(id, presenceSessionId.current, {
      selectedNodeIds: selectedIds.slice(0, 100),
      ...(presenceCursor.current ? { cursor: presenceCursor.current } : {}),
    }).catch(() => undefined);
  }, [id, selectedIds]);

  useEffect(() => {
    if (!snapshot) return;
    streamedRevision.current = Math.max(streamedRevision.current, snapshot.artifact.revision);
    const source = new EventSource(`/api/artifacts/${encodeURIComponent(id)}/event-stream?sinceRevision=${snapshot.artifact.revision}`);
    const onRevision = (event: MessageEvent<string>) => {
      const notice = parseMindmapRevisionNotice(event.data, id, streamedRevision.current);
      if (!notice) return;
      streamedRevision.current = notice.revision;
      void load(theme, notice);
    };
    const onPresence = (event: MessageEvent<string>) => {
      const participants = parseMindmapPresence(event.data, id, presenceSessionId.current);
      if (participants) setRemotePresence(participants);
    };
    source.addEventListener("revision", onRevision as EventListener);
    source.addEventListener("presence", onPresence as EventListener);
    return () => source.close();
  }, [id, load, snapshot?.artifact.revision, theme]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing || isTextTarget(event.target) || editingId) return;
      if (event.key === "Escape") { setContextMenu(null); setSelectedIds([]); setPrimaryId(null); setSelectedEdgeId(null); setSelectedAdvanced(null); return; }
      const command = event.metaKey || event.ctrlKey;
      if (!command && event.target instanceof HTMLElement && event.target.closest("button, a, [role=menu], [role=dialog]")) return;
      if (command && event.key.toLowerCase() === "a") { event.preventDefault(); setSelectedEdgeId(null); setSelectedAdvanced(null); setSelectedIds(model?.nodes.map((node) => node.id) ?? []); setPrimaryId(model?.root ?? null); return; }
      if (command && event.key.toLowerCase() === "c") { event.preventDefault(); void copySelection(false); return; }
      if (busy) return;
      if (command && event.key.toLowerCase() === "z") { event.preventDefault(); void history(event.shiftKey ? "redo" : "undo"); return; }
      if (command && event.key.toLowerCase() === "y") { event.preventDefault(); void history("redo"); return; }
      if (command && event.key.toLowerCase() === "x") { event.preventDefault(); void copySelection(true); return; }
      if (command && event.key.toLowerCase() === "v") { event.preventDefault(); void paste(); return; }
      if (event.key === "Tab") { event.preventDefault(); void (event.shiftKey ? outdent() : addNode("child")); return; }
      if (event.key === "Enter") { event.preventDefault(); void addNode("sibling"); return; }
      if (event.key === "Delete" || event.key === "Backspace") { event.preventDefault(); void (selectedAdvanced ? deleteAdvanced() : selectedEdge ? deleteSelectedEdge() : deleteSelection()); return; }
      if (event.key === "F2" && selected && capabilities.has("mindmap.replaceNodeText")) { event.preventDefault(); setEditingId(selected.id); return; }
      if (event.key.startsWith("Arrow")) { event.preventDefault(); navigateSelection(event.key.slice(5).toLowerCase() as "left" | "right" | "up" | "down"); }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [addNode, busy, capabilities, copySelection, deleteAdvanced, deleteSelectedEdge, deleteSelection, editingId, history, model, navigateSelection, outdent, paste, selected, selectedAdvanced, selectedEdge]);

  const viewportPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || (event.target instanceof Element && event.target.closest(".mindmap-node, .mindmap-link-hit, .mindmap-advanced-layer, .mindmap-minimap, .mindmap-empty"))) return;
    setContextMenu(null);
    const rect = event.currentTarget.getBoundingClientRect();
    const point = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    event.currentTarget.focus();
    event.currentTarget.setPointerCapture(event.pointerId);
    if (event.shiftKey) setMarquee({ start: point, current: point });
    else {
      panGesture.current = { pointerId: event.pointerId, origin: point, pan };
      setSelectedIds([]);
      setPrimaryId(null);
      setSelectedEdgeId(null);
      setSelectedAdvanced(null);
    }
  };

  const viewportPointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const rect = event.currentTarget.getBoundingClientRect();
    const point = { x: event.clientX - rect.left, y: event.clientY - rect.top };
    const endpointGesture = edgeEndpointGesture.current;
    if (endpointGesture && projection && selectedEdge) {
      const world = { x: (point.x - pan.x) / zoom, y: (point.y - pan.y) / zoom };
      const otherNodeId = endpointGesture.endpoint === "source" ? selectedEdge.targetId : selectedEdge.sourceId;
      const candidates = spatialIndex
        ? queryMindmapViewport(spatialIndex, { x: world.x - 12, y: world.y - 12, width: 24, height: 24 }).nodeIndexes
        : null;
      const targetNode = projection.layout.nodes.find((node, indexValue) => (!candidates || candidates.has(indexValue)) && node.id !== otherNodeId && world.x >= node.x - 12 && world.x <= node.x + node.width + 12 && world.y >= node.y - 12 && world.y <= node.y + node.height + 12);
      endpointGesture.targetNodeId = targetNode?.id ?? null;
      setEdgeEndpointPreview({ endpoint: endpointGesture.endpoint, targetNodeId: targetNode?.id ?? null, point: world });
      return;
    }
    if (marquee) setMarquee({ ...marquee, current: point });
    const gesture = panGesture.current;
    if (gesture?.pointerId === event.pointerId) setPan({ x: gesture.pan.x + point.x - gesture.origin.x, y: gesture.pan.y + point.y - gesture.origin.y });
  };

  const viewportPointerUp = (event: ReactPointerEvent<HTMLDivElement>) => {
    const endpointGesture = edgeEndpointGesture.current;
    if (endpointGesture?.pointerId === event.pointerId) {
      edgeEndpointGesture.current = null;
      setEdgeEndpointPreview(null);
      if (endpointGesture.targetNodeId) {
        if (endpointGesture.endpoint === "source") updateEdge({ sourceId: endpointGesture.targetNodeId });
        else updateEdge({ targetId: endpointGesture.targetNodeId });
      }
      return;
    }
    if (marquee && projection) {
      const box = normalizedRect(marquee.start, marquee.current);
      const worldBox = { x: (box.x - pan.x) / zoom, y: (box.y - pan.y) / zoom, width: box.width / zoom, height: box.height / zoom };
      const candidates = spatialIndex ? queryMindmapViewport(spatialIndex, worldBox).nodeIndexes : null;
      const ids = projection.layout.nodes
        .filter((node, indexValue) => (!candidates || candidates.has(indexValue)) && intersects(worldBox, node))
        .map((node) => node.id);
      setSelectedIds(ids);
      setPrimaryId(ids.at(-1) ?? null);
    }
    setMarquee(null);
    panGesture.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  };

  if (busy && !snapshot) return <div className="mindmap-status">正在打开思维脑图…</div>;
  if (!model || !projection) return <div className="mindmap-status"><p>{error ?? "无法打开思维脑图"}</p><button onClick={() => { setBusy(true); void load(theme); }}>重试</button><button onClick={onBack}>返回</button></div>;

  return (
    <div className={`mindmap-studio mindmap-studio--${theme}`} tabIndex={-1}>
      <header className="mindmap-topbar">
        <button className="mindmap-back" onClick={onBack} aria-label="返回文件列表" title="返回文件列表"><Icon name="arrow-left" size={18} /></button>
        <span className="mindmap-file-icon"><Icon name="mindmap" size={19} /></span>
        <strong title={title}>{title}</strong>
        <span className="mindmap-sync" title={syncText}><Icon name={error ? "history" : "check"} size={14} />{busy ? "处理中…" : error ? "同步失败" : "已保存"}</span>
        <span className="mindmap-topbar__grow" />
        <label className="mindmap-theme-label"><span>显示</span>
          <select aria-label="显示主题" value={theme} onChange={(event) => setTheme(event.target.value as ViewTheme)}>
            <option value="light">浅色</option><option value="dark">深色</option><option value="highContrast">高对比度</option>
          </select>
        </label>
        <div className="mindmap-presence-list" aria-label="在线协作者">
          {remotePresence.slice(0, 5).map((participant, indexValue) => <span key={participant.sessionId} style={{ "--presence-color": presenceColor(indexValue) } as CSSProperties} title={`${participant.displayName} 在线`}>{participant.displayName.slice(0, 1).toUpperCase()}</span>)}
          {remotePresence.length > 5 && <small>+{remotePresence.length - 5}</small>}
        </div>
        <Popover open={exportOpen} onOpenChange={setExportOpen} placement="bottom-end" role="menu" popupClassName="mindmap-export-menu" getPopupContainer={() => viewportRef.current?.closest(".mindmap-studio") ?? null} content={<>
          <span className="mindmap-menu-title">导出脑图</span>
          <a role="menuitem" href={`/api/artifacts/${id}/export/md`} download onClick={() => setExportOpen(false)}><Icon name="text" />Markdown<span>层级文本</span></a>
          <a role="menuitem" href={`/api/artifacts/${id}/export/json`} download onClick={() => setExportOpen(false)}><Icon name="code" />JSON<span>完整脑图</span></a>
          <a role="menuitem" href={`/api/artifacts/${id}/export/svg`} download onClick={() => setExportOpen(false)}><Icon name="image" />SVG<span>可缩放图片</span></a>
          <a role="menuitem" href={`/api/artifacts/${id}/export/pdf?paper=a4&orientation=landscape&mode=fit`} download onClick={() => setExportOpen(false)}><Icon name="text-extract" />PDF<span>适合单页</span></a>
          <a role="menuitem" href={`/api/artifacts/${id}/export/pdf?paper=a4&orientation=landscape&mode=tile`} download onClick={() => setExportOpen(false)}><Icon name="copy" />分页 PDF<span>保持阅读比例</span></a>
        </>}>
          <button className="mindmap-export-trigger"><Icon name="download" size={16} /><span>导出</span><Icon name="arrow-down" size={12} /></button>
        </Popover>
      </header>
      <div className="mindmap-toolbar" role="toolbar" aria-label="思维脑图工具栏">
        <div className="mindmap-toolbar__group" role="group" aria-label="视图与历史">
          <MindmapTool icon="bullet-list" label="大纲" aria-pressed={outlineOpen} onClick={() => { setOutlineOpen(!outlineOpen); if (window.innerWidth <= 900) setInspectorOpen(false); }} />
          <span className="mindmap-toolbar__divider" />
          <MindmapTool icon="undo" label="撤销" compact disabled={!canUndo || busy} onClick={() => void history("undo")} />
          <MindmapTool icon="redo" label="重做" compact disabled={!canRedo || busy} onClick={() => void history("redo")} />
        </div>
        <div className="mindmap-toolbar__group" role="group" aria-label="主题编辑">
          <MindmapTool icon="plus" label={model.root ? "子主题" : "中心主题"} title={model.root ? "添加子主题 · Tab" : "添加中心主题"} disabled={busy} onClick={() => void addNode(model.root ? "child" : "root")} />
          <MindmapTool icon="mindmap" label="同级主题" title="添加同级主题 · Enter" disabled={busy || !selected?.parentId} onClick={() => void addNode("sibling")} />
          <MindmapTool icon="link" label="添加关联" disabled={busy || selectedIds.length < 2 || !capabilities.has("mindmap.addEdge")} onClick={() => void addRelationship()} />
          <MindmapTool icon="bullet-list" label="添加概要" disabled={busy || selectedIds.length < 2 || !capabilities.has("mindmap.addSummary")} onClick={() => void addSummary()} />
          <MindmapTool icon="shape" label="添加外框" disabled={busy || !selected || !capabilities.has("mindmap.addBoundary")} onClick={() => void addBoundary()} />
          <MindmapTool icon="code" label="添加公式" disabled={busy || !selected || !capabilities.has("mindmap.addFormula")} onClick={() => void addFormula()} />
          <MindmapTool icon="delete" label="删除" compact disabled={busy || (selectedIds.length === 0 && !selectedEdge && !selectedAdvanced) || Boolean(selectedEdge && !capabilities.has("mindmap.deleteEdge")) || Boolean(selectedAdvancedDeleteCapability && !capabilities.has(selectedAdvancedDeleteCapability))} onClick={() => void (selectedAdvanced ? deleteAdvanced() : selectedEdge ? deleteSelectedEdge() : deleteSelection())} />
        </div>
        <div className="mindmap-toolbar__group" role="group" aria-label="格式面板">
          <Popover open={themePickerOpen} onOpenChange={setThemePickerOpen} placement="bottom-end" popupClassName="mindmap-theme-picker" getPopupContainer={() => viewportRef.current?.closest(".mindmap-studio") ?? null} content={<>
            <h2>主题风格</h2><p>应用于整张脑图，保留单独设置的颜色。</p>
            <div className="mindmap-theme-grid">{MINDMAP_THEMES.map((preset) => <button key={preset.id} disabled={busy} aria-label={preset.name} aria-pressed={mindmapTheme(model.settings.themeId).id === preset.id} onClick={() => void setMapTheme(preset.id)}>
              <svg viewBox="0 0 160 80" aria-hidden="true"><path d="M53 40H78V20H94M78 40V60H94" fill="none" stroke={preset.root} strokeWidth="1.5" /><rect x="12" y="30" width="42" height="20" rx="5" fill={preset.root} /><path d="M22 40H44" stroke="white" strokeWidth="2" />{[20,60].map((y, i) => <g key={y}><rect x="94" y={y-9} width="48" height="18" rx="4" fill={preset.branches[i]} fillOpacity=".13" /><path d={`M104 ${y}H132`} stroke={preset.branches[i]} strokeWidth="2" /></g>)}</svg>
              <span>{preset.name}</span>{mindmapTheme(model.settings.themeId).id === preset.id && <Icon name="check" size={13} />}
            </button>)}</div>
          </>}><button type="button" className="mindmap-tool" aria-label="主题风格" title="主题风格"><Icon name="brush" size={17} /><span>主题</span><Icon name="arrow-down" size={11} /></button></Popover>
          <MindmapTool icon="settings" label="格式" aria-pressed={inspectorOpen} onClick={() => { setInspectorOpen(!inspectorOpen); if (window.innerWidth <= 900) setOutlineOpen(false); }} />
        </div>
        <div className="mindmap-search">
          <Icon name="search" size={15} />
          <input value={search} onChange={(event) => setSearch(event.target.value)} placeholder="搜索主题" aria-label="搜索主题" onKeyDown={(event) => { if (event.nativeEvent.isComposing) return; if (event.key === "Enter") { event.preventDefault(); nextSearchMatch(event.shiftKey ? -1 : 1); } if (event.key === "Escape") setSearch(""); }} />
          {search && <>
            <output>{`${Math.max(0, searchCursor + 1)}/${matches.length}`}</output>
            <button disabled={!matches.length || busy} onClick={() => nextSearchMatch(-1)} aria-label="上一个结果" title="上一个结果"><Icon name="arrow-up" size={13} /></button>
            <button disabled={!matches.length || busy} onClick={() => nextSearchMatch(1)} aria-label="下一个结果" title="下一个结果"><Icon name="arrow-down" size={13} /></button>
            <button onClick={() => setSearch("")} aria-label="清除搜索" title="清除搜索"><Icon name="close" size={13} /></button>
          </>}
        </div>
      </div>
      <main className="mindmap-workspace">
        {outlineOpen && <OutlinePanel model={model} index={index} selectedIds={selectedIds} onSelect={focusNode} />}
        <div
          ref={viewportRef}
          className="mindmap-viewport"
          tabIndex={0}
          aria-label="脑图画布"
          onDoubleClick={(event) => { if (!model.root && event.target === event.currentTarget) void addNode("root"); }}
          onPointerDown={viewportPointerDown}
          onPointerMove={viewportPointerMove}
          onPointerUp={viewportPointerUp}
          onPointerCancel={viewportPointerUp}
          onContextMenu={(event) => { if (event.target === event.currentTarget) event.preventDefault(); }}
          onMouseMove={(event) => {
            const now = performance.now();
            if (now - lastPresenceAt.current < 120) return;
            const rect = event.currentTarget.getBoundingClientRect();
            presenceCursor.current = { x: (event.clientX - rect.left - pan.x) / zoom, y: (event.clientY - rect.top - pan.y) / zoom };
            lastPresenceAt.current = now;
            void api.updatePresence(id, presenceSessionId.current, { selectedNodeIds: selectedIds.slice(0, 100), cursor: presenceCursor.current }).catch(() => undefined);
          }}
        >
          {!model.root && <div className="mindmap-empty">
            <h1>从一个中心主题开始</h1>
            <p>创建中心主题，再用 Tab 添加分支、Enter 添加同级主题。</p>
            <button disabled={busy} onClick={() => void addNode("root")}>新建中心节点</button>
          </div>}
          <div
            className="mindmap-canvas"
            style={{ width: projection.layout.width, height: projection.layout.height, transform: `translate(${pan.x}px, ${pan.y}px) scale(${zoom})` }}
          >
            <svg className="mindmap-links" width={projection.layout.width} height={projection.layout.height} role="group" aria-label="脑图连线">
              <MindmapAdvancedLayer model={model} projection={renderedAdvanced} selected={selectedAdvanced} onSelect={selectAdvanced} />
              {renderedRoutes.map((route) => {
                const edge = route.edgeId ? edgeById.get(route.edgeId) : null;
                const connector = edge?.style ?? model.settings.connector;
                const path = routePath(route.points, connector.shape);
                if (!edge) return <path key={`${route.parentId}:${route.childId}`} className="mindmap-link" d={path} style={{ stroke: theme === "highContrast" ? "#333" : branchColors.get(route.childId), strokeWidth: connector.width, strokeDasharray: connector.dashed ? "6 4" : undefined }} />;
                const source = index.nodeById.get(edge.sourceId)?.content?.text || "未命名主题";
                const target = index.nodeById.get(edge.targetId)?.content?.text || "未命名主题";
                const middle = route.points[Math.floor(route.points.length / 2)] ?? route.points[0];
                const first = route.points[0];
                const last = route.points.at(-1);
                const selectedRoute = selectedEdgeId === edge.id;
                const previewAnchor = edgeEndpointPreview?.endpoint === "source" ? last : first;
                return <g key={edge.id} className={selectedRoute ? "is-selected" : ""}>
                  <path className="mindmap-link mindmap-link--explicit" d={path} style={{ stroke: connector.color ?? "#9a57d5", strokeWidth: connector.width, strokeDasharray: connector.dashed ? "6 4" : undefined }} />
                  <path className="mindmap-link-hit" d={path} role="button" tabIndex={0} aria-label={`关联线：${source} 到 ${target}`} aria-pressed={selectedEdgeId === edge.id} onClick={(event) => { event.stopPropagation(); setSelectedIds([]); setPrimaryId(null); setSelectedAdvanced(null); setSelectedEdgeId(edge.id); }} onKeyDown={(event) => { if (event.key === "Enter" || event.key === " ") { event.preventDefault(); setSelectedIds([]); setPrimaryId(null); setSelectedAdvanced(null); setSelectedEdgeId(edge.id); } }} />
                  {edge.label?.text && middle && <text className="mindmap-link-label" x={middle.x} y={middle.y - 7}>{edge.label.text}</text>}
                  {selectedRoute && edgeEndpointPreview && previewAnchor && <line className="mindmap-link-preview" x1={previewAnchor.x} y1={previewAnchor.y} x2={edgeEndpointPreview.point.x} y2={edgeEndpointPreview.point.y} />}
                  {selectedRoute && capabilities.has("mindmap.updateEdge") && first && <circle className="mindmap-link-endpoint" cx={first.x} cy={first.y} r="7" role="button" tabIndex={0} aria-label="拖动关联线起点" onPointerDown={(event) => { event.preventDefault(); event.stopPropagation(); edgeEndpointGesture.current = { pointerId: event.pointerId, endpoint: "source", targetNodeId: null }; event.currentTarget.setPointerCapture(event.pointerId); }} />}
                  {selectedRoute && capabilities.has("mindmap.updateEdge") && last && <circle className="mindmap-link-endpoint" cx={last.x} cy={last.y} r="7" role="button" tabIndex={0} aria-label="拖动关联线终点" onPointerDown={(event) => { event.preventDefault(); event.stopPropagation(); edgeEndpointGesture.current = { pointerId: event.pointerId, endpoint: "target", targetNodeId: null }; event.currentTarget.setPointerCapture(event.pointerId); }} />}
                </g>;
              })}
            </svg>
            {renderedLayoutNodes.map((layoutNode) => {
              const node = index.nodeById.get(layoutNode.id);
              if (!node) return null;
              const selectedNode = selectedIds.includes(node.id);
              const hint = dropTarget?.id === node.id ? `is-drop-${dropTarget.position}` : "";
              return (
                <div
                  key={node.id}
                  className={`mindmap-node mindmap-node--${node.style.shape} ${node.parentId === null ? "mindmap-node--root" : layoutNode.depth === 1 ? "mindmap-node--branch" : "mindmap-node--leaf"} ${selectedNode ? "is-selected" : ""} ${matches.includes(node.id) ? "is-match" : ""} ${edgeEndpointPreview?.targetNodeId === node.id ? "is-edge-target" : ""} ${hint}`}
                  style={{ ...nodeStyle(layoutNode, node.style, nodeThemeColors(layoutNode.depth, branchColors.get(node.id) ?? mindmapTheme(model.settings.themeId).root, theme)), "--node-accent": branchColors.get(node.id), "--node-root": mindmapTheme(model.settings.themeId).root } as CSSProperties}
                  draggable={!busy && editingId !== node.id}
                  onClick={(event) => { event.stopPropagation(); selectNode(node.id, event.metaKey || event.ctrlKey); }}
                  onDoubleClick={(event) => { event.stopPropagation(); if (capabilities.has("mindmap.replaceNodeText")) setEditingId(node.id); }}
                  onContextMenu={(event) => { event.preventDefault(); event.stopPropagation(); selectNode(node.id); setContextMenu({ x: event.clientX, y: event.clientY, nodeId: node.id }); }}
                  onDragStart={(event) => { event.dataTransfer.effectAllowed = "move"; event.dataTransfer.setData("application/x-open-office-mindmap-node", node.id); }}
                  onDragOver={(event) => { event.preventDefault(); setDropTarget({ id: node.id, position: dropPosition(event) }); }}
                  onDragLeave={(event) => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDropTarget(null); }}
                  onDrop={(event) => { event.preventDefault(); const sourceId = event.dataTransfer.getData("application/x-open-office-mindmap-node"); const target = { id: node.id, position: dropPosition(event) }; setDropTarget(null); void moveNode(sourceId, target); }}
                >
                  {editingId === node.id ? (
                    <MindmapRichTextEditor
                      nodeId={node.id}
                      content={node.content ?? richText("")}
                      disabled={busy || !capabilities.has("mindmap.replaceNodeText")}
                      patchDisabled={busy || !capabilities.has("mindmap.patchNodeTextRange")}
                      onCommit={(content) => void replaceNodeText(node.id, content)}
                      onPatch={(draft, range, patch) => void patchNodeText(node.id, draft, range, patch)}
                      onCancel={() => setEditingId(null)}
                    />
                  ) : <>
                    {node.supplement.image && <img className="mindmap-node__image" src={api.assetUrl(id, node.supplement.image.assetId)} alt={node.supplement.image.alt} />}
                    <RichTextView node={node} />
                    <span className="mindmap-node__badges">{node.supplement.note && <i title="包含备注">▤</i>}{node.supplement.hyperlink && <a href={node.supplement.hyperlink} target="_blank" rel="noreferrer" title={node.supplement.hyperlink} onClick={(event) => event.stopPropagation()}>↗</a>}{node.supplement.markers.map((marker) => <i key={marker}>{marker}</i>)}</span>
                  </>}
                  {editingId !== node.id && <button className="mindmap-node__add" aria-label="快捷添加子主题" title="添加子主题 · Tab" disabled={busy} onClick={(event) => { event.stopPropagation(); void addNode("child", node.id); }}><Icon name="plus" size={12} /><span>子主题</span></button>}
                  {(index.children.get(node.id)?.length ?? 0) > 0 && (
                    <button disabled={busy} className="mindmap-node__collapse" title={node.collapsed ? "展开分支" : "折叠分支"} aria-expanded={!node.collapsed} onClick={(event) => { event.stopPropagation(); void submit([{ typeId: "mindmap.setNodeCollapsed", payload: { type: "setNodeCollapsed", nodeId: node.id, collapsed: !node.collapsed } }]); }} aria-label={node.collapsed ? "展开分支" : "折叠分支"}><Icon name={node.collapsed ? "arrow-right" : "arrow-left"} size={12} /></button>
                  )}
                </div>
              );
            })}
            <PresenceOverlay participants={remotePresence} layoutById={layoutById} />
          </div>
          {marquee && <div className="mindmap-marquee" style={rectStyle(normalizedRect(marquee.start, marquee.current))} />}
          {model.root && <MiniMap projection={projection} selectedIds={selectedIds} pan={pan} zoom={zoom} onFocus={focusNode} />}
        </div>
        {inspectorOpen && (selectedAdvanced
          ? <MindmapAdvancedInspector model={model} selection={selectedAdvanced} capabilities={capabilities} disabled={busy} onUpdate={updateAdvanced} onDelete={() => void deleteAdvanced()} />
          : <Inspector defaultColors={nodeThemeColors(selected ? index.depth.get(selected.id) ?? 0 : 0, (selected && branchColors.get(selected.id)) || mindmapTheme(model.settings.themeId).root, theme)} model={model} selected={selected} selectedEdge={selectedEdge} edgeCapabilities={capabilities} disabled={busy} onLayout={setLayout} onConnectorShape={setConnectorShape} onNodeStyle={setNodeStyle} onSupplement={setNodeSupplement} onUploadImage={uploadNodeImage} onToggleTextStyle={toggleWholeTextStyle} onFontFamily={setWholeTextFont} onUpdateEdge={updateEdge} onEdgeStyle={setEdgeStyle} onDeleteEdge={() => void deleteSelectedEdge()} />)}
      </main>
      <footer className="mindmap-statusbar">
        <span className="mindmap-statusbar__count">{model.nodes.length} 个主题{selectedAdvanced ? ` · 已选${selectedAdvanced.type === "summary" ? "概要" : selectedAdvanced.type === "boundary" ? "外框" : "公式"}` : selectedEdge ? " · 已选关联线" : selectedIds.length > 0 ? ` · 已选 ${selectedIds.length}` : ""}</span>
        <span className="mindmap-statusbar__hint">Tab 添加子主题 · Enter 添加同级主题</span>
        <div className="mindmap-zoom" role="group" aria-label="画布缩放">
          <button onClick={fitView} title="适合画布"><Icon name="compress" size={15} /><span>适合画布</span></button>
          <span className="mindmap-toolbar__divider" />
          <button aria-label="缩小" title="缩小" disabled={zoom <= .25} onClick={() => setZoom((value) => clamp(value - .1, .25, 2.5))}>−</button>
          <button className="mindmap-zoom__value" aria-label="重置缩放" title="重置为 100%" onClick={() => setZoom(1)}>{Math.round(zoom * 100)}%</button>
          <button aria-label="放大" title="放大" disabled={zoom >= 2.5} onClick={() => setZoom((value) => clamp(value + .1, .25, 2.5))}><Icon name="plus" size={14} /></button>
        </div>
      </footer>
      {contextMenu && (
        <div className="mindmap-context" style={{ left: contextMenu.x, top: contextMenu.y }} role="menu" onMouseLeave={() => setContextMenu(null)}>
          <button disabled={busy} onClick={() => { setContextMenu(null); void addNode("child"); }}>添加子主题 <kbd>Tab</kbd></button>
          <button disabled={busy || !selected?.parentId} onClick={() => { setContextMenu(null); void addNode("sibling"); }}>添加同级主题 <kbd>Enter</kbd></button>
          <button disabled={busy || !capabilities.has("mindmap.replaceNodeText")} onClick={() => { const node = index.nodeById.get(contextMenu.nodeId); setContextMenu(null); if (node) setEditingId(node.id); }}>重命名 <kbd>F2</kbd></button>
          <button onClick={() => { setContextMenu(null); void copySelection(false); }}>复制</button>
          <button disabled={busy} className="is-danger" onClick={() => { setContextMenu(null); void deleteSelection(); }}>删除</button>
        </div>
      )}
      {importNotice && <p className="mindmap-import-notice" role="status" onClick={() => setImportNotice(null)}>{importNotice}</p>}
      {error && <p className="mindmap-error" onClick={() => setError(null)}>{error}</p>}
    </div>
  );
}

function MindmapTool({ icon, label, compact, className = "", ...props }: ButtonHTMLAttributes<HTMLButtonElement> & { icon: IconName; label: string; compact?: boolean }) {
  return <button type="button" aria-label={label} title={label} className={`mindmap-tool ${compact ? "mindmap-tool--compact" : ""} ${className}`} {...props}><Icon name={icon} size={17} />{!compact && <span>{label}</span>}</button>;
}

function PresenceOverlay({ participants, layoutById }: { participants: readonly PresentationPresenceParticipant[]; layoutById: Map<string, { id: string; depth: number; x: number; y: number; width: number; height: number }> }) {
  return <div className="mindmap-presence-layer" aria-live="polite" aria-label="协作者状态">
    {participants.flatMap((participant, participantIndex) => participant.selectedNodeIds.map((nodeId) => {
      const node = layoutById.get(nodeId);
      if (!node) return null;
      return <div key={`${participant.sessionId}:${nodeId}`} className="mindmap-remote-selection" style={{ left: node.x - 4, top: node.y - 4, width: node.width + 8, height: node.height + 8, "--presence-color": presenceColor(participantIndex) } as CSSProperties} title={`${participant.displayName} 正在选择此主题`} />;
    }))}
    {participants.map((participant, participantIndex) => participant.cursor && <div key={`${participant.sessionId}:cursor`} className="mindmap-remote-cursor" style={{ left: participant.cursor.x, top: participant.cursor.y, "--presence-color": presenceColor(participantIndex) } as CSSProperties}><span>{participant.displayName}</span></div>)}
  </div>;
}

function OutlinePanel({ model, index, selectedIds, onSelect }: { model: MindmapModel; index: ReturnType<typeof buildMindmapIndex>; selectedIds: string[]; onSelect: (id: string) => void }) {
  const visible: MindmapNode[] = [];
  let hiddenBelowDepth: number | null = null;
  for (const node of model.nodes) {
    const depth = index.depth.get(node.id) ?? 0;
    if (hiddenBelowDepth !== null && depth > hiddenBelowDepth) continue;
    hiddenBelowDepth = null;
    visible.push(node);
    if (node.collapsed) hiddenBelowDepth = depth;
    if (visible.length >= (model.nodes.length > 600 ? 250 : 1_000)) break;
  }
  return <aside className="mindmap-outline"><h2>大纲</h2>{model.root ? <ul>{visible.map((node) => <li key={node.id}><button className={selectedIds.includes(node.id) ? "is-selected" : ""} style={{ paddingLeft: `${.4 + (index.depth.get(node.id) ?? 0) * .75}rem` }} onClick={() => onSelect(node.id)}><span>{node.collapsed ? "▸" : (index.children.get(node.id)?.length ?? 0) ? "▾" : "·"}</span>{node.content?.text || "未命名主题"}</button></li>)}</ul> : <p>还没有主题</p>}{model.nodes.length > visible.length && <small>大纲仅显示前 {visible.length} 项，可使用搜索定位其余主题。</small>}<small>Shift 拖动画框多选</small></aside>;
}

function Inspector({ defaultColors, model, selected, selectedEdge, edgeCapabilities, disabled, onLayout, onConnectorShape, onNodeStyle, onSupplement, onUploadImage, onToggleTextStyle, onFontFamily, onUpdateEdge, onEdgeStyle, onDeleteEdge }: { defaultColors: ReturnType<typeof nodeThemeColors>; model: MindmapModel; selected: MindmapNode | null; selectedEdge: MindmapEdge | null; edgeCapabilities: ReadonlySet<string>; disabled: boolean; onLayout: (layout: MindmapLayoutKind) => void; onConnectorShape: (shape: MindmapConnectorShape) => void; onNodeStyle: (patch: Partial<MindmapNodeStyle>) => void; onSupplement: (patch: Partial<MindmapNodeSupplement>) => void; onUploadImage: (file: File) => Promise<void>; onFontFamily: (value: string) => void; onToggleTextStyle: (field: "bold" | "italic" | "underline") => void; onUpdateEdge: (patch: { sourceId?: string; targetId?: string; label?: ReturnType<typeof richText> | null }) => void; onEdgeStyle: (patch: Partial<MindmapConnectorStyle>) => void; onDeleteEdge: () => void }) {
  const edgeUpdateDisabled = disabled || !edgeCapabilities.has("mindmap.updateEdge");
  const edgeStyleDisabled = disabled || !edgeCapabilities.has("mindmap.setEdgeStyle");
  const nodeTextPatchDisabled = disabled || !edgeCapabilities.has("mindmap.patchNodeTextRange");
  return <aside className="mindmap-inspector">
    <h2>脑图设置</h2>
    <label>布局<select aria-label="布局" disabled={disabled} value={model.settings.layout} onChange={(event) => onLayout(event.target.value as MindmapLayoutKind)}>{LAYOUT_LABELS.map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
    <label>连线<select aria-label="连线" disabled={disabled} value={model.settings.connector.shape} onChange={(event) => onConnectorShape(event.target.value as MindmapConnectorShape)}><option value="orthogonal">折线</option><option value="curve">曲线</option><option value="straight">直线</option></select></label>
    <hr />
    <h2>{selectedEdge ? "所选关联线" : "所选主题"}</h2>
    {selectedEdge ? <>
      <label>起点<select aria-label="关联线起点" disabled={edgeUpdateDisabled} value={selectedEdge.sourceId} onChange={(event) => onUpdateEdge({ sourceId: event.target.value })}>{model.nodes.map((node) => <option key={node.id} value={node.id} disabled={node.id === selectedEdge.targetId}>{node.content?.text || "未命名主题"}</option>)}</select></label>
      <label>终点<select aria-label="关联线终点" disabled={edgeUpdateDisabled} value={selectedEdge.targetId} onChange={(event) => onUpdateEdge({ targetId: event.target.value })}>{model.nodes.map((node) => <option key={node.id} value={node.id} disabled={node.id === selectedEdge.sourceId}>{node.content?.text || "未命名主题"}</option>)}</select></label>
      <label className="mindmap-inspector__stack">标签<input aria-label="关联线标签" disabled={edgeUpdateDisabled} key={`${selectedEdge.id}:label`} defaultValue={selectedEdge.label?.text ?? ""} placeholder="输入关系说明" onBlur={(event) => onUpdateEdge({ label: event.currentTarget.value.trim() ? richText(event.currentTarget.value) : null })} /></label>
      <label>线型<select aria-label="关联线线型" disabled={edgeStyleDisabled} value={selectedEdge.style.shape} onChange={(event) => onEdgeStyle({ shape: event.target.value as MindmapConnectorShape })}><option value="orthogonal">折线</option><option value="curve">曲线</option><option value="straight">直线</option></select></label>
      <label>颜色<input aria-label="关联线颜色" disabled={edgeStyleDisabled} type="color" value={selectedEdge.style.color ?? "#9a57d5"} onChange={(event) => onEdgeStyle({ color: event.target.value })} /></label>
      <label>宽度<input aria-label="关联线宽度" disabled={edgeStyleDisabled} type="number" min="0.5" max="16" step="0.5" value={selectedEdge.style.width} onChange={(event) => onEdgeStyle({ width: Number(event.target.value) })} /></label>
      <label className="mindmap-inspector__check"><span>虚线</span><input aria-label="关联线虚线" disabled={edgeStyleDisabled} type="checkbox" checked={selectedEdge.style.dashed} onChange={(event) => onEdgeStyle({ dashed: event.target.checked })} /></label>
      <button className="is-danger" disabled={disabled || !edgeCapabilities.has("mindmap.deleteEdge")} onClick={onDeleteEdge}>删除关联线</button>
      <p className="mindmap-inspector__hint">可用 Delete 删除，Enter 或空格选择连线。</p>
    </> : selected ? <>
      <p className="mindmap-inspector__label">{selected.content?.text || "未命名主题"}</p>
      <div className="mindmap-inspector__rich"><button aria-label="粗体" aria-pressed={selected.content?.runs[0]?.style.bold === true} disabled={nodeTextPatchDisabled} onClick={() => onToggleTextStyle("bold")}><b>B</b></button><button aria-label="斜体" aria-pressed={selected.content?.runs[0]?.style.italic === true} disabled={nodeTextPatchDisabled} onClick={() => onToggleTextStyle("italic")}><i>I</i></button><button aria-label="下划线" aria-pressed={selected.content?.runs[0]?.style.underline === true} disabled={nodeTextPatchDisabled} onClick={() => onToggleTextStyle("underline")}><u>U</u></button></div>
      <label>字体<FontPicker value={selected.content?.runs[0]?.style.fontFamily ?? ""} disabled={nodeTextPatchDisabled} onChange={onFontFamily} /></label>
      <label>形状<select aria-label="形状" disabled={disabled} value={selected.style.shape} onChange={(event) => onNodeStyle({ shape: event.target.value as MindmapNodeShape })}>{SHAPE_LABELS.map(([value, label]) => <option key={value} value={value}>{label}</option>)}</select></label>
      <label>填充<input aria-label="填充" disabled={disabled} type="color" value={asColor(selected.style.fillColor, defaultColors.fillColor)} onChange={(event) => onNodeStyle({ fillColor: event.target.value })} /></label>
      <label>边框<input aria-label="边框" disabled={disabled} type="color" value={asColor(selected.style.borderColor, defaultColors.borderColor)} onChange={(event) => onNodeStyle({ borderColor: event.target.value })} /></label>
      <label>文字<input aria-label="文字" disabled={disabled} type="color" value={asColor(selected.style.textColor, defaultColors.textColor)} onChange={(event) => onNodeStyle({ textColor: event.target.value })} /></label>
      <button disabled={disabled || (!selected.style.fillColor && !selected.style.borderColor && !selected.style.textColor)} onClick={() => onNodeStyle({ fillColor: null, borderColor: null, textColor: null })}>恢复主题配色</button>
      <label className="mindmap-inspector__stack">备注<textarea aria-label="备注" disabled={disabled} key={`${selected.id}:note`} defaultValue={selected.supplement.note?.text ?? ""} placeholder="补充说明" onBlur={(event) => onSupplement({ note: event.target.value.trim() ? richText(event.target.value) : null })} /></label>
      <label className="mindmap-inspector__stack">链接<input aria-label="链接" disabled={disabled} key={`${selected.id}:link`} type="url" defaultValue={selected.supplement.hyperlink ?? ""} placeholder="https://" onBlur={(event) => onSupplement({ hyperlink: event.target.value.trim() || null })} /></label>
      <label className="mindmap-inspector__stack">标记<input aria-label="标记" disabled={disabled} key={`${selected.id}:markers`} defaultValue={selected.supplement.markers.join(", ")} placeholder="重点, 待办" onBlur={(event) => onSupplement({ markers: event.target.value.split(",").map((value) => value.trim()).filter(Boolean) })} /></label>
      <label className="mindmap-inspector__stack">图片<input aria-label="图片" disabled={disabled} type="file" accept="image/*" onChange={(event) => { const file = event.target.files?.[0]; if (file) void onUploadImage(file); event.currentTarget.value = ""; }} /></label>
      {selected.supplement.image && <button disabled={disabled} onClick={() => onSupplement({ image: null })}>移除图片</button>}
      <p className="mindmap-inspector__hint">双击或 F2 编辑文字</p>
    </> : <p>选择一个主题或关联线后可调整属性。</p>}
  </aside>;
}

function RichTextView({ node }: { node: MindmapNode }) {
  const text = node.content?.text || "未命名主题";
  if (!node.content?.runs.length) return <span className="mindmap-node__text">{text}</span>;
  const characters = [...text];
  const whole = node.content.runs.length === 1 ? node.content.runs[0]?.style : null;
  return <span className="mindmap-node__text" style={whole ? { fontFamily: whole.fontFamily ?? undefined, fontWeight: whole.bold ? 700 : undefined, fontStyle: whole.italic ? "italic" : undefined, textDecoration: whole.underline ? "underline" : undefined } : undefined}>{node.content.runs.map(run => <span key={run.start} style={{ fontFamily: run.style.fontFamily ?? undefined, fontSize: run.style.fontSize ?? undefined, color: run.style.color ?? undefined, fontWeight: run.style.bold ? 700 : undefined, fontStyle: run.style.italic ? "italic" : undefined, textDecoration: [run.style.underline ? "underline" : "", run.style.strikethrough ? "line-through" : ""].filter(Boolean).join(" ") || undefined }}>{characters.slice(run.start, run.end).join("")}</span>)}</span>;
}

function MiniMap({ projection, selectedIds, onFocus }: { projection: MindmapProjection; selectedIds: string[]; pan: Point; zoom: number; onFocus: (id: string) => void }) {
  const width = 170;
  const height = 105;
  const scale = Math.min(width / Math.max(projection.layout.width, 1), height / Math.max(projection.layout.height, 1));
  const nodeStride = Math.max(1, Math.ceil(projection.layout.nodes.length / 500));
  const routeStride = Math.max(1, Math.ceil(projection.edges.routes.length / 500));
  const nodes = projection.layout.nodes.filter((node, index) => index % nodeStride === 0 || selectedIds.includes(node.id));
  const routes = projection.edges.routes.filter((_, index) => index % routeStride === 0);
  return <svg className="mindmap-minimap" width={width} height={height} viewBox={`0 0 ${width} ${height}`} aria-label="脑图小地图">
    <rect width={width} height={height} rx="8" className="mindmap-minimap__bg" />
    {routes.map((route) => <polyline key={route.edgeId ?? `${route.parentId}:${route.childId}`} points={route.points.map((point) => `${point.x * scale},${point.y * scale}`).join(" ")} className="mindmap-minimap__link" />)}
    {nodes.map((node) => <rect key={node.id} x={node.x * scale} y={node.y * scale} width={Math.max(3, node.width * scale)} height={Math.max(2, node.height * scale)} className={selectedIds.includes(node.id) ? "is-selected" : ""} onClick={() => onFocus(node.id)} />)}
  </svg>;
}

function nodeStyle(layout: { x: number; y: number; width: number; height: number }, style: MindmapNodeStyle, defaults: ReturnType<typeof nodeThemeColors>): CSSProperties {
  return {
    left: layout.x,
    top: layout.y,
    width: layout.width,
    minHeight: layout.height,
    background: style.fillColor ?? defaults.fillColor,
    borderColor: style.borderColor ?? defaults.borderColor,
    color: style.textColor ?? defaults.textColor,
    borderWidth: style.borderWidth,
    textAlign: style.textAlign === "start" ? "left" : style.textAlign === "end" ? "right" : "center",
  };
}

function routePath(points: Point[], shape: MindmapConnectorShape): string {
  if (points.length < 2) return "";
  const start = points[0];
  const end = points.at(-1)!;
  if (shape === "straight") return `M ${start.x} ${start.y} L ${end.x} ${end.y}`;
  if (shape === "curve") {
    const mid = (start.x + end.x) / 2;
    return `M ${start.x} ${start.y} C ${mid} ${start.y}, ${mid} ${end.y}, ${end.x} ${end.y}`;
  }
  return `M ${points.map((point) => `${point.x} ${point.y}`).join(" L ")}`;
}

function dropPosition(event: DragEvent<HTMLElement>): DropPosition {
  const rect = event.currentTarget.getBoundingClientRect();
  const ratio = (event.clientY - rect.top) / rect.height;
  return ratio < .28 ? "before" : ratio > .72 ? "after" : "child";
}

function richText(text: string) { return { text, runs: [] }; }
function randomId() { return globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`; }
function clamp(value: number, min: number, max: number) { return Math.min(max, Math.max(min, value)); }
function asColor(value: string | null, fallback: string) { return value?.startsWith("#") && (value.length === 4 || value.length === 7) ? value : fallback; }
function isTextTarget(target: EventTarget | null) { return target instanceof HTMLInputElement || target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement || (target instanceof HTMLElement && target.isContentEditable); }
function errorMessage(reason: unknown) { return reason instanceof Error ? reason.message : "操作失败"; }
function normalizedRect(a: Point, b: Point) { return { x: Math.min(a.x, b.x), y: Math.min(a.y, b.y), width: Math.abs(a.x - b.x), height: Math.abs(a.y - b.y) }; }
function rectStyle(rect: { x: number; y: number; width: number; height: number }): CSSProperties { return { left: rect.x, top: rect.y, width: rect.width, height: rect.height }; }
function presenceColor(index: number) { return ["#e5484d", "#8e4ec6", "#0d9b8a", "#d97706", "#2563eb"][index % 5]; }

function incrementalProjectionInvalidation(notice: MindmapRevisionNotice): MindmapProjectionInvalidation | null {
  if (notice.structureChanged || notice.changedEntities.length === 0) return null;
  const changedEntities = notice.changedEntities.map((key) => {
    const separator = key.indexOf("\u001f");
    return separator > 0 ? { entityType: key.slice(0, separator), entityId: key.slice(separator + 1) } : null;
  });
  if (changedEntities.some((entity) => !entity || !["mindmap.edge", "mindmap.summary", "mindmap.boundary", "mindmap.formula"].includes(entity.entityType))) return null;
  return { changedEntities: changedEntities as Array<{ entityType: string; entityId: string }>, changedContainers: [], structureChanged: false };
}
