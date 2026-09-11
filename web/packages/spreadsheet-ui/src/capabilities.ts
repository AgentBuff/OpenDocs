import type { SpreadsheetSemanticCommand } from "./types";

/**
 * Structured Spreadsheet capability matrix (mirrors the presentation-ui
 * contract shape).
 *
 * A toolbar entry that is not in this matrix must never render as a
 * working-looking action, and a matrix entry whose typeId is absent from the
 * server capability catalog is filtered out by `capabilitiesForSelection`.
 * The scope/selectionRequirement pair is the render-neutral contract: the
 * Studio shell decides which surface a button lives on, this module only
 * decides whether an action may run for the current selection.
 */

export type SpreadsheetCommandScope = "sheet" | "cell" | "range" | "workbook";
export type SpreadsheetSelectionRequirement = "none" | "cell" | "range";
/** 工具栏分组；`null` 表示不直接出现在工具栏（仍受契约约束）。 */
export type SpreadsheetToolbarGroupId =
  | "clipboard"
  | "structure"
  | "font"
  | "align"
  | "number"
  | "data"
  | "view"
  | null;

export interface SpreadsheetCapabilityDescriptor {
  typeId: SpreadsheetSemanticCommand["typeId"];
  scope: SpreadsheetCommandScope;
  /** What the current selection must look like for the action to enable. */
  selectionRequirement: SpreadsheetSelectionRequirement;
  label: string;
  undoable: true;
  /** 工具栏分组归属；未分组的命令仍可被 Studio 菜单使用。 */
  toolbarGroup?: SpreadsheetToolbarGroupId;
}

export const SPREADSHEET_CAPABILITY_MATRIX: readonly SpreadsheetCapabilityDescriptor[] = [
  { typeId: "spreadsheet.setCell", scope: "cell", selectionRequirement: "cell", label: "设置单元格", undoable: true },
  { typeId: "spreadsheet.clearCell", scope: "range", selectionRequirement: "range", label: "清空单元格", undoable: true, toolbarGroup: "clipboard" },
  { typeId: "spreadsheet.setCellStyle", scope: "cell", selectionRequirement: "cell", label: "单元格格式", undoable: true, toolbarGroup: "font" },
  { typeId: "spreadsheet.setSheetMetadata", scope: "sheet", selectionRequirement: "none", label: "工作表元数据", undoable: true },
  { typeId: "spreadsheet.createSheet", scope: "sheet", selectionRequirement: "none", label: "新建工作表", undoable: true, toolbarGroup: "structure" },
  { typeId: "spreadsheet.renameSheet", scope: "sheet", selectionRequirement: "none", label: "重命名工作表", undoable: true, toolbarGroup: "structure" },
  { typeId: "spreadsheet.deleteSheet", scope: "sheet", selectionRequirement: "none", label: "删除工作表", undoable: true, toolbarGroup: "structure" },
  { typeId: "spreadsheet.insertRows", scope: "sheet", selectionRequirement: "cell", label: "插入行", undoable: true, toolbarGroup: "structure" },
  { typeId: "spreadsheet.deleteRows", scope: "sheet", selectionRequirement: "cell", label: "删除行", undoable: true, toolbarGroup: "structure" },
  { typeId: "spreadsheet.insertColumns", scope: "sheet", selectionRequirement: "cell", label: "插入列", undoable: true, toolbarGroup: "structure" },
  { typeId: "spreadsheet.deleteColumns", scope: "sheet", selectionRequirement: "cell", label: "删除列", undoable: true, toolbarGroup: "structure" },
  { typeId: "spreadsheet.mergeCells", scope: "range", selectionRequirement: "range", label: "合并单元格", undoable: true, toolbarGroup: "align" },
  { typeId: "spreadsheet.unmergeCells", scope: "range", selectionRequirement: "range", label: "取消合并", undoable: true, toolbarGroup: "align" },
  { typeId: "spreadsheet.sortRange", scope: "range", selectionRequirement: "range", label: "排序", undoable: true, toolbarGroup: "data" },
  { typeId: "spreadsheet.formatRange", scope: "range", selectionRequirement: "range", label: "范围格式", undoable: true, toolbarGroup: "font" },
  { typeId: "spreadsheet.clearRange", scope: "range", selectionRequirement: "range", label: "范围清除", undoable: true, toolbarGroup: "clipboard" },
  { typeId: "spreadsheet.replaceRange", scope: "range", selectionRequirement: "none", label: "范围替换", undoable: true, toolbarGroup: null },
  { typeId: "spreadsheet.pasteRange", scope: "range", selectionRequirement: "cell", label: "范围粘贴", undoable: true, toolbarGroup: "clipboard" },
  { typeId: "spreadsheet.fillRange", scope: "range", selectionRequirement: "range", label: "范围填充", undoable: true, toolbarGroup: "clipboard" },
  { typeId: "spreadsheet.setFreezePane", scope: "sheet", selectionRequirement: "none", label: "冻结窗格", undoable: true, toolbarGroup: "view" },
  { typeId: "spreadsheet.setAutoFilter", scope: "sheet", selectionRequirement: "none", label: "筛选", undoable: true, toolbarGroup: "data" },
  { typeId: "spreadsheet.upsertFilterColumn", scope: "sheet", selectionRequirement: "none", label: "筛选列谓词", undoable: true, toolbarGroup: "data" },
  { typeId: "spreadsheet.clearFilter", scope: "sheet", selectionRequirement: "none", label: "清除筛选列", undoable: true, toolbarGroup: "data" },
  { typeId: "spreadsheet.setCalculationMode", scope: "workbook", selectionRequirement: "none", label: "计算模式", undoable: true, toolbarGroup: null },
  { typeId: "spreadsheet.setRowLayout", scope: "sheet", selectionRequirement: "none", label: "行高与可见性", undoable: true, toolbarGroup: null },
  { typeId: "spreadsheet.setRowDimensions", scope: "sheet", selectionRequirement: "none", label: "行数维度", undoable: true, toolbarGroup: null },
  { typeId: "spreadsheet.setColumnDimensions", scope: "sheet", selectionRequirement: "none", label: "列数维度", undoable: true, toolbarGroup: null },
  { typeId: "spreadsheet.upsertConditionalFormat", scope: "sheet", selectionRequirement: "none", label: "条件格式规则", undoable: true, toolbarGroup: null },
  { typeId: "spreadsheet.deleteConditionalFormat", scope: "sheet", selectionRequirement: "none", label: "删除条件格式", undoable: true, toolbarGroup: null },
  { typeId: "spreadsheet.upsertDataValidation", scope: "sheet", selectionRequirement: "none", label: "数据校验规则", undoable: true, toolbarGroup: null },
  { typeId: "spreadsheet.deleteDataValidation", scope: "sheet", selectionRequirement: "none", label: "删除数据校验", undoable: true, toolbarGroup: null },
] as const;

