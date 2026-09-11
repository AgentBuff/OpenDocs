import { describe, expect, it } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import type { SheetModel } from "@open-office/schema/artifact";
import { SpreadsheetGrid } from "../src/spreadsheet/SpreadsheetGrid.js";

/**
 * 表头回归测试：行号列（左）与列标行（顶）曾经全部堆叠在条带原点——
 * 绝对定位 cell 缺少 `left`/`top` 坐标，视觉上表现为"没有行号和列标"。
 * 这里通过 SSR 输出断言每个表头 cell 携带独立坐标。
 */

const noop = () => undefined;

const sheet: SheetModel = {
  id: "s1",
  name: "Sheet 1",
  cells: [
    { row: 0, column: 0, value: "A1", attrs: {}, style: null },
    { row: 1, column: 1, value: "B2", attrs: {}, style: null },
  ],
  metadata: {
    visibility: "visible",
    rowCount: 50,
    columnCount: 20,
    freeze: { rows: 0, columns: 0 },
    autoFilter: null,
    sort: null,
    conditionalFormats: [],
    dataValidations: [],
    mergedRanges: [],
    media: [],
  },
};

function renderGrid() {
  return renderToStaticMarkup(
    <SpreadsheetGrid
      id="artifact-1"
      sheetId="s1"
      sheet={sheet}
      revision={1}
      selection={null}
      editing={null}
      canEdit={false}
      onSelect={noop}
      onSelectRow={noop}
      onSelectColumn={noop}
      onSelectAll={noop}
      onActivate={noop}
      onEditCommit={noop}
      onEditCancel={noop}
    />,
  );
}

describe("SpreadsheetGrid headers", () => {
  it("positions each column header cell at its own horizontal offset", () => {
    const html = renderGrid();
    expect(html).toContain('left:0;');
    expect(html).toContain('left:100px;');
    expect(html).toContain('left:200px;');
    // 列标内容使用 A1 风格字母。
    expect(html).toContain(">A</div>");
    expect(html).toContain(">B</div>");
  });

  it("positions each row header cell at its own vertical offset and prints 1-based numbers", () => {
    const html = renderGrid();
    expect(html).toContain('top:0;');
    expect(html).toContain('top:28px;');
    expect(html).toContain('top:56px;');
    expect(html).toContain(">1</div>");
    expect(html).toContain(">2</div>");
    expect(html).toContain(">3</div>");
  });

  it("keeps the selectable corner cell for select-all", () => {
    const html = renderGrid();
    expect(html).toContain('aria-label="全选"');
  });

  it("translates an inner layer instead of the clipping strip", () => {
    // transform 连剪裁框一起移动：滚动时整个条带会滑走、序号看起来不再递增。
    // 正确结构 = 条带只剪裁，内层 .ss-grid__headlayer 承担 translate。
    const html = renderGrid();
    const stripTags = html.match(/class="ss-grid__(colhead|rowhead)"/g) ?? [];
    expect(stripTags.length).toBe(2);
    expect(html.match(/ss-grid__headlayer/g)?.length).toBe(2);
    // 初始 SSR 输出的条带容器自身不应带 transform。
    expect(html).not.toMatch(/class="ss-grid__colhead" style="transform/);
    expect(html).not.toMatch(/class="ss-grid__rowhead" style="transform/);
  });
});
