import {
  type Dispatch,
  type MutableRefObject,
  type SetStateAction,
  useCallback,
  useRef,
} from "react";

import { OpenOfficeSdk, type SemanticCommandInput } from "@open-office/sdk";
import type {
  PresentationV5Layout,
  PresentationV5Master,
  PresentationV5Node,
  PresentationV5RichText,
} from "@open-office/schema";
import type { PresentationSlideProjection } from "@open-office/schema/api";
import type {
  BuiltinPresentationNodeAction,
  PresentationMultiSelectionAction,
} from "@open-office/presentation-ui";

import {
  createChartNode,
  createConnectorNode,
  createImageNode,
  createLayoutCommand,
  createMasterCommand,
  createShapeNode,
  createSlideCommand,
  createTextNode,
  deleteLayoutCommand,
  deleteMasterCommand,
  deleteSlideCommand,
  duplicatePresentationNode,
  duplicateSlideCommand,
  insertNodeCommand,
  groupNodesCommand,
  moveSlideCommand,
  multiNodeArrangeCommands,
  presentationSemanticInputs,
  registerPresentationAssetCommand,
  updateLayoutCommand,
  updateMasterCommand,
} from "./commands.js";
import { createNodeContext, presentationNodeUiRegistry } from "./presentationNodeContext.js";
import type { TableSelection } from "./presentationTableSelection.js";
import {
  presentationErrorMessage,
  type PresentationStudioData,
  type PresentationSubmit,
} from "./usePresentationSession.js";

const sdk = new OpenOfficeSdk();

export interface PresentationActionOptions {
  artifactId: string;
  data: PresentationStudioData | null;
  slide: PresentationSlideProjection | null;
  nodes: readonly PresentationV5Node[];
  selectedNode: PresentationV5Node | null;
  selectedNodeIds: readonly string[];
  editingNodeId: string | null;
  availableCapabilities: ReadonlySet<string>;
  saving: boolean;
  submit: PresentationSubmit;
  setError: Dispatch<SetStateAction<string | null>>;
  setSaving: Dispatch<SetStateAction<boolean>>;
  tableSelectionRef: MutableRefObject<TableSelection | null>;
  setTableSelection: Dispatch<SetStateAction<TableSelection | null>>;
  setSelectedNodeId: Dispatch<SetStateAction<string | null>>;
  setSelectedNodeIds: Dispatch<SetStateAction<readonly string[]>>;
  setEditingNodeId: Dispatch<SetStateAction<string | null>>;
  setInspectorOpen: Dispatch<SetStateAction<boolean>>;
  setSlideInspectorOpen: Dispatch<SetStateAction<boolean>>;
  setDeckInspectorOpen: Dispatch<SetStateAction<boolean>>;
}

