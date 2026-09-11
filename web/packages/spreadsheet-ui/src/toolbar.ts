import type { CellStyle } from "@open-office/schema/artifact";

/**
 * Toolbar 底层：纯函数层。
 *
 * 这一模块刻意不持有模型、会话或 React 状态——它把"样式合并、数字格式预设、
 * 能力→分组映射"表达为可单测的纯函数，由 Studio 壳层消费。所有格式写入仍
 * 走 `setCellStyle` 语义命令；这里只负责计算目标样式。
 */

/** 追加一个数字格式预设；保留自定义格式的合并语义。 */
export function mergeNumberFormat(base: CellStyle, format: string | null): CellStyle {
  return { ...base, numberFormat: format === null ? null : format };
}

/** 字号合并（schema 的 font.size 是浮点 pt）。 */
export function mergeFontSize(base: CellStyle, size: number | null): CellStyle {
  return {
    ...base,
    font: {
      family: base.font?.family ?? null,
      size,
      bold: base.font?.bold ?? false,
      italic: base.font?.italic ?? false,
      strikethrough: base.font?.strikethrough ?? false,
      underline: base.font?.underline ?? false,
      color: base.font?.color ?? null,
    },
  };
}

/** 字体颜色合并。 */
export function mergeFontColor(base: CellStyle, color: string | null): CellStyle {
  return {
    ...base,
    font: {
      family: base.font?.family ?? null,
      size: base.font?.size ?? null,
      bold: base.font?.bold ?? false,
      italic: base.font?.italic ?? false,
      strikethrough: base.font?.strikethrough ?? false,
      underline: base.font?.underline ?? false,
      color,
    },
  };
}

/** 垂直对齐合并。 */
export function mergeVerticalAlignment(
  base: CellStyle,
  vertical: "top" | "middle" | "bottom" | null,
): CellStyle {
  return {
    ...base,
    alignment: {
      horizontal: base.alignment?.horizontal ?? null,
      vertical,
      wrap: base.alignment?.wrap ?? false,
    },
  };
}

/** 自动换行开关。 */
export function mergeWrap(base: CellStyle, wrap: boolean): CellStyle {
  return {
    ...base,
    alignment: {
      horizontal: base.alignment?.horizontal ?? null,
      vertical: base.alignment?.vertical ?? null,
      wrap,
    },
  };
}

/** 工具栏数字格式下拉的预设集合。key 与 schema numberFormat 字符串一致。 */
export const NUMBER_FORMAT_PRESETS: ReadonlyArray<{
  key: string;
  label: string;
  sample: string;
}> = [
  { key: "General", label: "常规", sample: "1234.5" },
  { key: "0.00", label: "两位小数", sample: "1234.50" },
  { key: "#,##0", label: "千分位", sample: "1,235" },
  { key: "#,##0.00", label: "千分位两位小数", sample: "1,234.50" },
  { key: "0%", label: "百分比", sample: "123450%" },
  { key: "0.00%", label: "百分比两位小数", sample: "123450.00%" },
  { key: "¥#,##0.00", label: "人民币", sample: "¥1,234.50" },
  { key: "yyyy-mm-dd", label: "日期", sample: "2026-09-01" },
];

/** 加粗/斜体等布尔装饰的切换派生。 */
export type FontDecoration = "strikethrough" | "underline";

export function fontDecorationActive(base: CellStyle, decoration: FontDecoration): boolean {
  return base.font?.[decoration] ?? false;
}

/** 删除线/下划线切换。 */
export function mergeFontDecoration(base: CellStyle, decoration: FontDecoration, value: boolean): CellStyle {
  return {
    ...base,
    font: {
      family: base.font?.family ?? null,
      size: base.font?.size ?? null,
      bold: base.font?.bold ?? false,
      italic: base.font?.italic ?? false,
      strikethrough: decoration === "strikethrough" ? value : base.font?.strikethrough ?? false,
      underline: decoration === "underline" ? value : base.font?.underline ?? false,
      color: base.font?.color ?? null,
    },
  };
}

/**
 * 边框预设派生。`side: "all"` 四边同样式；`"outer"` 只覆盖外框（保留内部
 * 已有边）；`"none"` 清空。颜色统一为默认墨色。
 */
export type BorderPreset = {
  side: "all" | "top" | "bottom" | "left" | "right" | "outer" | "none";
  style: "thin" | "medium" | "thick" | "dashed" | "dotted" | "double" | "hair";
};