/** 工具栏分组渲染顺序；不在列表中的分组不渲染。 */
export const SPREADSHEET_TOOLBAR_GROUPS: ReadonlyArray<{
  id: Exclude<SpreadsheetToolbarGroupId, null>;
  label: string;
}> = [
  { id: "clipboard", label: "剪贴板" },
  { id: "structure", label: "插入" },
  { id: "font", label: "字体" },
  { id: "align", label: "对齐" },
  { id: "data", label: "数据" },
  { id: "view", label: "视图" },
];

/** Ribbon 顶层标签页（顺序即渲染顺序）。 */
/** 腾讯文档式顶层 Ribbon。标签可先呈现为产品导航；只有已接入引擎的
 * 命令才会在功能区中变为可操作状态。 */
export type RibbonTabId = "home" | "insert" | "data" | "formula" | "collaborate" | "view" | "efficiency" | "membership";

export const RIBBON_TABS: ReadonlyArray<{ id: RibbonTabId; label: string }> = [
  { id: "home", label: "开始" },
  { id: "insert", label: "插入" },
  { id: "data", label: "数据" },
  { id: "formula", label: "公式" },
  { id: "collaborate", label: "协作" },
  { id: "view", label: "视图" },
  { id: "efficiency", label: "效率工具" },
  { id: "membership", label: "会员专享" },
];

/** Ribbon 布局契约：每个标签页内分组的渲染顺序，逐项对齐 Excel/WPS 官方布局。
 *
 * - home（开始）：剪贴板 → 字体 → 对齐 → 数字 → 单元格 → 编辑（编辑组最右，
 *   含排序筛选与查找替换；行列增删也通过插入页的行列菜单提供。
 * - data（数据）：排序和筛选在左（Excel 数据 tab 的第一组也是排序和筛选）。
 * - view（视图）：冻结窗格归入"窗口"组（Excel 视图 → 窗口 → 冻结窗格）。
 * - insert（插入）：行列菜单接入结构命令；表格/图表/图片等尚待实现。
 */
export const RIBBON_LAYOUT: Readonly<
  Record<"home" | "data" | "view", ReadonlyArray<string>>
> = {
  home: ["clipboard", "font", "align", "number", "cell", "edit"],
  data: ["sortFilter"],
  view: ["window"],
};

/** 按分组聚合能力矩阵，供工具栏按组渲染。 */
export function capabilitiesByToolbarGroup(): Map<
  Exclude<SpreadsheetToolbarGroupId, null>,
  SpreadsheetCapabilityDescriptor[]
> {
  const groups = new Map<Exclude<SpreadsheetToolbarGroupId, null>, SpreadsheetCapabilityDescriptor[]>();
  for (const descriptor of SPREADSHEET_CAPABILITY_MATRIX) {
    if (!descriptor.toolbarGroup) continue;
    const bucket = groups.get(descriptor.toolbarGroup) ?? [];
    bucket.push(descriptor);
    groups.set(descriptor.toolbarGroup, bucket);
  }
  return groups;
}

/** The active selection descriptor used by `capabilitiesForSelection`. */
export interface SpreadsheetSelectionSnapshot {
  /** True when the selection spans more than one cell. */
  isRange: boolean;
  /** True when exactly one cell is focused (single selection). */
  isCell: boolean;
  /** True when any merged range intersects the current selection. */
  intersectsMerge: boolean;
}

export function capabilitiesForSelection(
  selection: SpreadsheetSelectionSnapshot,
  availableTypeIds: ReadonlySet<string> = new Set(
    SPREADSHEET_CAPABILITY_MATRIX.map((descriptor) => descriptor.typeId),
  ),
): SpreadsheetCapabilityDescriptor[] {
  return SPREADSHEET_CAPABILITY_MATRIX.filter((descriptor) => {
    if (!availableTypeIds.has(descriptor.typeId)) return false;
    switch (descriptor.selectionRequirement) {
      case "none":
        return true;
      case "cell":
        return selection.isCell || selection.isRange;
      case "range":
        return selection.isRange || (descriptor.typeId === "spreadsheet.mergeCells" && selection.isCell);
    }
  });
}
