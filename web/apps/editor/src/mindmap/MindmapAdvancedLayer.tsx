import type { MindmapProjection } from "@open-office/schema/api";
import type { MindmapModel, RichText } from "@open-office/schema/artifact";

export type MindmapAdvancedSelection = { type: "summary" | "boundary" | "formula"; id: string };

export function MindmapAdvancedLayer({ model, projection, selected, onSelect }: {
  model: MindmapModel;
  projection: MindmapProjection["advanced"];
  selected: MindmapAdvancedSelection | null;
  onSelect: (selection: MindmapAdvancedSelection) => void;
}) {
  const summaries = new Map(model.summaries.map((item) => [item.id, item]));
  const boundaries = new Map(model.boundaries.map((item) => [item.id, item]));
  const formulas = new Map(model.formulas.map((item) => [item.id, item]));
  const activate = (selection: MindmapAdvancedSelection) => (event: React.MouseEvent | React.KeyboardEvent) => {
    if ("key" in event && event.key !== "Enter" && event.key !== " ") return;
    event.preventDefault();
    event.stopPropagation();
    onSelect(selection);
  };
  return <g className="mindmap-advanced-layer" aria-label="脑图高级结构">
    {projection.boundaries.map((item) => {
      const boundary = boundaries.get(item.boundaryId);
      if (!boundary) return null;
      const selection = { type: "boundary", id: item.boundaryId } as const;
      return <g key={item.boundaryId} className={selected?.type === "boundary" && selected.id === item.boundaryId ? "is-selected" : ""}>
        <rect className="mindmap-boundary" {...item.rect} rx="12" role="button" tabIndex={0} aria-label={`外框：${boundary.label?.text || boundary.rootNodeId}`} onClick={activate(selection)} onKeyDown={activate(selection)} />
        {boundary.label?.text && <text className="mindmap-boundary__label" x={item.labelAnchor.x} y={item.labelAnchor.y}>{boundary.label.text}</text>}
      </g>;
    })}
    {projection.summaries.map((item) => {
      const summary = summaries.get(item.summaryId);
      if (!summary) return null;
      const selection = { type: "summary", id: item.summaryId } as const;
      const points = item.points.map((point) => `${point.x},${point.y}`).join(" ");
      return <g key={item.summaryId} className={selected?.type === "summary" && selected.id === item.summaryId ? "is-selected" : ""}>
        <polyline className="mindmap-summary" points={points} />
        <polyline className="mindmap-advanced-hit" points={points} role="button" tabIndex={0} aria-label={`概要：${summary.content.text}`} onClick={activate(selection)} onKeyDown={activate(selection)} />
        <text className="mindmap-summary__label" x={item.labelAnchor.x} y={item.labelAnchor.y}>{summary.content.text}</text>
      </g>;
    })}
    {projection.formulas.map((item) => {
      const formula = formulas.get(item.formulaId);
      if (!formula) return null;
      const selection = { type: "formula", id: item.formulaId } as const;
      return <text key={item.formulaId} className={`mindmap-formula ${selected?.type === "formula" && selected.id === item.formulaId ? "is-selected" : ""}`} x={item.anchor.x} y={item.anchor.y} textAnchor="middle" role="button" tabIndex={0} aria-label={`公式：${formula.source}`} onClick={activate(selection)} onKeyDown={activate(selection)}>{formula.display === "block" ? `$$${formula.source}$$` : `$${formula.source}$`}</text>;
    })}
  </g>;
}

