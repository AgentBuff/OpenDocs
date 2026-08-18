/**
 * Presentation control-plane contract.
 *
 * This is deliberately a catalogue of already executable server capabilities,
 * not a wish list.  A control surface resolves items from this registry and
 * from the server capability catalogue; an absent type id must never produce a
 * working-looking action.
 */
export type PresentationControlSurface =
  | "global"
  | "insert"
  | "slide"
  | "node"
  | "text"
  | "shape"
  | "chart"
  | "table"
  | "image"
  | "media"
  | "timeline"
  | "master"
  | "layout";

export type PresentationSelectionRequirement =
  | "none"
  | "slide"
  | "single-node"
  | "multi-node"
  | "text-editing";

export interface PresentationCapabilityDefinition {
  /** Server-advertised transaction typeId. Never use an action id here. */
  readonly typeId: string;
  readonly scope: string;
  readonly surfaces: readonly PresentationControlSurface[];
  readonly selection: readonly PresentationSelectionRequirement[];
  readonly label: string;
  readonly undoable: boolean;
}

/**
 * Canonical UI inventory for the currently implemented Presentation engine.
 * Planned features live in design documents until they own a typeId, engine
 * mutation, server dispatch and tests.
 */
export const PRESENTATION_CAPABILITY_MATRIX = [
  item("presentation.history", "presentation.history", ["global"], ["none", "slide", "single-node", "multi-node", "text-editing"], "撤销与重做"),
  item("presentation.setPageSpec", "presentation.deck", ["global"], ["none"], "页面尺寸"),
  item("presentation.setTheme", "presentation.deck", ["global"], ["none", "slide"], "主题"),
  item("presentation.createMaster", "presentation.master", ["master"], ["none"], "新建母版"),
  item("presentation.updateMaster", "presentation.master", ["master"], ["none"], "编辑母版"),
  item("presentation.deleteMaster", "presentation.master", ["master"], ["none"], "删除母版"),
  item("presentation.createLayout", "presentation.layout", ["layout"], ["none"], "新建版式"),
  item("presentation.updateLayout", "presentation.layout", ["layout"], ["none"], "编辑版式"),
  item("presentation.deleteLayout", "presentation.layout", ["layout"], ["none"], "删除版式"),

  item("presentation.createSlide", "presentation.slide", ["global", "slide"], ["none", "slide"], "新建幻灯片"),
  item("presentation.deleteSlide", "presentation.slide", ["slide"], ["slide"], "删除幻灯片"),
  item("presentation.moveSlide", "presentation.slide", ["slide"], ["slide"], "移动幻灯片"),
  item("presentation.setSlideLayout", "presentation.slide", ["slide"], ["slide"], "幻灯片版式"),
  item("presentation.setSlideBackground", "presentation.slide", ["slide"], ["slide"], "幻灯片背景"),
  item("presentation.setSlideNotes", "presentation.slide", ["slide"], ["slide"], "演讲者备注"),
  item("presentation.setSlideTransition", "presentation.slide", ["slide", "timeline"], ["slide"], "页面切换"),
  item("presentation.upsertAnimation", "presentation.slide.timeline", ["timeline"], ["slide", "single-node"], "新增或修改动画"),
  item("presentation.deleteAnimation", "presentation.slide.timeline", ["timeline"], ["slide", "single-node"], "删除动画"),
  item("presentation.moveAnimation", "presentation.slide.timeline", ["timeline"], ["slide", "single-node"], "调整动画顺序"),

  item("presentation.registerAsset", "presentation.asset", ["insert"], ["slide"], "注册演示素材"),
  item("presentation.insertNode", "presentation.node", ["insert"], ["slide"], "插入对象"),
  item("presentation.deleteNode", "presentation.node", ["node", "text", "shape", "image", "media"], ["single-node", "multi-node"], "删除对象"),
  item("presentation.moveNode", "presentation.node", ["node"], ["single-node"], "移动对象层级"),
  item("presentation.reorderNode", "presentation.node", ["node"], ["single-node", "multi-node"], "调整对象顺序"),
  item("presentation.groupNodes", "presentation.node", ["node"], ["multi-node"], "组合对象"),
  item("presentation.ungroupNodes", "presentation.node", ["node"], ["single-node"], "取消组合"),
  item("presentation.setNodeTransform", "presentation.node", ["node", "text", "shape", "image", "media"], ["single-node"], "位置与大小"),
  item("presentation.setNodeLocked", "presentation.node", ["node", "text", "shape", "image", "media"], ["single-node"], "锁定对象"),
  item("presentation.alignNodes", "presentation.node", ["node"], ["multi-node"], "对齐对象"),
  item("presentation.distributeNodes", "presentation.node", ["node"], ["multi-node"], "分布对象"),
  item("presentation.setShapeStyle", "presentation.node.shape", ["shape"], ["single-node"], "形状样式"),
  item("presentation.setShapeGeometry", "presentation.node.shape", ["shape"], ["single-node"], "形状类型"),
  item("presentation.setChartSpec", "presentation.node.chart", ["chart"], ["single-node"], "图表数据"),
  item("presentation.setConnectorEndpoints", "presentation.node.connector", ["node"], ["single-node"], "连接线端点"),
  item("presentation.setTableCellContent", "presentation.node.table", ["table"], ["single-node"], "单元格内容"),
  item("presentation.setTableCellStyle", "presentation.node.table", ["table"], ["single-node"], "单元格样式"),
  item("presentation.insertTableRows", "presentation.node.table", ["table"], ["single-node"], "插入表格行"),
  item("presentation.insertTableColumns", "presentation.node.table", ["table"], ["single-node"], "插入表格列"),
  item("presentation.deleteTableRow", "presentation.node.table", ["table"], ["single-node"], "删除表格行"),
  item("presentation.deleteTableColumn", "presentation.node.table", ["table"], ["single-node"], "删除表格列"),
  item("presentation.mergeTableCells", "presentation.node.table", ["table"], ["single-node"], "合并表格单元格"),
  item("presentation.splitTableCell", "presentation.node.table", ["table"], ["single-node"], "拆分表格单元格"),
  item("presentation.setTextContent", "presentation.node.text", ["text"], ["single-node", "text-editing"], "文本内容"),
  item("presentation.setTextFrame", "presentation.node.text", ["text"], ["single-node", "text-editing"], "文本框设置"),
  item("presentation.setImageConfig", "presentation.node.image", ["image"], ["single-node"], "图片设置"),
  item("presentation.setMediaConfig", "presentation.node.media", ["media"], ["single-node"], "媒体设置"),
] as const satisfies readonly PresentationCapabilityDefinition[];

export type PresentationCapabilityTypeId = (typeof PRESENTATION_CAPABILITY_MATRIX)[number]["typeId"];

export function presentationCapability(typeId: string): PresentationCapabilityDefinition | undefined {
  return PRESENTATION_CAPABILITY_MATRIX.find((item) => item.typeId === typeId);
}

export function capabilitiesForSurface(
  surface: PresentationControlSurface,
  availableCapabilities: ReadonlySet<string>,
  selection: PresentationSelectionRequirement,
): readonly PresentationCapabilityDefinition[] {
  return PRESENTATION_CAPABILITY_MATRIX.filter((item) =>
    item.surfaces.includes(surface)
    && item.selection.includes(selection)
    && availableCapabilities.has(item.typeId),
  );
}

function item(
  typeId: string,
  scope: string,
  surfaces: readonly PresentationControlSurface[],
  selection: readonly PresentationSelectionRequirement[],
  label: string,
): PresentationCapabilityDefinition {
  return { typeId, scope, surfaces, selection, label, undoable: true };
}
