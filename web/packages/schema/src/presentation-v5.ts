/**
 * Strict browser mirror of the target Presentation v5 schema.
 *
 * This target contract deliberately has no `attrs`, generic node payload or v4 fallback. It is
 * not wired into `parseSnapshot` until the Rust envelope and all consumers cut over together.
 */

export type ThemeColorToken = "background" | "text" | "accent1" | "accent2" | "accent3" | "accent4" | "accent5" | "accent6" | "hyperlink" | "followedHyperlink";
export type PageUnit = "emu" | "point";
export type PlaceholderKind = "title" | "centeredTitle" | "subtitle" | "body" | "picture" | "table" | "chart" | "object";
export type Rgba = { r: number; g: number; b: number; a: number };
export type ColorRef = { type: "theme"; value: ThemeColorToken } | { type: "rgba"; value: Rgba };
export type Paint = { type: "none" } | { type: "solid"; value: ColorRef };
export type PresentationV5SlideBackground = { type: "none" } | { type: "solid"; value: ColorRef };
export interface PresentationV5SlideTransition { kind: "none" | "fade" | "push" | "wipe"; durationMs: number }
export interface PresentationV5RichText { text: string; runs: PresentationV5TextRun[]; paragraphs: PresentationV5Paragraph[] }
export interface PresentationV5Paragraph { start: number; end: number; alignment: "left" | "center" | "right" | "justify"; list: { type: "bullet" } | { type: "ordered"; startAt: number } | null; indentLevel: number }
export function presentationParagraphsForText(text: string, previous: readonly PresentationV5Paragraph[] = []): PresentationV5Paragraph[] { const characters = [...text]; const ranges: { start: number; end: number }[] = []; let start = 0; characters.forEach((character, index) => { if (character === "\n") { ranges.push({ start, end: index + 1 }); start = index + 1; } }); if (start < characters.length) ranges.push({ start, end: characters.length }); return ranges.map((range, index) => ({ ...range, alignment: previous[index]?.alignment ?? "left", list: previous[index]?.list ?? null, indentLevel: previous[index]?.indentLevel ?? 0 })); }
export function plainPresentationRichText(text: string): PresentationV5RichText { return { text, runs: [], paragraphs: presentationParagraphsForText(text) }; }
export interface PresentationV5TextRun { start: number; end: number; style: PresentationV5TextStyle }
export interface PresentationV5TextStyle { bold: boolean; italic: boolean; underline: boolean; strikethrough: boolean; fontFamily: string | null; fontSize: number | null; color: ColorRef | null }
export interface PresentationV5Transform { x: number; y: number; width: number; height: number; rotation: number }
export interface PresentationV5Asset { assetId: string; digest: string; mimeType: string; width: number | null; height: number | null; originalAssetId: string | null }
export type PresentationV5ChartType = "column" | "bar" | "line" | "pie";
export interface PresentationV5ChartSeries { name: string; values: number[]; color: ColorRef | null }
export interface PresentationV5ChartSpec { chartType: PresentationV5ChartType; title: string | null; categories: string[]; series: PresentationV5ChartSeries[] }
/**
 * Extension payloads are intentionally opaque to the host, but never
 * unstructured. `typeId` and an object-shaped `data` form the stable hand-off
 * between a persisted node and a registered, read-only extension renderer.
 */
export interface PresentationV5ExtensionPayload { typeId: string; data: Record<string, unknown> }
export interface PresentationV5Node { id: string; parentId: string | null; orderKey: string; name: string | null; altText: string | null; layoutPlaceholderId: string | null; transform: PresentationV5Transform; visible: boolean; locked: boolean; opacity: number; kind: PresentationV5NodeKind }
export type PresentationV5NodeKind =
  | { type: "shape"; data: { geometry: "rectangle" | "ellipse" | "line" | "arrow"; style: { fill: Paint; stroke: { color: ColorRef; width: number } | null } } }
  | { type: "text"; data: { frame: { body: PresentationV5RichText; verticalAlign: "top" | "middle" | "bottom"; padding: Insets; autoFit: "none" | "shrinkText" | "resizeShape" } } }
  | { type: "image"; data: { assetId: string; originalAssetId: string | null; crop: PresentationV5ImageCrop; flipH: boolean; flipV: boolean; caption: string | null } }
  | { type: "video" | "audio"; data: { assetId: string; posterAssetId: string | null } }
  | { type: "table"; data: { rows: number; columns: number; cells: PresentationV5TableCell[] } }
  | { type: "chart"; data: { spec: PresentationV5ChartSpec } }
  | { type: "connector"; data: { start: ConnectorEndpoint; end: ConnectorEndpoint } }
  | { type: "group"; data: Record<string, never> }
  | { type: "embed"; data: { source: string; posterAssetId: string | null } }
  | { type: "extension"; data: { namespace: string; version: string; typeId: string; data: Record<string, unknown> } };