export function usePresentationActions({
  artifactId,
  data,
  slide,
  nodes,
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
}: PresentationActionOptions) {
  const imageInput = useRef<HTMLInputElement>(null);

  const submitNodeAction = useCallback((
    node: PresentationV5Node,
    action: BuiltinPresentationNodeAction,
    value?: unknown,
  ) => {
    if (!slide || !data) return;
    const context = createNodeContext({
      artifactId,
      revision: data.revision,
      slide,
      node,
      selectedNodeIds,
      mode: editingNodeId === node.id ? "text" : "node",
      availableCapabilities,
    });
    try {
      const commands = presentationNodeUiRegistry.mapAction(action, { context, value });
      if (commands.length > 0) void submit(presentationSemanticInputs(commands));
    } catch (reason) {
      setError(presentationErrorMessage(reason));
    }
  }, [artifactId, availableCapabilities, data, editingNodeId, selectedNodeIds, setError, slide, submit]);

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
    if (
      action === "node.bringForward" || action === "node.sendBackward" ||
      action === "node.bringToFront" || action === "node.sendToBack"
    ) {
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
      if (index !== currentIndex) {
        submitNodeAction(selectedNode, action, {
          type: "reorderNode",
          slideId: slide.slideId,
          nodeId: selectedNode.id,
          index,
        });
      }
      return;
    }
    if (action === "node.delete" || action === "group.ungroup") {
      submitNodeAction(selectedNode, action);
      return;
    }
    setInspectorOpen(true);
  }, [selectedNode, setEditingNodeId, setInspectorOpen, slide, submitNodeAction]);

  const submitTableAction = useCallback((action: BuiltinPresentationNodeAction, value?: unknown) => {
    if (!selectedNode || selectedNode.kind.type !== "table") return;
    if (
      action === "table.insertRows" || action === "table.insertColumns" ||
      action === "table.deleteRow" || action === "table.deleteColumn" ||
      action === "table.mergeCells" || action === "table.splitCell"
    ) {
      tableSelectionRef.current = null;
      setTableSelection(null);
    }
    submitNodeAction(selectedNode, action, value);
  }, [selectedNode, setTableSelection, submitNodeAction, tableSelectionRef]);

  const handleMultiSelectionAction = useCallback((action: PresentationMultiSelectionAction) => {
    if (!slide || selectedNodeIds.length < 2) return;
    if (action === "selection.group") {
      const groupId = randomId("group");
      const command = groupNodesCommand(slide.slideId, nodes, selectedNodeIds, groupId, `ui-${Date.now()}-${groupId}`);
      if (!command) return;
      void submit(presentationSemanticInputs([command])).then((committed) => {
        if (committed) {
          setSelectedNodeId(groupId);
          setSelectedNodeIds([groupId]);
        }
      });
      return;
    }
    const commands = multiNodeArrangeCommands(slide.slideId, nodes, selectedNodeIds, action);
    if (commands.length > 0) void submit(presentationSemanticInputs(commands));
  }, [nodes, selectedNodeIds, setSelectedNodeId, setSelectedNodeIds, slide, submit]);

  const createText = useCallback(() => {
    if (!slide) return;
    const id = randomId("text");
    void submit([insertNodeCommand(slide.slideId, createTextNode(id, `ui-${Date.now()}-${id}`), slide.nodes?.length ?? 0)]);
  }, [slide, submit]);

  const createShape = useCallback((geometry: "rectangle" | "ellipse" | "line" | "arrow") => {
    if (!slide) return;
    const id = randomId("shape");
    void submit([insertNodeCommand(slide.slideId, createShapeNode(id, `ui-${Date.now()}-${id}`, geometry), slide.nodes?.length ?? 0)]);
  }, [slide, submit]);

  const createConnector = useCallback(() => {
    if (!slide || !availableCapabilities.has("presentation.insertNode")) return;
    const id = randomId("connector");
    void submit([insertNodeCommand(slide.slideId, createConnectorNode(id, `ui-${Date.now()}-${id}`), slide.nodes?.length ?? 0)]);
  }, [availableCapabilities, slide, submit]);

  const createChart = useCallback(() => {
    if (!slide || !availableCapabilities.has("presentation.insertNode") || !availableCapabilities.has("presentation.setChartSpec")) return;
    const id = randomId("chart");
    void submit([insertNodeCommand(slide.slideId, createChartNode(id, `ui-${Date.now()}-${id}`), slide.nodes?.length ?? 0)]);
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
      uploaded = await sdk.api.uploadAsset(artifactId, file, file.name);
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
      if (!committed) await sdk.api.deleteAsset(artifactId, uploaded.assetId).catch(() => undefined);
    } catch (reason) {
      if (uploaded) await sdk.api.deleteAsset(artifactId, uploaded.assetId).catch(() => undefined);
      setError(presentationErrorMessage(reason));
    } finally {
      setSaving(false);
      if (imageInput.current) imageInput.current.value = "";
    }
  }, [artifactId, availableCapabilities, data, saving, setError, setSaving, slide, submit]);

  const requestImageInsert = useCallback(() => imageInput.current?.click(), []);

  const createSlide = useCallback(() => {
    const index = data?.slides.length ?? 0;
    const id = randomId("slide");
    void submit([createSlideCommand(id, `ui-${Date.now()}-${id}`, index)]);
  }, [data?.slides.length, submit]);

  const createMaster = useCallback(() => {
    const id = randomId("master");
    const master: PresentationV5Master = {
      id,
      name: "新建母版",
      background: { type: "none" },
      placeholders: [],
    };
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
    const nodeIdMap = (slide.nodes ?? []).map((node) => ({ sourceId: node.id, targetId: randomId("node") }));
    const animationIdMap = (slide.timeline?.entries ?? []).map((entry) => ({
      sourceId: entry.id,
      targetId: randomId("animation"),
    }));
    void submit([duplicateSlideCommand(
      slide.slideId,
      slideId,
      `ui-${Date.now()}-${slideId}`,
      `${slide.name || "未命名幻灯片"} 副本`,
      nodeIdMap,
      animationIdMap,
      sourceIndex + 1,
    )], "local", slideId);
  }, [data, slide, submit]);

  const moveActiveSlide = useCallback((offset: -1 | 1) => {
    if (!slide || !data) return;
    const index = data.slides.findIndex((candidate) => candidate.slideId === slide.slideId);
    const nextIndex = index + offset;
    if (index >= 0 && nextIndex >= 0 && nextIndex < data.slides.length) {
      void submit([moveSlideCommand(slide.slideId, nextIndex)], "local", slide.slideId);
    }
  }, [data, slide, submit]);

  const deleteActiveSlide = useCallback(() => {
    if (!slide || !data) return;
    const index = data.slides.findIndex((candidate) => candidate.slideId === slide.slideId);
    if (index < 0) return;
    const fallbackSlideId = data.slides[index + 1]?.slideId ?? data.slides[index - 1]?.slideId ?? null;
    setSlideInspectorOpen(false);
    void submit([deleteSlideCommand(slide.slideId)], "local", fallbackSlideId);
  }, [data, setSlideInspectorOpen, slide, submit]);

  const submitSlideProperty = useCallback((command: SemanticCommandInput) => {
    if (slide && !saving) void submit([command]);
  }, [saving, slide, submit]);

  const openSlideInspector = useCallback(() => {
    if (!slide) return;
    setSelectedNodeId(null);
    setSelectedNodeIds([]);
    setEditingNodeId(null);
    setInspectorOpen(false);
    setDeckInspectorOpen(false);
    setSlideInspectorOpen(true);
  }, [setDeckInspectorOpen, setEditingNodeId, setInspectorOpen, setSelectedNodeId, setSelectedNodeIds, setSlideInspectorOpen, slide]);

  const openDeckInspector = useCallback(() => {
    if (!data) return;
    setSelectedNodeId(null);
    setSelectedNodeIds([]);
    setEditingNodeId(null);
    setInspectorOpen(false);
    setSlideInspectorOpen(false);
    setDeckInspectorOpen(true);
  }, [data, setDeckInspectorOpen, setEditingNodeId, setInspectorOpen, setSelectedNodeId, setSelectedNodeIds, setSlideInspectorOpen]);

  const saveText = useCallback((node: PresentationV5Node, body: PresentationV5RichText) => {
    if (!slide || node.kind.type !== "text") return;
    setEditingNodeId(null);
    if (JSON.stringify(node.kind.data.frame.body) !== JSON.stringify(body)) {
      submitNodeAction(node, "text.content", body);
    }
  }, [setEditingNodeId, slide, submitNodeAction]);

  return {
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
  };
}

function randomId(prefix: string) {
  return `${prefix}-${globalThis.crypto?.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`}`;
}
