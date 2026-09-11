import { useEffect, useState } from "react";
import { Button, Checkbox, Icon, IconButton, Input, Select, Textarea } from "@open-office/ui";
import type { BuiltinPresentationNodeAction, ResolvedPresentationNodeUi } from "@open-office/presentation-ui";
import type {
  PresentationV5ChartSpec,
  PresentationV5Node,
  PresentationV5NodeKind,
  PresentationV5RichText,
  PresentationV5TimelineEntry,
} from "@open-office/schema";
import { presentationParagraphsForText } from "@open-office/schema";
import type { PresentationSlideProjection } from "@open-office/schema/api";

import { FontPicker } from "../typography/FontPicker.js";
import { ColorPickerField } from "./PresentationDeckInspectors.js";
import { ConnectorInspector, ImageInspector, TableInspector, TransformInspector } from "./PresentationSpecializedInspectors.js";
import {
  colorInputValue,
  colorRefInputValue,
  positiveNumberOrNull,
  solidColor,
  solidPaint,
  textStyleForInspector,
} from "./presentationInspectorValues.js";
import type { PresentationTableNode, TableSelection } from "./presentationTableSelection.js";
import { presentationErrorMessage } from "./usePresentationSession.js";

type PresentationTextNode = PresentationV5Node & { kind: Extract<PresentationV5NodeKind, { type: "text" }> };
type PresentationShapeNode = PresentationV5Node & { kind: Extract<PresentationV5NodeKind, { type: "shape" }> };
type PresentationImageNode = PresentationV5Node & { kind: Extract<PresentationV5NodeKind, { type: "image" }> };
type PresentationChartNode = PresentationV5Node & { kind: Extract<PresentationV5NodeKind, { type: "chart" }> };
type PresentationConnectorNode = PresentationV5Node & { kind: Extract<PresentationV5NodeKind, { type: "connector" }> };

export function NodeInspector({
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

export function NodeAnimationInspector({
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


export function TextInspector({ node, disabled, canEditContent, canEditFrame, onAction }: {
  node: PresentationTextNode;
  disabled: boolean;
  canEditContent: boolean;
  canEditFrame: boolean;
  onAction: (action: BuiltinPresentationNodeAction, value?: unknown) => void;
}) {
  const [text, setText] = useState(node.kind.data.frame.body.text);
  const [verticalAlign, setVerticalAlign] = useState(node.kind.data.frame.verticalAlign);
  const [autoFit, setAutoFit] = useState(node.kind.data.frame.autoFit);
  const [paragraphAlignment, setParagraphAlignment] = useState(node.kind.data.frame.body.paragraphs[0]?.alignment ?? "left");
  const [listType, setListType] = useState<"none" | "bullet" | "ordered">(node.kind.data.frame.body.paragraphs[0]?.list?.type ?? "none");
  const [indentLevel, setIndentLevel] = useState(node.kind.data.frame.body.paragraphs[0]?.indentLevel ?? 0);
  const [style, setStyle] = useState(() => textStyleForInspector(node.kind.data.frame.body));
  useEffect(() => {
    setText(node.kind.data.frame.body.text);
    setVerticalAlign(node.kind.data.frame.verticalAlign);
    setAutoFit(node.kind.data.frame.autoFit);
    setParagraphAlignment(node.kind.data.frame.body.paragraphs[0]?.alignment ?? "left");
    setListType(node.kind.data.frame.body.paragraphs[0]?.list?.type ?? "none");
    setIndentLevel(node.kind.data.frame.body.paragraphs[0]?.indentLevel ?? 0);
    setStyle(textStyleForInspector(node.kind.data.frame.body));
  }, [node.id, node.kind.data]);
  const styledBody = (): PresentationV5RichText => ({
    text,
    // The inspector is a whole-text formatter.  It deliberately writes one
    // complete run instead of a partial range or an `attrs` patch, so the
    // canonical schema can validate coverage independently of this UI.
    runs: text.length ? [{ start: 0, end: [...text].length, style }] : [],
    paragraphs: presentationParagraphsForText(text, node.kind.data.frame.body.paragraphs).map((paragraph) => ({
      ...paragraph,
      alignment: paragraphAlignment,
      list: listType === "none" ? null : listType === "bullet" ? { type: "bullet" } : { type: "ordered", startAt: 1 },
      indentLevel,
    })),
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
      <label>字体<FontPicker value={style.fontFamily ?? ""} disabled={disabled} onChange={fontFamily => setStyle(current => ({ ...current, fontFamily }))} /></label>
      <label>字号<Input type="number" min="1" max="512" value={style.fontSize ?? ""} onChange={(event) => setStyle((current) => ({ ...current, fontSize: positiveNumberOrNull(event.target.value) }))} /></label>
      <ColorPickerField label="文字颜色" role="text" value={colorRefInputValue(style.color)} disabled={disabled} onValueChange={(value) => setStyle((current) => ({ ...current, color: solidColor(value ?? "#000000") }))} />
    </fieldset> : null}
    {canEditContent ? <Button type="button" size="sm" disabled={disabled} onClick={() => onAction("text.content", styledBody())}>应用文字与样式</Button> : null}
    {canEditContent ? <div className="presentation-studio__transform-grid">
      <label>段落对齐<Select value={paragraphAlignment} disabled={disabled} onChange={(event) => setParagraphAlignment(event.target.value as typeof paragraphAlignment)}><option value="left">左对齐</option><option value="center">居中</option><option value="right">右对齐</option><option value="justify">两端对齐</option></Select></label>
      <label>列表<Select value={listType} disabled={disabled} onChange={(event) => setListType(event.target.value as typeof listType)}><option value="none">无</option><option value="bullet">项目符号</option><option value="ordered">编号</option></Select></label>
      <label>缩进层级<Input type="number" min="0" max="8" value={indentLevel} disabled={disabled} onChange={(event) => setIndentLevel(Math.max(0, Math.min(8, Number(event.target.value) || 0)))} /></label>
    </div> : null}
    {canEditFrame ? <label>垂直对齐<Select value={verticalAlign} disabled={disabled} onChange={(event) => setVerticalAlign(event.target.value as typeof verticalAlign)}><option value="top">顶端</option><option value="middle">居中</option><option value="bottom">底端</option></Select></label> : null}
    {canEditFrame ? <label>自动适应<Select value={autoFit} disabled={disabled} onChange={(event) => setAutoFit(event.target.value as typeof autoFit)}><option value="none">不自动适应</option><option value="shrinkText">缩小文字</option><option value="resizeShape">调整形状</option></Select></label> : null}
    {canEditFrame ? <Button type="button" size="sm" disabled={disabled} onClick={() => onAction("text.frame", { ...node.kind.data.frame, verticalAlign, autoFit })}>应用文本框</Button> : null}
  </section>;
}

export function ShapeInspector({ node, disabled, canEditStyle, canEditGeometry, onAction }: {
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
export function ChartInspector({ node, disabled, canEdit, onAction }: {
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
      setDraftError(presentationErrorMessage(reason));
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

export function chartSeriesDraft(spec: PresentationV5ChartSpec) {
  return spec.series.map((series) => `${series.name}: ${series.values.join(", ")}`).join("\n");
}

export function parseChartInspectorDraft(input: {
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