export interface PresentationV5TableCell { row: number; column: number; rowSpan: number; columnSpan: number; content: PresentationV5RichText; style: { fill: Paint; horizontalAlign: "left" | "center" | "right"; verticalAlign: "top" | "middle" | "bottom" } }
export type ConnectorEndpoint = { type: "free"; value: { x: number; y: number } } | { type: "node"; value: { nodeId: string; anchor: "top" | "right" | "bottom" | "left" | "center" } };
export interface Insets { top: number; right: number; bottom: number; left: number }
export interface PresentationV5ImageCrop { top: number; right: number; bottom: number; left: number }
export interface PresentationV5Placeholder { id: string; kind: PlaceholderKind; transform: PresentationV5Transform; defaultText: PresentationV5RichText | null; masterPlaceholderId?: string | null }
export interface PresentationV5Master { id: string; name: string; background: PresentationV5SlideBackground; placeholders: PresentationV5Placeholder[] }
export interface PresentationV5Layout { id: string; masterId: string; name: string; placeholders: PresentationV5Placeholder[] }
export interface PresentationV5TimelineEntry { id: string; targetNodeId: string; trigger: "onClick" | "withPrevious" | "afterPrevious"; preset: "appear" | "fade" | "flyIn" | "wipe"; durationMs: number; delayMs: number; orderKey: string }
export interface PresentationV5Slide { id: string; orderKey: string; name: string; layoutId: string | null; background: PresentationV5SlideBackground; notes: string | null; transition: PresentationV5SlideTransition | null; nodes: PresentationV5Node[]; timeline: { entries: PresentationV5TimelineEntry[] } }
export interface PresentationV5Deck { pageSpec: { width: number; height: number; unit: PageUnit; safeArea: Insets | null }; slides: PresentationV5Slide[]; masters: PresentationV5Master[]; layouts: PresentationV5Layout[]; theme: { id: string; name: string }; assets: PresentationV5Asset[] }

/**
 * Strictly parse a node returned by the read-only Presentation projection
 * surface.  Projection callers provide the canonical asset and same-slide
 * node ids carried alongside the node, so this parser still rejects dangling
 * media, parent and connector references without downloading a whole Deck.
 */
export function parsePresentationV5ProjectedNode(
  value: unknown,
  context: { assetIds: Iterable<string>; slideNodeIds: Iterable<string> },
): PresentationV5Node {
  const assetIds = new Set(context.assetIds);
  const nodeIds = new Set(context.slideNodeIds);
  const node = parseNode(value, "presentation projection node", assetIds);
  if (node.parentId && !nodeIds.has(node.parentId)) throw new Error(`node 引用不存在 parent：${node.parentId}`);
  if (node.kind.type === "connector") {
    for (const endpoint of [node.kind.data.start, node.kind.data.end]) {
      if (endpoint.type === "node" && !nodeIds.has(endpoint.value.nodeId)) {
        throw new Error(`connector 引用不存在 node：${endpoint.value.nodeId}`);
      }
    }
  }
  return node;
}

const nodeTypes = ["shape", "text", "image", "video", "audio", "table", "chart", "connector", "group", "embed", "extension"] as const;
const placeholderKinds = ["title", "centeredTitle", "subtitle", "body", "picture", "table", "chart", "object"] as const;

export function parsePresentationV5Deck(value: unknown): PresentationV5Deck {
  const deck = object(value, "presentation v5");
  exact(deck, ["pageSpec", "slides", "masters", "layouts", "theme", "assets"], "presentation v5");
  const page = object(deck.pageSpec, "pageSpec"); exact(page, ["width", "height", "unit", "safeArea"], "pageSpec");
  const width = positive(page.width, "pageSpec.width"); const height = positive(page.height, "pageSpec.height");
  const unit = enumValue(string(page.unit, "pageSpec.unit"), ["emu", "point"] as const, "pageSpec.unit");
  const safeArea = nullable(page.safeArea, "pageSpec.safeArea", parseInsets);
  const assets = array(deck.assets, "assets").map((raw, index) => parseAsset(raw, index)); unique(assets.map((item) => item.assetId), "assetId");
  const assetIds = new Set(assets.map((item) => item.assetId));
  const masters = array(deck.masters, "masters").map((raw, index) => parseMaster(raw, index)); unique(masters.map((item) => item.id), "master.id");
  const mastersById = new Map(masters.map((item) => [item.id, item]));
  const layouts = array(deck.layouts, "layouts").map((raw, index) => parseLayout(raw, index, mastersById)); unique(layouts.map((item) => item.id), "layout.id");
  const layoutsById = new Map(layouts.map((item) => [item.id, item]));
  const slides = array(deck.slides, "slides").map((raw, index) => parseSlide(raw, index, layoutsById, assetIds)); unique(slides.map((item) => item.id), "slide.id");
  const theme = object(deck.theme, "theme"); exact(theme, ["id", "name", "colors", "fonts"], "theme");
  return { pageSpec: { width, height, unit, safeArea }, slides, masters, layouts, theme: { id: id(theme.id, "theme.id"), name: optionalString(theme.name, "theme.name", "") }, assets };
}