const BORDER_COLOR = "#1f2329";

export function mergeBorders(base: CellStyle, preset: BorderPreset): CellStyle {
  const edge = { style: preset.style, color: BORDER_COLOR };
  const current: NonNullable<CellStyle["borders"]> = base.borders
    ? { top: base.borders.top, bottom: base.borders.bottom, left: base.borders.left, right: base.borders.right }
    : { top: null, bottom: null, left: null, right: null };
  let next: NonNullable<CellStyle["borders"]>;
  switch (preset.side) {
    case "none":
      next = { top: null, bottom: null, left: null, right: null };
      break;
    case "all":
      next = { top: edge, bottom: edge, left: edge, right: edge };
      break;
    case "outer":
      next = { ...current, top: edge, bottom: edge, left: edge, right: edge };
      break;
    case "top":
      next = { ...current, top: edge };
      break;
    case "bottom":
      next = { ...current, bottom: edge };
      break;
    case "left":
      next = { ...current, left: edge };
      break;
    case "right":
      next = { ...current, right: edge };
      break;
  }
  // 四边全空时整个 borders 节点置空，避免持久化全 null 的空壳。
  const allEmpty = !next.top && !next.bottom && !next.left && !next.right;
  return { ...base, borders: allEmpty ? null : next };
}

/** 清除格式：样式全部归零，值与公式保持不变。 */
export function clearFormatting(_base: CellStyle): CellStyle {
  return { numberFormat: null, font: null, fill: null, alignment: null, borders: null };
}

/** 由聚焦单元格样式推导工具栏的激活态，供按钮 aria-pressed / 高亮使用。 */
export interface ToolbarStyleState {
  family: string | null;
  size: number | null;
  bold: boolean;
  italic: boolean;
  strikethrough: boolean;
  underline: boolean;
  wrap: boolean;
  horizontal: string | null;
  vertical: string | null;
  numberFormat: string | null;
  hasBorder: boolean;
  fontColor: string | null;
  fillColor: string | null;
}

export function toolbarStyleState(style: CellStyle): ToolbarStyleState {
  return {
    family: style.font?.family ?? null,
    size: style.font?.size ?? null,
    bold: style.font?.bold ?? false,
    italic: style.font?.italic ?? false,
    strikethrough: style.font?.strikethrough ?? false,
    underline: style.font?.underline ?? false,
    wrap: style.alignment?.wrap ?? false,
    horizontal: style.alignment?.horizontal ?? null,
    vertical: style.alignment?.vertical ?? null,
    numberFormat: style.numberFormat ?? null,
    hasBorder: style.borders !== null,
    fontColor: style.font?.color ?? null,
    fillColor: style.fill?.background ?? null,
  };
}

/**
 * 筛选开关的派生：为选区（或整表已用区域）生成 autoFilter 元数据。
 *
 * 开启时返回空 `columns`（引擎语义：无谓词 = 不过滤任何行，仅表示筛选
 * 模式已启用）；关闭时返回 `null`。按列谓词属于筛选配置面板的职责，
 * 不由工具栏开关伪造。
 */
export function nextAutoFilter(
  current: { range: { startRow: number; startColumn: number; endRow: number; endColumn: number }; columns: ReadonlyArray<unknown> } | null,
  range: { startRow: number; startColumn: number; endRow: number; endColumn: number },
): { active: boolean; autoFilter: { range: typeof range; columns: Array<never> } | null } {
  return {
    active: current !== null,
    autoFilter: current === null ? { range, columns: [] } : null,
  };
}

/**
 * AutoSum 范围推导（Excel 语义的简化实现）：
 * - 选区为单列多行时，求和目标 = 选区下方一格，公式覆盖整个选区；
 * - 单格时向上收集同列连续数字格（遇空/文本/公式非数字即停），
 *   目标 = 连续块下一格。
 * 返回 null 表示没有可求和的输入（工具栏按钮应禁用或提示）。
 */
export interface AutoSumTarget {
  /** =SUM(...) 写入的目标格。 */
  row: number;
  column: number;
  /** 求和范围（闭区间）。 */
  range: { startRow: number; startColumn: number; endRow: number; endColumn: number };
}

