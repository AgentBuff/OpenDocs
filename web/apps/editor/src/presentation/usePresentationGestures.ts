import {
  type Dispatch,
  type KeyboardEvent as ReactKeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type SetStateAction,
  useCallback,
  useRef,
  useState,
} from "react";

import type { PresentationV5Node, PresentationV5Transform } from "@open-office/schema";
import type { PresentationSlideProjection } from "@open-office/schema/api";

import { presentationSemanticInputs } from "./commands.js";
import { changedNodeTransforms, dragPreview, snapMovePreview, type PresentationDragMode, type PresentationSnapGuide } from "./interactions.js";
import { createNodeContext, presentationNodeUiRegistry } from "./presentationNodeContext.js";
import {
  clamp,
  sameConnectorEndpoints,
  snapConnectorEndpoint,
  withoutConnectorPreview,
  type ConnectorPreview,
} from "./presentationGeometry.js";
import type { PresentationSubmit } from "./usePresentationSession.js";

type DragState = {
  nodeIds: readonly string[];
  mode: PresentationDragMode;
  originClientX: number;
  originClientY: number;
  origins: Readonly<Record<string, PresentationV5Transform>>;
  rotationCenter: { x: number; y: number } | null;
  originPointerAngle: number | null;
};

type ConnectorDragState = {
  nodeId: string;
  endpoint: "start" | "end";
  pointerId: number;
};

export interface PresentationGestureOptions {
  artifactId: string;
  revision: number | null;
  slide: PresentationSlideProjection | null;
  pageSpec: { width: number; height: number } | null | undefined;
  scale: number;
  nodes: readonly PresentationV5Node[];
  selectedNodeIds: readonly string[];
  availableCapabilities: ReadonlySet<string>;
  submit: PresentationSubmit;
  updatePresenceCursor: (cursor: { x: number; y: number }) => void;
  setSelectedNodeId: Dispatch<SetStateAction<string | null>>;
  setSelectedNodeIds: Dispatch<SetStateAction<readonly string[]>>;
  selectNodes: (nodeId: string, extend?: boolean) => void;
  clearTableSelection: () => void;
  setEditingNodeId: Dispatch<SetStateAction<string | null>>;
  setInspectorOpen: Dispatch<SetStateAction<boolean>>;
  setSlideInspectorOpen: Dispatch<SetStateAction<boolean>>;
  setDeckInspectorOpen: Dispatch<SetStateAction<boolean>>;
}