function parseAsset(raw: unknown, index: number): PresentationV5Asset {
  const item = object(raw, `assets[${index}]`); exact(item, ["assetId", "digest", "mimeType", "width", "height", "originalAssetId"], `assets[${index}]`);
  return { assetId: id(item.assetId, `assets[${index}].assetId`), digest: id(item.digest, `assets[${index}].digest`), mimeType: id(item.mimeType, `assets[${index}].mimeType`), width: nullable(item.width, `assets[${index}].width`, (v, n) => integer(v, n, 1)), height: nullable(item.height, `assets[${index}].height`, (v, n) => integer(v, n, 1)), originalAssetId: nullable(item.originalAssetId, `assets[${index}].originalAssetId`, id) };
}

function parseMaster(raw: unknown, index: number): PresentationV5Master {
  const master = object(raw, `masters[${index}]`); exact(master, ["id", "name", "background", "placeholders"], `masters[${index}]`);
  const placeholders = arrayOrDefault(master.placeholders, `masters[${index}].placeholders`).map((value, i) => parseMasterPlaceholder(value, `${index}:${i}`)); unique(placeholders.map((item) => item.id), "master placeholder.id");
  return { id: id(master.id, `masters[${index}].id`), name: string(master.name, `masters[${index}].name`), background: master.background === undefined ? { type: "none" } : parseSlideBackground(master.background, `masters[${index}].background`), placeholders };
}

/** Strict parser for a master returned by the read-only Deck projection. */
export function parsePresentationV5Master(value: unknown): PresentationV5Master {
  return parseMaster(value, 0);
}
function parseLayout(raw: unknown, index: number, masters: Map<string, PresentationV5Master>): PresentationV5Layout {
  const layout = object(raw, `layouts[${index}]`); exact(layout, ["id", "masterId", "name", "placeholders"], `layouts[${index}]`);
  const masterId = id(layout.masterId, `layouts[${index}].masterId`); const master = masters.get(masterId); if (!master) throw new Error(`layout 引用不存在 master：${masterId}`);
  const placeholders = arrayOrDefault(layout.placeholders, `layouts[${index}].placeholders`).map((value, i) => parseLayoutPlaceholder(value, `${index}:${i}`, master)); unique(placeholders.map((item) => item.id), "layout placeholder.id");
  return { id: id(layout.id, `layouts[${index}].id`), masterId, name: string(layout.name, `layouts[${index}].name`), placeholders };
}

/** Strict parser for a layout returned by the read-only Deck projection. */
export function parsePresentationV5Layout(
  value: unknown,
  masters: Iterable<PresentationV5Master>,
): PresentationV5Layout {
  return parseLayout(value, 0, new Map([...masters].map((master) => [master.id, master])));
}
function parseMasterPlaceholder(raw: unknown, name: string): PresentationV5Placeholder {
  const value = object(raw, `masterPlaceholder[${name}]`); exact(value, ["id", "kind", "transform", "defaultText"], `masterPlaceholder[${name}]`);
  return { id: id(value.id, `masterPlaceholder[${name}].id`), kind: enumValue(string(value.kind, "master placeholder kind"), placeholderKinds, "master placeholder kind"), transform: parseTransform(value.transform, "master placeholder transform"), defaultText: nullable(value.defaultText, "master placeholder defaultText", parseRichText) };
}
function parseLayoutPlaceholder(raw: unknown, name: string, master: PresentationV5Master): PresentationV5Placeholder {
  const value = object(raw, `layoutPlaceholder[${name}]`); exact(value, ["id", "kind", "masterPlaceholderId", "transform", "defaultText"], `layoutPlaceholder[${name}]`);
  const masterPlaceholderId = nullable(value.masterPlaceholderId, "layout placeholder masterPlaceholderId", id); if (masterPlaceholderId && !master.placeholders.some((item) => item.id === masterPlaceholderId)) throw new Error(`layout placeholder 引用不存在 master placeholder：${masterPlaceholderId}`);
  return { id: id(value.id, `layoutPlaceholder[${name}].id`), kind: enumValue(string(value.kind, "layout placeholder kind"), placeholderKinds, "layout placeholder kind"), masterPlaceholderId, transform: parseTransform(value.transform, "layout placeholder transform"), defaultText: nullable(value.defaultText, "layout placeholder defaultText", parseRichText) };
}