export function MindmapAdvancedInspector({ model, selection, capabilities, disabled, onUpdate, onDelete }: {
  model: MindmapModel;
  selection: MindmapAdvancedSelection;
  capabilities: ReadonlySet<string>;
  disabled: boolean;
  onUpdate: (patch: Record<string, unknown>) => void;
  onDelete: () => void;
}) {
  const nodes = model.nodes;
  if (selection.type === "summary") {
    const item = model.summaries.find((value) => value.id === selection.id);
    if (!item) return null;
    const canUpdate = !disabled && capabilities.has("mindmap.updateSummary");
    return <AdvancedAside title="所选概要" deleteDisabled={disabled || !capabilities.has("mindmap.deleteSummary")} onDelete={onDelete}>
      <label>起点<select aria-label="概要起点" disabled={!canUpdate} value={item.startNodeId} onChange={(event) => onUpdate({ startNodeId: event.target.value })}>{nodes.map(nodeOption)}</select></label>
      <label>终点<select aria-label="概要终点" disabled={!canUpdate} value={item.endNodeId} onChange={(event) => onUpdate({ endNodeId: event.target.value })}>{nodes.map(nodeOption)}</select></label>
      <label className="mindmap-inspector__stack">标签<input aria-label="概要标签" disabled={!canUpdate} key={`${item.id}:content`} defaultValue={item.content.text} onBlur={(event) => onUpdate({ content: richText(event.currentTarget.value.trim() || "概要") })} /></label>
    </AdvancedAside>;
  }
  if (selection.type === "boundary") {
    const item = model.boundaries.find((value) => value.id === selection.id);
    if (!item) return null;
    const canUpdate = !disabled && capabilities.has("mindmap.updateBoundary");
    return <AdvancedAside title="所选外框" deleteDisabled={disabled || !capabilities.has("mindmap.deleteBoundary")} onDelete={onDelete}>
      <label>子树根<select aria-label="外框子树根" disabled={!canUpdate} value={item.rootNodeId} onChange={(event) => onUpdate({ rootNodeId: event.target.value })}>{nodes.map(nodeOption)}</select></label>
      <label className="mindmap-inspector__stack">标签<input aria-label="外框标签" disabled={!canUpdate} key={`${item.id}:label`} defaultValue={item.label?.text ?? ""} onBlur={(event) => onUpdate({ label: event.currentTarget.value.trim() ? richText(event.currentTarget.value) : null })} /></label>
    </AdvancedAside>;
  }
  const item = model.formulas.find((value) => value.id === selection.id);
  if (!item) return null;
  const canUpdate = !disabled && capabilities.has("mindmap.updateFormula");
  return <AdvancedAside title="所选公式" deleteDisabled={disabled || !capabilities.has("mindmap.deleteFormula")} onDelete={onDelete}>
    <label>所属主题<select aria-label="公式所属主题" disabled={!canUpdate} value={item.nodeId} onChange={(event) => onUpdate({ nodeId: event.target.value })}>{nodes.map(nodeOption)}</select></label>
    <label className="mindmap-inspector__stack">LaTeX<input aria-label="公式 LaTeX" disabled={!canUpdate} key={`${item.id}:source`} defaultValue={item.source} onBlur={(event) => { if (event.currentTarget.value.trim()) onUpdate({ source: event.currentTarget.value }); }} /></label>
    <label>显示<select aria-label="公式显示方式" disabled={!canUpdate} value={item.display} onChange={(event) => onUpdate({ display: event.target.value })}><option value="inline">行内</option><option value="block">块级</option></select></label>
  </AdvancedAside>;
}

function AdvancedAside({ title, deleteDisabled, onDelete, children }: { title: string; deleteDisabled: boolean; onDelete: () => void; children: React.ReactNode }) {
  return <aside className="mindmap-inspector"><h2>{title}</h2>{children}<button className="is-danger" disabled={deleteDisabled} onClick={onDelete}>删除{title.slice(2)}</button><p className="mindmap-inspector__hint">可用 Enter 选择、Delete 删除。</p></aside>;
}

function nodeOption(node: MindmapModel["nodes"][number]) {
  return <option key={node.id} value={node.id}>{node.content?.text || "未命名主题"}</option>;
}

function richText(text: string): RichText { return { text, runs: [] }; }