export function autoSumTarget(
  selection: { startRow: number; startColumn: number; endRow: number; endColumn: number },
  isNumeric: (row: number, column: number) => boolean,
): AutoSumTarget | null {
  const singleColumn = selection.startColumn === selection.endColumn;
  if (singleColumn && selection.endRow > selection.startRow) {
    return {
      row: selection.endRow + 1,
      column: selection.startColumn,
      range: selection,
    };
  }
  // 单格：结果写入选中格本身，向上收集同列连续数字块（Excel AutoSum 语义）。
  const column = selection.startColumn;
  const anchorRow = selection.endRow;
  let top = anchorRow - 1;
  while (top >= 0 && isNumeric(top, column)) {
    top -= 1;
  }
  const start = top + 1;
  if (start > anchorRow - 1) return null;
  return {
    row: anchorRow,
    column,
    range: { startRow: start, startColumn: column, endRow: anchorRow - 1, endColumn: column },
  };
}

/** =SUM(A1:A5) 公式文本。 */
export function sumFormula(range: {
  startRow: number;
  startColumn: number;
  endRow: number;
  endColumn: number;
}): string {
  const cell = (row: number, column: number): string => {
    let value = column;
    let letters = "";
    while (value >= 0) {
      letters = String.fromCharCode((value % 26) + 65) + letters;
      value = Math.floor(value / 26) - 1;
    }
    return `${letters}${row + 1}`;
  };
  const lo = {
    row: Math.min(range.startRow, range.endRow),
    column: Math.min(range.startColumn, range.endColumn),
  };
  const hi = {
    row: Math.max(range.startRow, range.endRow),
    column: Math.max(range.startColumn, range.endColumn),
  };
  return `=SUM(${cell(lo.row, lo.column)}:${cell(hi.row, hi.column)})`;
}

/**
 * 数字格式显示（read-only 渲染派生）：
 * 把 canonical 数值 + numberFormat 词表映射为显示文本。只覆盖工具栏预设
 * 词表；未知格式原样返回 String(value)，不猜测 Excel 全语法。
 */
export function formatDisplayValue(value: unknown, numberFormat: string | null | undefined): string {
  if (value === null || value === undefined) return "";
  if (typeof value === "boolean") return value ? "TRUE" : "FALSE";
  if (typeof value !== "number") return String(value);
  if (!Number.isFinite(value)) return String(value);
  const finite = value;
  switch (numberFormat) {
    case "yyyy-mm-dd": {
      // Excel's 1900 system includes the fictitious leap day at serial 60.
      const serial = Math.floor(finite);
      if (serial < 1 || serial > 2958465) return "########";
      if (serial === 60) return "1900-02-29";
      const days = serial < 60 ? serial + 1 : serial;
      return new Date(Date.UTC(1899, 11, 30) + days * 86_400_000).toISOString().slice(0, 10);
    }
    case "0.00":
      return finite.toFixed(2);
    case "#,##0":
      return finite.toLocaleString("en-US", { maximumFractionDigits: 0 });
    case "#,##0.00":
      return finite.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 });
    case "0%":
      return `${(finite * 100).toFixed(0)}%`;
    case "0.00%":
      return `${(finite * 100).toFixed(2)}%`;
    case "¥#,##0.00":
      return `¥${finite.toLocaleString("en-US", { minimumFractionDigits: 2, maximumFractionDigits: 2 })}`;
    default:
      return String(value);
  }
}


/** Range appearance presets; these do not create a structured data table. */
export const TABLE_STYLE_PRESETS = [
  { id: "gray", label: "灰色表格样式", header: "#30343b", stripe: "#f0f1f3" },
  { id: "blue", label: "蓝色表格样式", header: "#165dff", stripe: "#e8f3ff" },
  { id: "green", label: "绿色表格样式", header: "#00875a", stripe: "#e8f7ef" },
  { id: "red", label: "红色表格样式", header: "#d9363e", stripe: "#fff0f0" },
  { id: "yellow", label: "黄色表格样式", header: "#997300", stripe: "#fffbe6" },
  { id: "purple", label: "紫色表格样式", header: "#722ed1", stripe: "#f5edff" },
  { id: "orange", label: "橙色表格样式", header: "#a64b00", stripe: "#fff3e8" },
] as const;
export type TableStylePreset = typeof TABLE_STYLE_PRESETS[number]["id"];

export interface TableStyleOptions { headerRow: boolean; headerColumn: boolean; outline: boolean; filter: boolean; pattern: "rows" | "borders" | "columns"; }