export function usePresentationGestures({
  artifactId,
  revision,
  slide,
  pageSpec,
  scale,
  nodes,
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
}: PresentationGestureOptions) {
  const [preview, setPreview] = useState<Record<string, PresentationV5Transform>>({});
  const previewRef = useRef<Record<string, PresentationV5Transform>>({});
  const [connectorPreview, setConnectorPreview] = useState<ConnectorPreview>({});
  const [snapGuides, setSnapGuides] = useState<readonly PresentationSnapGuide[]>([]);
  const connectorPreviewRef = useRef<ConnectorPreview>({});
  const drag = useRef<DragState | null>(null);
  const connectorDrag = useRef<ConnectorDragState | null>(null);

  const resetPreviews = useCallback(() => {
    drag.current = null;
    connectorDrag.current = null;
    previewRef.current = {};
    connectorPreviewRef.current = {};
    setPreview({});
    setConnectorPreview({});
    setSnapGuides([]);
  }, []);

  const effectiveTransform = useCallback(
    (node: PresentationV5Node) => preview[node.id] ?? node.transform,
    [preview],
  );

  const pointerDown = useCallback((
    event: ReactPointerEvent<HTMLElement>,
    node: PresentationV5Node,
    mode: PresentationDragMode,
  ) => {
    const extend = event.shiftKey || event.metaKey || event.ctrlKey;
    const nodeIds = mode !== "move"
      ? [node.id]
      : extend
        ? (selectedNodeIds.includes(node.id) ? selectedNodeIds : [...selectedNodeIds, node.id])
        : (selectedNodeIds.includes(node.id) ? selectedNodeIds : [node.id]);
    if (!extend) {
      setSelectedNodeId(node.id);
      setSelectedNodeIds(nodeIds);
      clearTableSelection();
    } else {
      selectNodes(node.id, true);
      return;
    }
    setEditingNodeId(null);
    setSlideInspectorOpen(false);
    setDeckInspectorOpen(false);
    setInspectorOpen(true);
    const canTransform = Boolean(slide && revision !== null && presentationNodeUiRegistry.resolve(createNodeContext({
      artifactId,
      revision,
      slide,
      node,
      selectedNodeIds: nodeIds,
      mode: "node",
      availableCapabilities,
    })).toolbar.some((item) => item.action === "node.transform"));
    if (nodeIds.some((id) => nodes.find((candidate) => candidate.id === id)?.locked) || !pageSpec || !canTransform) return;
    event.stopPropagation();
    event.currentTarget.setPointerCapture(event.pointerId);
    const origins = Object.fromEntries(nodes
      .filter((candidate) => nodeIds.includes(candidate.id))
      .map((candidate) => [candidate.id, effectiveTransform(candidate)]));
    drag.current = {
      nodeIds,
      mode,
      originClientX: event.clientX,
      originClientY: event.clientY,
      origins,
      rotationCenter: null,
      originPointerAngle: null,
    };
    if (mode === "rotate") {
      const element = event.currentTarget.closest<HTMLElement>("[data-node-id]") ?? event.currentTarget;
      const bounds = element.getBoundingClientRect();
      const center = { x: bounds.left + bounds.width / 2, y: bounds.top + bounds.height / 2 };
      drag.current.rotationCenter = center;
      drag.current.originPointerAngle = Math.atan2(event.clientY - center.y, event.clientX - center.x);
    }
  }, [artifactId, availableCapabilities, clearTableSelection, effectiveTransform, nodes, pageSpec, revision, selectNodes, selectedNodeIds, setDeckInspectorOpen, setEditingNodeId, setInspectorOpen, setSelectedNodeId, setSelectedNodeIds, setSlideInspectorOpen, slide]);

  const connectorPointerDown = useCallback((
    event: ReactPointerEvent<SVGCircleElement>,
    nodeId: string,
    endpoint: "start" | "end",
  ) => {
    const node = nodes.find((candidate) => candidate.id === nodeId);
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
  }, [availableCapabilities, nodes, setDeckInspectorOpen, setEditingNodeId, setInspectorOpen, setSelectedNodeId, setSelectedNodeIds, setSlideInspectorOpen]);

  const pointerMove = useCallback((event: ReactPointerEvent<HTMLElement>) => {
    const current = drag.current;
    if (!current || !pageSpec) return;
    let nextPreview = dragPreview({
      nodeIds: current.nodeIds,
      mode: current.mode,
      origins: current.origins,
      deltaClientX: event.clientX - current.originClientX,
      deltaClientY: event.clientY - current.originClientY,
      scale,
      rotationDelta: current.mode === "rotate" && current.rotationCenter && current.originPointerAngle !== null
        ? (Math.atan2(event.clientY - current.rotationCenter.y, event.clientX - current.rotationCenter.x) - current.originPointerAngle) * 180 / Math.PI
        : 0,
    });
    if (current.mode === "move") {
      const snapped = snapMovePreview({
        preview: nextPreview,
        movingNodeIds: current.nodeIds,
        nodes,
        page: pageSpec,
        threshold: 8 / scale,
      });
      nextPreview = snapped.preview;
      setSnapGuides(snapped.guides);
    } else {
      setSnapGuides([]);
    }
    previewRef.current = nextPreview;
    setPreview(nextPreview);
  }, [nodes, pageSpec, scale]);

  const connectorPointerMove = useCallback((event: ReactPointerEvent<HTMLElement>) => {
    const current = connectorDrag.current;
    if (!current || current.pointerId !== event.pointerId || !pageSpec) return;
    const node = nodes.find((candidate) => candidate.id === current.nodeId);
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
  }, [nodes, pageSpec, scale]);

  const recordPresenceCursor = useCallback((event: ReactPointerEvent<HTMLElement>) => {
    if (!pageSpec) return;
    const bounds = event.currentTarget.getBoundingClientRect();
    const x = (event.clientX - bounds.left) / scale;
    const y = (event.clientY - bounds.top) / scale;
    if (x < 0 || y < 0 || x > pageSpec.width || y > pageSpec.height) return;
    updatePresenceCursor({ x, y });
  }, [pageSpec, scale, updatePresenceCursor]);

  const pointerUp = useCallback(() => {
    const current = drag.current;
    drag.current = null;
    setSnapGuides([]);
    if (!current || !slide) return;
    const latestPreview = previewRef.current;
    const commands = changedNodeTransforms(slide.nodes ?? [], latestPreview)
      .map(({ nodeId, transform }) => ({
        type: "setNodeTransform" as const,
        slideId: slide.slideId,
        nodeId,
        transform,
      }));
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
    const node = nodes.find((candidate) => candidate.id === current.nodeId);
    if (!node || node.kind.type !== "connector") return;
    const draft = connectorPreviewRef.current[node.id] ?? node.kind.data;
    const endpoint = current.endpoint === "start" ? draft.start : draft.end;
    const snapped = endpoint.type === "free"
      ? snapConnectorEndpoint(endpoint, node.id, nodes, previewRef.current)
      : endpoint;
    const next = current.endpoint === "start"
      ? { start: snapped, end: draft.end }
      : { start: draft.start, end: snapped };
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
  }, [nodes, slide, submit]);

  const cancelConnectorPointer = useCallback(() => {
    const current = connectorDrag.current;
    connectorDrag.current = null;
    if (!current) return;
    connectorPreviewRef.current = withoutConnectorPreview(connectorPreviewRef.current, current.nodeId);
    setConnectorPreview(connectorPreviewRef.current);
  }, []);

  const stagePointerDown = useCallback((event: ReactPointerEvent<HTMLElement>) => {
    if (event.target !== event.currentTarget && !(event.target instanceof HTMLCanvasElement)) return;
    setSelectedNodeId(null);
    setSelectedNodeIds([]);
    clearTableSelection();
    setEditingNodeId(null);
    setInspectorOpen(false);
    setSlideInspectorOpen(Boolean(slide));
    setDeckInspectorOpen(false);
  }, [clearTableSelection, setDeckInspectorOpen, setEditingNodeId, setInspectorOpen, setSelectedNodeId, setSelectedNodeIds, setSlideInspectorOpen, slide]);

  const keyboardNudge = useCallback((event: ReactKeyboardEvent<HTMLElement>) => {
    if (!pageSpec || !["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key)) return;
    const target = event.target as HTMLElement;
    if (target.matches("input, textarea, select, [contenteditable='true']")) return;
    const selected = nodes.filter((node) => selectedNodeIds.includes(node.id) && !node.locked);
    if (selected.length === 0) return;
    event.preventDefault();
    const amount = 12_700 * (event.shiftKey ? 10 : 1);
    const deltaX = event.key === "ArrowLeft" ? -amount : event.key === "ArrowRight" ? amount : 0;
    const deltaY = event.key === "ArrowUp" ? -amount : event.key === "ArrowDown" ? amount : 0;
    const next = { ...previewRef.current };
    for (const node of selected) {
      const current = next[node.id] ?? node.transform;
      next[node.id] = {
        ...current,
        x: clamp(current.x + deltaX, 0, Math.max(0, pageSpec.width - current.width)),
        y: clamp(current.y + deltaY, 0, Math.max(0, pageSpec.height - current.height)),
      };
    }
    previewRef.current = next;
    setPreview(next);
  }, [nodes, pageSpec, selectedNodeIds]);

  const keyboardNudgeCommit = useCallback((event: ReactKeyboardEvent<HTMLElement>) => {
    if (!["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown"].includes(event.key) || !slide) return;
    const commands = changedNodeTransforms(slide.nodes ?? [], previewRef.current)
      .map(({ nodeId, transform }) => ({ type: "setNodeTransform" as const, slideId: slide.slideId, nodeId, transform }));
    if (commands.length === 0) return;
    previewRef.current = {};
    setPreview({});
    void submit(presentationSemanticInputs(commands));
  }, [slide, submit]);

  return {
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
  };
}
