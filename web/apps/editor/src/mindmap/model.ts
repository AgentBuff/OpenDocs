import type { MindmapBoundary, MindmapEdge, MindmapFormula, MindmapModel, MindmapNode, MindmapSummary } from "@open-office/schema/artifact";

export type MindmapCommandInput = { typeId: string; payload: Record<string, unknown> };

export interface MindmapModelIndex {
  nodeById: Map<string, MindmapNode>;
  children: Map<string, MindmapNode[]>;
  depth: Map<string, number>;
}

export interface MindmapClipboardPayload {
  type: "open-office/mindmap-fragment";
  version: 2;
  roots: string[];
  nodes: MindmapNode[];
  edges: MindmapEdge[];
  summaries: MindmapSummary[];
  boundaries: MindmapBoundary[];
  formulas: MindmapFormula[];
}

export function buildMindmapIndex(model: MindmapModel | null): MindmapModelIndex {
  const nodeById = new Map<string, MindmapNode>();
  const children = new Map<string, MindmapNode[]>();
  const depth = new Map<string, number>();
  for (const node of model?.nodes ?? []) {
    nodeById.set(node.id, node);
    if (node.parentId) {
      const siblings = children.get(node.parentId);
      if (siblings) siblings.push(node);
      else children.set(node.parentId, [node]);
    }
  }
  for (const node of model?.nodes ?? []) {
    depth.set(node.id, node.parentId ? (depth.get(node.parentId) ?? 0) + 1 : 0);
  }
  return { nodeById, children, depth };
}

export function isDescendant(index: MindmapModelIndex, ancestorId: string, candidateId: string): boolean {
  let current = index.nodeById.get(candidateId);
  while (current?.parentId) {
    if (current.parentId === ancestorId) return true;
    current = index.nodeById.get(current.parentId);
  }
  return false;
}

/** Removes selected descendants when their ancestor is already selected. */
export function selectionRoots(selectedIds: readonly string[], index: MindmapModelIndex): string[] {
  const selected = new Set(selectedIds);
  return selectedIds.filter((id) => {
    let current = index.nodeById.get(id);
    while (current?.parentId) {
      if (selected.has(current.parentId)) return false;
      current = index.nodeById.get(current.parentId);
    }
    return true;
  });
}

export function createClipboardPayload(
  model: MindmapModel,
  selectedIds: readonly string[],
  index = buildMindmapIndex(model),
): MindmapClipboardPayload | null {
  const roots = selectionRoots(selectedIds, index);
  if (roots.length === 0) return null;
  const included = new Set<string>();
  const visit = (id: string) => {
    if (included.has(id)) return;
    included.add(id);
    for (const child of index.children.get(id) ?? []) visit(child.id);
  };
  roots.forEach(visit);
  return {
    type: "open-office/mindmap-fragment",
    version: 2,
    roots,
    nodes: model.nodes.filter((node) => included.has(node.id)),
    edges: model.edges
      .filter((edge) => included.has(edge.sourceId) && included.has(edge.targetId)),
    summaries: model.summaries.filter((summary) => included.has(summary.startNodeId) && included.has(summary.endNodeId)),
    boundaries: model.boundaries.filter((boundary) => included.has(boundary.rootNodeId)),
    formulas: model.formulas.filter((formula) => included.has(formula.nodeId)),
  };
}

export function parseClipboardPayload(text: string): MindmapClipboardPayload | null {
  try {
    const value = JSON.parse(text) as Partial<Omit<MindmapClipboardPayload, "version">> & { version?: number };
    if (value.type !== "open-office/mindmap-fragment" || (value.version !== 1 && value.version !== 2)) return null;
    if (!Array.isArray(value.roots) || !Array.isArray(value.nodes) || !Array.isArray(value.edges)) return null;
    if (value.roots.some((id) => typeof id !== "string")) return null;
    return {
      ...value,
      version: 2,
      summaries: value.version === 2 && Array.isArray(value.summaries) ? value.summaries : [],
      boundaries: value.version === 2 && Array.isArray(value.boundaries) ? value.boundaries : [],
      formulas: value.version === 2 && Array.isArray(value.formulas) ? value.formulas : [],
    } as MindmapClipboardPayload;
  } catch {
    return null;
  }
}