function parseSlide(raw: unknown, index: number, layouts: Map<string, PresentationV5Layout>, assets: Set<string>): PresentationV5Slide {
  const slide = object(raw, `slides[${index}]`); exact(slide, ["id", "orderKey", "name", "layoutId", "background", "notes", "transition", "nodes", "timeline"], `slides[${index}]`);
  const layoutId = nullable(slide.layoutId, `slides[${index}].layoutId`, id); const layout = layoutId ? layouts.get(layoutId) : undefined; if (layoutId && !layout) throw new Error(`slide 引用不存在 layout：${layoutId}`);
  const nodes = array(slide.nodes, `slides[${index}].nodes`).map((node, i) => parseNode(node, `${index}:${i}`, assets)); unique(nodes.map((node) => node.id), "node.id");
  const nodeIds = new Set(nodes.map((node) => node.id)); const siblingKeys = new Set<string>();
  for (const node of nodes) {
    if (node.parentId && !nodeIds.has(node.parentId)) throw new Error(`node 引用不存在 parent：${node.parentId}`);
    if (node.layoutPlaceholderId && (!layout || !layout.placeholders.some((item) => item.id === node.layoutPlaceholderId))) throw new Error(`node 引用不存在 layout placeholder：${node.layoutPlaceholderId}`);
    if (node.kind.type === "connector") {
      for (const endpoint of [node.kind.data.start, node.kind.data.end]) {
        if (endpoint.type === "node" && !nodeIds.has(endpoint.value.nodeId)) throw new Error(`connector 引用不存在 node：${endpoint.value.nodeId}`);
      }
    }
    const siblingKey = `${node.parentId ?? "<root>"}\u0000${node.orderKey}`; if (siblingKeys.has(siblingKey)) throw new Error("node sibling orderKey 重复"); siblingKeys.add(siblingKey);
  }
  validateNodeCycles(nodes);
  const timeline = parseTimeline(slide.timeline, `slides[${index}].timeline`, nodeIds);
  return { id: id(slide.id, `slides[${index}].id`), orderKey: id(slide.orderKey, `slides[${index}].orderKey`), name: optionalString(slide.name, `slides[${index}].name`, ""), layoutId, background: slide.background === undefined ? { type: "none" } : parseSlideBackground(slide.background, `slides[${index}].background`), notes: nullable(slide.notes, `slides[${index}].notes`, string), transition: nullable(slide.transition, `slides[${index}].transition`, parseSlideTransition), nodes, timeline };
}
function parseNode(raw: unknown, name: string, assets: Set<string>): PresentationV5Node {
  const node = object(raw, `nodes[${name}]`); exact(node, ["id", "parentId", "orderKey", "name", "altText", "layoutPlaceholderId", "transform", "visible", "locked", "opacity", "kind"], `nodes[${name}]`);
  const idValue = id(node.id, `nodes[${name}].id`); const kind = parseKind(node.kind, name, idValue, assets);
  return { id: idValue, parentId: nullable(node.parentId, "node.parentId", id), orderKey: id(node.orderKey, "node.orderKey"), name: nullable(node.name, "node.name", string), altText: nullable(node.altText, "node.altText", string), layoutPlaceholderId: nullable(node.layoutPlaceholderId, "node.layoutPlaceholderId", id), transform: parseTransform(node.transform, "node.transform"), visible: optionalBoolean(node.visible, "node.visible", true), locked: optionalBoolean(node.locked, "node.locked", false), opacity: bounded(optionalNumber(node.opacity, "node.opacity", 1), 0, 1, "node.opacity"), kind };
}
function parseKind(raw: unknown, name: string, owner: string, assets: Set<string>): PresentationV5NodeKind {
  const kind = object(raw, `nodes[${name}].kind`); exact(kind, ["type", "data"], `nodes[${name}].kind`); const type = enumValue(string(kind.type, "node kind"), nodeTypes, "node kind"); const data = object(kind.data, "node kind data");
  switch (type) {
    case "shape": return { type, data: parseShape(data) };
    case "text": return { type, data: parseText(data) };
    case "image": return { type, data: parseImage(data, owner, assets) };
    case "video": case "audio": return { type, data: parseMedia(data, owner, assets) };
    case "table": return { type, data: parseTable(data, owner) };
    case "chart": return { type, data: { spec: parseChartSpec(data, owner) } };
    case "connector": return { type, data: parseConnector(data, owner) };
    case "group": exact(data, [], "group"); return { type, data: {} };
    case "embed": exact(data, ["source", "posterAssetId"], "embed"); { const posterAssetId = nullable(data.posterAssetId, "embed.posterAssetId", id); if (posterAssetId) requireAsset(owner, posterAssetId, assets); return { type, data: { source: id(data.source, "embed.source"), posterAssetId } }; }
    case "extension": exact(data, ["namespace", "version", "typeId", "data"], "extension"); return { type, data: { namespace: id(data.namespace, "extension.namespace"), version: id(data.version, "extension.version"), typeId: id(data.typeId, "extension.typeId"), data: object(data.data, "extension.data") } };
  }
}
function parseShape(value: Record<string, unknown>) { exact(value, ["geometry", "style"], "shape"); const style = object(value.style, "shape.style"); exact(style, ["fill", "stroke"], "shape.style"); const stroke = nullable(style.stroke, "shape.stroke", (raw, name) => { const item = object(raw, name); exact(item, ["color", "width"], name); return { color: parseColor(item.color, `${name}.color`), width: positive(item.width, `${name}.width`) }; }); return { geometry: enumValue(string(value.geometry, "shape.geometry"), ["rectangle", "ellipse", "line", "arrow"] as const, "shape.geometry"), style: { fill: parsePaint(style.fill, "shape.fill"), stroke } }; }
function parseText(value: Record<string, unknown>) { exact(value, ["frame"], "text"); const frame = object(value.frame, "text.frame"); exact(frame, ["body", "verticalAlign", "padding", "autoFit"], "text.frame"); return { frame: { body: parseRichText(frame.body, "text.frame.body"), verticalAlign: enumValue(optionalString(frame.verticalAlign, "text.frame.verticalAlign", "middle"), ["top", "middle", "bottom"] as const, "text.frame.verticalAlign"), padding: parseInsets(frame.padding, "text.frame.padding"), autoFit: enumValue(optionalString(frame.autoFit, "text.frame.autoFit", "none"), ["none", "shrinkText", "resizeShape"] as const, "text.frame.autoFit") } }; }
function parseImage(value: Record<string, unknown>, owner: string, assets: Set<string>) { exact(value, ["assetId", "originalAssetId", "crop", "flipH", "flipV", "caption"], "image"); const assetId = id(value.assetId, "image.assetId"); requireAsset(owner, assetId, assets); const originalAssetId = nullable(value.originalAssetId, "image.originalAssetId", id); if (originalAssetId) requireAsset(owner, originalAssetId, assets); return { assetId, originalAssetId, crop: parseCrop(value.crop, "image.crop"), flipH: optionalBoolean(value.flipH, "image.flipH", false), flipV: optionalBoolean(value.flipV, "image.flipV", false), caption: nullable(value.caption, "image.caption", string) }; }
function parseMedia(value: Record<string, unknown>, owner: string, assets: Set<string>) { exact(value, ["assetId", "posterAssetId"], "media"); const assetId = id(value.assetId, "media.assetId"); requireAsset(owner, assetId, assets); const posterAssetId = nullable(value.posterAssetId, "media.posterAssetId", id); if (posterAssetId) requireAsset(owner, posterAssetId, assets); return { assetId, posterAssetId }; }
function parseTable(value: Record<string, unknown>, owner: string) { exact(value, ["rows", "columns", "cells"], "table"); const rows = integer(value.rows, "table.rows", 1); const columns = integer(value.columns, "table.columns", 1); const occupied = new Set<string>(); const cells = arrayOrDefault(value.cells, "table.cells").map((raw, index) => { const cell = object(raw, `table.cells[${index}]`); exact(cell, ["row", "column", "rowSpan", "columnSpan", "content", "style"], `table.cells[${index}]`); const row = integer(cell.row, "table.cell.row", 0); const column = integer(cell.column, "table.cell.column", 0); const rowSpan = optionalInteger(cell.rowSpan, "table.cell.rowSpan", 1, 1); const columnSpan = optionalInteger(cell.columnSpan, "table.cell.columnSpan", 1, 1); if (row + rowSpan > rows || column + columnSpan > columns) throw new Error("table cell 范围无效"); for (let y = row; y < row + rowSpan; y += 1) for (let x = column; x < column + columnSpan; x += 1) { const key = `${y}:${x}`; if (occupied.has(key)) throw new Error("table cell 范围重叠"); occupied.add(key); } const style = object(cell.style, "table.cell.style"); exact(style, ["fill", "horizontalAlign", "verticalAlign"], "table.cell.style"); return { row, column, rowSpan, columnSpan, content: parseRichText(cell.content, `table.cells[${index}].content`), style: { fill: parsePaint(style.fill, "table.cell.fill"), horizontalAlign: enumValue(optionalString(style.horizontalAlign, "table.cell.horizontalAlign", "left"), ["left", "center", "right"] as const, "table.cell.horizontalAlign"), verticalAlign: enumValue(optionalString(style.verticalAlign, "table.cell.verticalAlign", "middle"), ["top", "middle", "bottom"] as const, "table.cell.verticalAlign") } }; }); if (occupied.size !== rows * columns) throw new Error(`presentation table ${owner} cells 未覆盖完整网格`); return { rows, columns, cells }; }
function parseChartSpec(value: Record<string, unknown>, owner: string): PresentationV5ChartSpec { exact(value, ["spec"], "chart"); const spec = object(value.spec, "chart.spec"); exact(spec, ["chartType", "title", "categories", "series"], "chart.spec"); const chartType = enumValue(string(spec.chartType, "chart.spec.chartType"), ["column", "bar", "line", "pie"] as const, "chart.spec.chartType"); const title = nullable(spec.title, "chart.spec.title", id); const categories = array(spec.categories, "chart.spec.categories").map((raw, index) => id(raw, `chart.spec.categories[${index}]`)); if (!categories.length) throw new Error(`presentation chart ${owner} categories 不能为空`); const names = new Set<string>(); const series = array(spec.series, "chart.spec.series").map((raw, index) => { const item = object(raw, `chart.spec.series[${index}]`); exact(item, ["name", "values", "color"], `chart.spec.series[${index}]`); const name = id(item.name, `chart.spec.series[${index}].name`); if (names.has(name)) throw new Error("presentation chart series 名称重复"); names.add(name); const values = array(item.values, `chart.spec.series[${index}].values`).map((entry, valueIndex) => finite(entry, `chart.spec.series[${index}].values[${valueIndex}]`)); if (values.length !== categories.length) throw new Error(`presentation chart ${owner} series ${name} values 数量必须与 categories 一致`); if (chartType === "pie" && values.some((entry) => entry < 0)) throw new Error(`presentation chart ${owner} pie values 不能为负数`); return { name, values, color: nullable(item.color, `chart.spec.series[${index}].color`, parseColor) }; }); if (!series.length) throw new Error(`presentation chart ${owner} series 不能为空`); if (chartType === "pie" && series.length !== 1) throw new Error(`presentation chart ${owner} pie 只支持一个 data series`); return { chartType, title, categories, series }; }
function parseConnector(value: Record<string, unknown>, owner: string) { exact(value, ["start", "end"], "connector"); return { start: parseEndpoint(value.start, "connector.start", owner), end: parseEndpoint(value.end, "connector.end", owner) }; }
function parseEndpoint(raw: unknown, name: string, owner: string): ConnectorEndpoint { const endpoint = object(raw, name); exact(endpoint, ["type", "value"], name); const type = enumValue(string(endpoint.type, `${name}.type`), ["free", "node"] as const, `${name}.type`); const value = object(endpoint.value, `${name}.value`); if (type === "free") { exact(value, ["x", "y"], `${name}.value`); return { type, value: { x: finite(value.x, `${name}.x`), y: finite(value.y, `${name}.y`) } }; } exact(value, ["nodeId", "anchor"], `${name}.value`); const nodeId = id(value.nodeId, `${name}.nodeId`); if (nodeId === owner) throw new Error("connector 不能连接自身"); return { type, value: { nodeId, anchor: enumValue(string(value.anchor, `${name}.anchor`), ["top", "right", "bottom", "left", "center"] as const, `${name}.anchor`) } }; }
function parseSlideBackground(raw: unknown, name: string): PresentationV5SlideBackground { return parsePaint(raw, name); }
function parseSlideTransition(raw: unknown, name: string): PresentationV5SlideTransition { const transition = object(raw, name); exact(transition, ["kind", "durationMs"], name); return { kind: enumValue(string(transition.kind, `${name}.kind`), ["none", "fade", "push", "wipe"] as const, `${name}.kind`), durationMs: optionalInteger(transition.durationMs, `${name}.durationMs`, 0, 0, 600_000) }; }
function parseTimeline(raw: unknown, name: string, nodeIds: Set<string>) { const timeline = object(raw, name); exact(timeline, ["entries"], name); const entries = arrayOrDefault(timeline.entries, `${name}.entries`).map((raw, index) => { const entry = object(raw, `${name}.entries[${index}]`); exact(entry, ["id", "targetNodeId", "trigger", "preset", "durationMs", "delayMs", "orderKey"], `${name}.entries[${index}]`); const targetNodeId = id(entry.targetNodeId, "timeline.targetNodeId"); if (!nodeIds.has(targetNodeId)) throw new Error(`timeline 引用不存在 node：${targetNodeId}`); return { id: id(entry.id, "timeline.id"), targetNodeId, trigger: enumValue(string(entry.trigger, "timeline.trigger"), ["onClick", "withPrevious", "afterPrevious"] as const, "timeline.trigger"), preset: enumValue(string(entry.preset, "timeline.preset"), ["appear", "fade", "flyIn", "wipe"] as const, "timeline.preset"), durationMs: optionalInteger(entry.durationMs, "timeline.durationMs", 0, 0, 600_000), delayMs: optionalInteger(entry.delayMs, "timeline.delayMs", 0, 0, 600_000), orderKey: id(entry.orderKey, "timeline.orderKey") }; }); unique(entries.map((entry) => entry.id), "timeline.id"); unique(entries.map((entry) => entry.orderKey), "timeline.orderKey"); return { entries }; }
function parseRichText(raw: unknown, name: string): PresentationV5RichText { const rich = object(raw, name); exact(rich, ["text", "runs", "paragraphs"], name); const text = string(rich.text, `${name}.text`); const chars = [...text].length; const runs = arrayOrDefault(rich.runs, `${name}.runs`).map((raw, index) => { const run = object(raw, `${name}.runs[${index}]`); exact(run, ["start", "end", "style"], `${name}.runs[${index}]`); const style = object(run.style, `${name}.runs[${index}].style`); exact(style, ["bold", "italic", "underline", "strikethrough", "fontFamily", "fontSize", "color"], `${name}.runs[${index}].style`); return { start: integer(run.start, "text run start", 0), end: integer(run.end, "text run end", 1), style: { bold: optionalBoolean(style.bold, "text.bold", false), italic: optionalBoolean(style.italic, "text.italic", false), underline: optionalBoolean(style.underline, "text.underline", false), strikethrough: optionalBoolean(style.strikethrough, "text.strikethrough", false), fontFamily: nullable(style.fontFamily, "text.fontFamily", id), fontSize: nullable(style.fontSize, "text.fontSize", (v, n) => bounded(finite(v, n), Number.EPSILON, 512, n)), color: nullable(style.color, "text.color", parseColor) } }; }); if (runs.length) { let expected = 0; for (const run of runs) { if (run.start !== expected || run.start >= run.end || run.end > chars) throw new Error("presentation text runs 区间无效"); expected = run.end; } if (expected !== chars) throw new Error("presentation text runs 未覆盖全文"); } const paragraphs = array(rich.paragraphs, `${name}.paragraphs`).map((raw, index) => { const paragraph = object(raw, `${name}.paragraphs[${index}]`); exact(paragraph, ["start", "end", "alignment", "list", "indentLevel"], `${name}.paragraphs[${index}]`); const list = paragraph.list === null ? null : parsePresentationList(paragraph.list, `${name}.paragraphs[${index}].list`); return { start: integer(paragraph.start, "paragraph start", 0), end: integer(paragraph.end, "paragraph end", 1), alignment: enumValue(string(paragraph.alignment, "paragraph alignment"), ["left", "center", "right", "justify"] as const, "paragraph alignment"), list, indentLevel: integer(paragraph.indentLevel, "paragraph indentLevel", 0, 8) }; }); if (chars === 0 && paragraphs.length) throw new Error("presentation empty text cannot contain paragraphs"); if (chars > 0) { let expected = 0; for (const paragraph of paragraphs) { if (paragraph.start !== expected || paragraph.start >= paragraph.end || paragraph.end > chars) throw new Error("presentation text paragraphs 区间无效"); expected = paragraph.end; } if (expected !== chars) throw new Error("presentation text paragraphs 未覆盖全文"); } return { text, runs, paragraphs }; }

