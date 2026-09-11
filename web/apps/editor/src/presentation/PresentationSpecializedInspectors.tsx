import { useEffect, useState } from "react";
import { Button, Checkbox, Input, Select, Textarea } from "@open-office/ui";
import type { BuiltinPresentationNodeAction } from "@open-office/presentation-ui";
import type { ConnectorEndpoint, PresentationV5Node, PresentationV5NodeKind, PresentationV5Transform } from "@open-office/schema";

import { api } from "../api.js";
import { FontPicker } from "../typography/FontPicker.js";
import { preservePresentationText, withPresentationFont } from "./PresentationRichText.js";
import { ColorPickerField } from "./PresentationDeckInspectors.js";
import { endpointPoint } from "./presentationGeometry.js";
import { colorInputValue, isUsableTransform, solidPaint } from "./presentationInspectorValues.js";
import {
  tableAnchorAt,
  tableAnchorsInSelection,
  tableRange,
  tableSelectionCanMerge,
  type PresentationTableNode,
  type TableSelection,
} from "./presentationTableSelection.js";
import { MIN_PRESENTATION_NODE_SIZE } from "./interactions.js";

type PresentationImageNode = PresentationV5Node & { kind: Extract<PresentationV5NodeKind, { type: "image" }> };
type PresentationConnectorNode = PresentationV5Node & { kind: Extract<PresentationV5NodeKind, { type: "connector" }> };

export function ConnectorInspector({ node, nodes, disabled, canEdit, onAction }: {
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

export function TableInspector({ node, selection, disabled, availableCapabilities, canEditContent, canEditStyle, onSelectionChange, onAction }: {
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
  const [draftFont, setDraftFont] = useState<string | null>(null);
  const [fill, setFill] = useState(colorInputValue(selected?.style.fill ?? { type: "none" }));
  const [fillEnabled, setFillEnabled] = useState(selected?.style.fill.type === "solid");
  const [horizontalAlign, setHorizontalAlign] = useState(selected?.style.horizontalAlign ?? "left");
  const [verticalAlign, setVerticalAlign] = useState(selected?.style.verticalAlign ?? "middle");
  useEffect(() => {
    const current = resolvedSelection ? tableAnchorAt(node, resolvedSelection.focus) : anchors[0];
    if (!current) return;
    setText(current.content.text);
    setDraftFont(null);
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
    {canEditContent && <label>字体<FontPicker value={draftFont ?? selected.content.runs[0]?.style.fontFamily ?? ""} disabled={disabled || selectedAnchors.length !== 1 || !text} onChange={setDraftFont} /></label>}
    {canEditContent && <Button type="button" size="sm" disabled={disabled || selectedAnchors.length !== 1} onClick={() => {
      const content = preservePresentationText(selected.content, text);
      onAction("table.cellContent", { ...address, content: draftFont ? withPresentationFont(content, draftFont) : content });
    }}>应用内容</Button>}
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

export function ImageInspector({ node, disabled, canEditImage, artifactId, onAction }: {
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

export function TransformInspector({ node, disabled, onApply }: { node: PresentationV5Node; disabled: boolean; onApply: (transform: PresentationV5Transform) => void }) {
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