export function buildPasteCommands(
  payload: MindmapClipboardPayload,
  parentId: string | null,
  insertionIndex: number,
  createNodeId: () => string,
  createEntityId: () => string,
): { commands: MindmapCommandInput[]; rootIds: string[] } {
  const sourceById = new Map(payload.nodes.map((node) => [node.id, node]));
  const idMap = new Map(payload.nodes.map((node) => [node.id, createNodeId()]));
  const roots = payload.roots.filter((id) => sourceById.has(id));
  const rootOrder = new Map(roots.map((id, index) => [id, index]));
  const siblingOrder = new Map<string, number>();
  for (const node of payload.nodes) {
    if (!node.parentId) continue;
    const key = node.parentId;
    const next = siblingOrder.get(key) ?? 0;
    siblingOrder.set(`${key}\u001f${node.id}`, next);
    siblingOrder.set(key, next + 1);
  }
  const commands: MindmapCommandInput[] = [];
  for (const node of payload.nodes) {
    const mappedId = idMap.get(node.id);
    if (!mappedId) continue;
    const isRoot = rootOrder.has(node.id);
    const mappedParent = isRoot
      ? parentId
      : node.parentId
        ? idMap.get(node.parentId) ?? parentId
        : parentId;
    const index = isRoot
      ? insertionIndex + (rootOrder.get(node.id) ?? 0)
      : siblingOrder.get(`${node.parentId}\u001f${node.id}`) ?? 0;
    commands.push({
      typeId: "mindmap.addNode",
      payload: {
        type: "addNode",
        nodeId: mappedId,
        parentId: mappedParent,
        content: node.content,
        attrs: node.attrs,
        index,
      },
    });
    commands.push({
      typeId: "mindmap.setNodeStyle",
      payload: { type: "setNodeStyle", nodeId: mappedId, style: node.style },
    });
    commands.push({
      typeId: "mindmap.setNodeSupplement",
      payload: { type: "setNodeSupplement", nodeId: mappedId, supplement: node.supplement },
    });
    if (node.collapsed) {
      commands.push({
        typeId: "mindmap.setNodeCollapsed",
        payload: { type: "setNodeCollapsed", nodeId: mappedId, collapsed: true },
      });
    }
  }
  for (const edge of payload.edges) {
    const sourceId = idMap.get(edge.sourceId);
    const targetId = idMap.get(edge.targetId);
    if (!sourceId || !targetId) continue;
    commands.push({
      typeId: "mindmap.addEdge",
      payload: {
        type: "addEdge",
        edge: { ...edge, id: createEntityId(), sourceId, targetId },
      },
    });
  }
  for (const summary of payload.summaries) {
    const startNodeId = idMap.get(summary.startNodeId);
    const endNodeId = idMap.get(summary.endNodeId);
    if (!startNodeId || !endNodeId) continue;
    commands.push({ typeId: "mindmap.addSummary", payload: { type: "addSummary", summary: { ...summary, id: createEntityId(), startNodeId, endNodeId } } });
  }
  for (const boundary of payload.boundaries) {
    const rootNodeId = idMap.get(boundary.rootNodeId);
    if (!rootNodeId) continue;
    commands.push({ typeId: "mindmap.addBoundary", payload: { type: "addBoundary", boundary: { ...boundary, id: createEntityId(), rootNodeId } } });
  }
  for (const formula of payload.formulas) {
    const nodeId = idMap.get(formula.nodeId);
    if (!nodeId) continue;
    commands.push({ typeId: "mindmap.addFormula", payload: { type: "addFormula", formula: { ...formula, id: createEntityId(), nodeId } } });
  }
  return { commands, rootIds: roots.map((id) => idMap.get(id)).filter((id): id is string => Boolean(id)) };
}

/** Rebase existing formatting over a plain-text edit, using Unicode scalar offsets. */
export function renameMindmapContent(content: MindmapNode["content"], text: string): NonNullable<MindmapNode["content"]> {
  if (!content?.runs.length) return { text, runs: [] };
  const before = [...content.text];
  const after = [...text];
  let start = 0;
  while (start < before.length && start < after.length && before[start] === after[start]) start++;
  let end = before.length;
  let newEnd = after.length;
  while (end > start && newEnd > start && before[end - 1] === after[newEnd - 1]) { end--; newEnd--; }
  const runs: typeof content.runs = [];
  const append = (from: number, to: number, style: (typeof content.runs)[number]["style"]) => {
    if (to <= from) return;
    const previous = runs.at(-1);
    if (previous?.end === from && JSON.stringify(previous.style) === JSON.stringify(style)) previous.end = to;
    else runs.push({ start: from, end: to, style });
  };
  for (const run of content.runs) append(run.start, Math.min(run.end, start), run.style);
  const inheritAt = Math.min(start, Math.max(0, before.length - 1));
  const inherited = content.runs.find((run) => run.start <= inheritAt && run.end > inheritAt);
  if (inherited) append(start, newEnd, inherited.style);
  for (const run of content.runs) append(Math.max(run.start, end) + newEnd - end, run.end + newEnd - end, run.style);
  return { text, runs };
}