function parsePresentationList(raw: unknown, name: string): PresentationV5Paragraph["list"] { const list = object(raw, name); const type = string(list.type, `${name}.type`); if (type === "bullet") { exact(list, ["type"], name); return { type }; } if (type === "ordered") { exact(list, ["type", "startAt"], name); return { type, startAt: integer(list.startAt, `${name}.startAt`, 1) }; } throw new Error(`${name}.type 无效`); }
function parseTransform(raw: unknown, name: string): PresentationV5Transform { const value = object(raw, name); exact(value, ["x", "y", "width", "height", "rotation"], name); return { x: finite(value.x, `${name}.x`), y: finite(value.y, `${name}.y`), width: positive(value.width, `${name}.width`), height: positive(value.height, `${name}.height`), rotation: optionalNumber(value.rotation, `${name}.rotation`, 0) }; }
function parseInsets(raw: unknown, name: string): Insets { const value = object(raw, name); exact(value, ["top", "right", "bottom", "left"], name); return { top: nonNegative(value.top, `${name}.top`), right: nonNegative(value.right, `${name}.right`), bottom: nonNegative(value.bottom, `${name}.bottom`), left: nonNegative(value.left, `${name}.left`) }; }
function parseCrop(raw: unknown, name: string): PresentationV5ImageCrop { const crop = raw === undefined ? { top: 0, right: 0, bottom: 0, left: 0 } : parseInsets(raw, name); for (const [key, value] of Object.entries(crop)) if (value >= 1) throw new Error(`${name}.${key} 必须小于 1`); if (crop.left + crop.right >= 1 || crop.top + crop.bottom >= 1) throw new Error(`${name} 不能裁掉整张图片`); return crop; }
function parsePaint(raw: unknown, name: string): Paint { const paint = object(raw, name); exact(paint, ["type", "value"], name); const type = enumValue(string(paint.type, `${name}.type`), ["none", "solid"] as const, `${name}.type`); if (type === "none") { if (paint.value !== undefined && paint.value !== null) throw new Error(`${name}.value 不受支持`); return { type }; } return { type, value: parseColor(paint.value, `${name}.value`) }; }
function parseColor(raw: unknown, name: string): ColorRef { const color = object(raw, name); exact(color, ["type", "value"], name); const type = enumValue(string(color.type, `${name}.type`), ["theme", "rgba"] as const, `${name}.type`); if (type === "theme") return { type, value: enumValue(string(color.value, `${name}.value`), ["background", "text", "accent1", "accent2", "accent3", "accent4", "accent5", "accent6", "hyperlink", "followedHyperlink"] as const, `${name}.value`) }; const rgba = object(color.value, `${name}.value`); exact(rgba, ["r", "g", "b", "a"], `${name}.value`); return { type, value: { r: integer(rgba.r, "rgba.r", 0, 255), g: integer(rgba.g, "rgba.g", 0, 255), b: integer(rgba.b, "rgba.b", 0, 255), a: optionalInteger(rgba.a, "rgba.a", 255, 0, 255) } }; }
function requireAsset(owner: string, assetId: string, assets: Set<string>) { if (!assets.has(assetId)) throw new Error(`${owner} 引用不存在 asset：${assetId}`); }
function validateNodeCycles(nodes: PresentationV5Node[]) { const parents = new Map(nodes.map((node) => [node.id, node.parentId])); for (const node of nodes) { const seen = new Set<string>(); let parent = node.parentId; while (parent) { if (seen.has(parent)) throw new Error("node hierarchy 存在环"); seen.add(parent); parent = parents.get(parent) ?? null; } } }
function object(value: unknown, name: string): Record<string, unknown> { if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error(`${name} 必须是 object`); return value as Record<string, unknown>; }
function array(value: unknown, name: string): unknown[] { if (!Array.isArray(value)) throw new Error(`${name} 必须是 array`); return value; }
function arrayOrDefault(value: unknown, name: string): unknown[] { return value === undefined ? [] : array(value, name); }
function string(value: unknown, name: string): string { if (typeof value !== "string") throw new Error(`${name} 必须是 string`); return value; }
function id(value: unknown, name: string): string { const result = string(value, name); if (!result.trim()) throw new Error(`${name} 不能为空`); return result; }
function nullable<T>(value: unknown, name: string, parser: (value: unknown, name: string) => T): T | null { return value === undefined || value === null ? null : parser(value, name); }
function finite(value: unknown, name: string): number { if (typeof value !== "number" || !Number.isFinite(value)) throw new Error(`${name} 必须是有限 number`); return value; }
function number(value: unknown, name: string): number { return finite(value, name); }
function optionalNumber(value: unknown, name: string, fallback: number): number { return value === undefined ? fallback : number(value, name); }
function positive(value: unknown, name: string): number { const result = finite(value, name); if (result <= 0) throw new Error(`${name} 必须大于 0`); return result; }
function nonNegative(value: unknown, name: string): number { const result = finite(value, name); if (result < 0) throw new Error(`${name} 必须非负`); return result; }
function bounded(value: number, min: number, max: number, name: string): number { if (value < min || value > max) throw new Error(`${name} 超出范围`); return value; }
function integer(value: unknown, name: string, min: number, max = Number.MAX_SAFE_INTEGER): number { const result = finite(value, name); if (!Number.isInteger(result) || result < min || result > max) throw new Error(`${name} 必须是范围内 integer`); return result; }
function optionalInteger(value: unknown, name: string, fallback: number, min: number, max = Number.MAX_SAFE_INTEGER): number { return value === undefined ? fallback : integer(value, name, min, max); }
function boolean(value: unknown, name: string): boolean { if (typeof value !== "boolean") throw new Error(`${name} 必须是 boolean`); return value; }
function optionalBoolean(value: unknown, name: string, fallback: boolean): boolean { return value === undefined ? fallback : boolean(value, name); }
function optionalString(value: unknown, name: string, fallback: string): string { return value === undefined ? fallback : string(value, name); }
function enumValue<T extends readonly string[]>(value: string, values: T, name: string): T[number] { if (!(values as readonly string[]).includes(value)) throw new Error(`${name} 无效：${value}`); return value as T[number]; }
function unique(values: string[], name: string) { if (new Set(values).size !== values.length) throw new Error(`${name} 重复`); }
function exact(value: Record<string, unknown>, fields: string[], name: string) { for (const key of Object.keys(value)) if (!fields.includes(key)) throw new Error(`${name}.${key} 不受支持`); }
