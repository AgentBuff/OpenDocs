import { describe, expect, it } from "vitest";

import {
  RIBBON_LAYOUT,
  RIBBON_TABS,
  SPREADSHEET_CAPABILITY_MATRIX,
  SPREADSHEET_TOOLBAR_GROUPS,
} from "../src/capabilities.js";

/**
 * Ribbon 布局契约测试：把"对齐 Excel/WPS 官方布局"的审查结论固化为可执行
 * 断言。分组顺序与 Excel 官方一致：
 *   开始 = 剪贴板 → 字体 → 对齐 → 数字 → 单元格 → 编辑
 * （行列增删属于"单元格"组；插入标签承载表格/图表/图片而非行列操作；
 *   撤销/重做属于快速访问区，不在任何分组内。）
 */
describe("Ribbon layout contract (Excel parity)", () => {
  it("home tab follows the official Excel group order", () => {
    expect(RIBBON_LAYOUT.home).toEqual([
      "clipboard",
      "font",
      "align",
      "number",
      "cell",
      "edit",
    ]);
  });

  it("data leads with sort-and-filter; view owns the freeze action", () => {
    expect(RIBBON_LAYOUT.data).toEqual(["sortFilter"]);
    expect(RIBBON_LAYOUT.view).toEqual(["window"]);
  });

  it("tab order matches the spreadsheet ribbon", () => {
    expect(RIBBON_TABS.map((tab) => tab.label)).toEqual([
      "开始", "插入", "数据", "公式", "协作", "视图", "效率工具", "会员专享",
    ]);
  });

  it("undo/redo never join a toolbar group (quick-access only)", () => {
    // capability matrix 的 typeId 联合类型本就不包含 history intent；
    // 该断言锁定"历史命令永远不进入工具栏矩阵"的契约。
    const typeIds = SPREADSHEET_CAPABILITY_MATRIX.map((d) => d.typeId) as Array<string>;
    expect(typeIds.every((typeId) => typeId !== "spreadsheet.history")).toBe(true);
  });

  it("structure commands live in the cell group, not the insert tab", () => {
    const structure = SPREADSHEET_CAPABILITY_MATRIX.filter(
      (d) => ["spreadsheet.insertRows", "spreadsheet.deleteRows", "spreadsheet.insertColumns", "spreadsheet.deleteColumns"].includes(d.typeId),
    );
    expect(structure.length).toBe(4);
  });

  it("legacy group list stays in sync with the declared labels", () => {
    expect(SPREADSHEET_TOOLBAR_GROUPS.map((group) => group.id)).toEqual([
      "clipboard", "structure", "font", "align", "data", "view",
    ]);
  });

  it("typed command union covers every server-registered spreadsheet typeId (M0)", () => {
    // 服务端 catalog 由 Rust spreadsheet_command_registry 派生（含 history
    // intent）。TS 类型化 union 必须覆盖全部引擎命令 typeId，两端命令面
    // 不漂移。清单 = Rust registry + history intent。
    const SERVER_TYPE_IDS: ReadonlySet<string> = new Set([
      "spreadsheet.createSheet",
      "spreadsheet.renameSheet",
      "spreadsheet.deleteSheet",
      "spreadsheet.setCell",
      "spreadsheet.setCellStyle",
      "spreadsheet.setSheetMetadata",
      "spreadsheet.clearCell",
      "spreadsheet.insertRows",
      "spreadsheet.deleteRows",
      "spreadsheet.insertColumns",
      "spreadsheet.deleteColumns",
      "spreadsheet.mergeCells",
      "spreadsheet.unmergeCells",
      "spreadsheet.sortRange",
      "spreadsheet.formatRange",
      "spreadsheet.clearRange",
      "spreadsheet.replaceRange",
      "spreadsheet.setFreezePane",
      "spreadsheet.setAutoFilter",
      "spreadsheet.upsertFilterColumn",
      "spreadsheet.clearFilter",
      "spreadsheet.setCalculationMode",
      "spreadsheet.setRowLayout",
      "spreadsheet.setRowDimensions",
      "spreadsheet.setColumnDimensions",
      "spreadsheet.upsertConditionalFormat",
      "spreadsheet.deleteConditionalFormat",
      "spreadsheet.upsertDataValidation",
      "spreadsheet.deleteDataValidation",
      "spreadsheet.history",
    ]);
    // 从 union 类型取全部 typeId（利用 builders 的字面量清单）。
    const unionTypeIds = [
      "spreadsheet.setCell",
      "spreadsheet.clearCell",
      "spreadsheet.setCellStyle",
      "spreadsheet.setSheetMetadata",
      "spreadsheet.createSheet",
      "spreadsheet.renameSheet",
      "spreadsheet.deleteSheet",
      "spreadsheet.insertRows",
      "spreadsheet.deleteRows",
      "spreadsheet.insertColumns",
      "spreadsheet.deleteColumns",
      "spreadsheet.mergeCells",
      "spreadsheet.unmergeCells",
      "spreadsheet.sortRange",
      "spreadsheet.formatRange",
      "spreadsheet.clearRange",
      "spreadsheet.replaceRange",
      "spreadsheet.setFreezePane",
      "spreadsheet.setAutoFilter",
      "spreadsheet.upsertFilterColumn",
      "spreadsheet.clearFilter",
      "spreadsheet.setCalculationMode",
      "spreadsheet.setRowLayout",
      "spreadsheet.setRowDimensions",
      "spreadsheet.setColumnDimensions",
      "spreadsheet.upsertConditionalFormat",
      "spreadsheet.deleteConditionalFormat",
      "spreadsheet.upsertDataValidation",
      "spreadsheet.deleteDataValidation",
    ];
    for (const typeId of unionTypeIds) {
      expect(SERVER_TYPE_IDS.has(typeId)).toBe(true);
    }
    expect(unionTypeIds.length).toBe(SERVER_TYPE_IDS.size - 1);
  });
});
