import { expect, test, type Page } from "@playwright/test";
import { deleteFixture } from "../support/fixtures.js";

async function enter(page: Page, row: number, column: number, value: string) {
  await page.locator(`[data-cell='${row}:${column}']`).dblclick();
  await page.locator("input.ss-grid__editor").fill(value);
  await page.locator("input.ss-grid__editor").press("Enter");
  await expect(page.locator(`[data-cell='${row}:${column}']`)).toHaveText(value);
}

test.describe("Spreadsheet functional toolbar panels", () => {
  let artifactId: string;
  test.beforeEach(async ({ page, request }) => {
    const response = await request.post("http://127.0.0.1:8788/api/artifacts", { data: { kind: "spreadsheet", title: "E2E functional panels" } });
    expect(response.ok()).toBeTruthy();
    artifactId = (await response.json()).id;
    await page.goto(`/?doc=${artifactId}`);
  });
  test.afterEach(async ({ request }) => { await deleteFixture(request, { artifactId, revision: 1 }); });

  test("find navigates matches; replace honors case and full-cell matching, then undo restores all", async ({ page }) => {
    await enter(page, 0, 0, "Apple"); await enter(page, 1, 0, "apple"); await enter(page, 2, 0, "pineapple");
    await page.getByRole("toolbar", { name: "开始", exact: true }).getByRole("button", { name: "查找", exact: true }).click();
    const panel = page.getByRole("dialog", { name: "查找替换" });
    await panel.getByLabel("查找内容", { exact: true }).fill("apple");
    await expect(panel.getByRole("status")).toContainText("3 个匹配");
    await panel.getByRole("button", { name: "下一个", exact: true }).click();
    await expect(panel.getByRole("status")).toContainText("A1");
    await panel.getByLabel("区分大小写").check();
    await panel.getByLabel("单元格完全匹配").check();
    await expect(panel.getByRole("status")).toContainText("1 个匹配");
    await panel.getByRole("tab", { name: "替换", exact: true }).click();
    await panel.getByLabel("替换为", { exact: true }).fill("$& fruit");
    await panel.getByRole("button", { name: "全部替换", exact: true }).click();
    await expect(page.locator("[data-cell='1:0']")).toHaveText("$& fruit");
    await expect(page.locator("[data-cell='0:0']")).toHaveText("Apple");
    await expect(page.locator("[data-cell='2:0']")).toHaveText("pineapple");
    await panel.getByRole("button", { name: "关闭查找替换" }).click();
    await page.getByTitle("撤销", { exact: true }).click();
    await expect(page.locator("[data-cell='1:0']")).toHaveText("apple");
  });

  test("table style gallery applies optional header column and alternating columns", async ({ page }) => {
    await enter(page, 0, 0, "Header");
    await page.locator("[data-cell='0:0']").click();
    await page.locator("[data-cell='3:2']").click({ modifiers: ["Shift"] });
    await page.getByTitle("表格样式", { exact: true }).click();
    expect(await page.locator(".ssr__wide-popup").evaluate(element => element.scrollWidth <= element.clientWidth)).toBe(true);
    await page.getByLabel("标题行", { exact: true }).uncheck();
    await page.getByLabel("标题列", { exact: true }).check();
    await page.getByRole("menuitem", { name: "紫色表格样式 · 隔列填充色", exact: true }).click();
    await expect(page.locator("[data-cell='0:0']")).toHaveCSS("background-color", "rgb(114, 46, 209)");
    await expect(page.locator("[data-cell='0:1']")).toHaveCSS("background-color", "rgb(245, 237, 255)");
    await expect(page.locator("[data-cell='0:2']")).toHaveCSS("background-color", "rgb(255, 255, 255)");
    await page.reload();
    await expect(page.locator("[data-cell='0:1']")).toHaveCSS("background-color", "rgb(245, 237, 255)");
  });

  test("applying a style activates a contextual tab and edits retain the original range", async ({ page }) => {
    await page.locator("[data-cell='0:0']").click();
    await page.locator("[data-cell='2:2']").click({ modifiers: ["Shift"] });
    await page.getByTitle("表格样式", { exact: true }).click();
    await page.getByRole("menuitem", { name: "蓝色表格样式", exact: true }).click();
    const tab = page.getByRole("tab", { name: "表格样式", exact: true });
    await expect(tab).toHaveAttribute("aria-selected", "true");
    const ribbon = page.getByRole("toolbar", { name: "表格样式", exact: true });
    await expect(ribbon.getByRole("button", { name: "蓝色表格样式", exact: true })).toHaveAttribute("aria-pressed", "true");
    await page.screenshot({ path: "test-results/table-style-context.png" });
    await page.locator("[data-cell='1:1']").click();
    await ribbon.getByRole("button", { name: "绿色表格样式", exact: true }).click();
    await expect(page.locator("[data-cell='0:2']")).toHaveCSS("background-color", "rgb(0, 135, 90)");
    await ribbon.getByLabel("标题行", { exact: true }).uncheck();
    await expect(page.locator("[data-cell='0:2']")).toHaveCSS("background-color", "rgb(255, 255, 255)");
    await page.locator("[data-cell='4:4']").click();
    await expect(tab).toHaveCount(0);
    await page.locator("[data-cell='1:1']").click();
    await tab.click();
    await expect(ribbon.getByLabel("标题行", { exact: true })).not.toBeChecked();
    await ribbon.getByRole("button", { name: "清除表格样式", exact: true }).click();
    await expect(tab).toHaveCount(0);
  });

  test("conditional comparisons and merge-center use real persisted commands", async ({ page }) => {
    await enter(page, 0, 0, "2"); await enter(page, 1, 0, "20");
    await page.locator("[data-cell='0:0']").click();
    await page.locator("[data-cell='1:0']").click({ modifiers: ["Shift"] });
    await page.getByTitle("条件格式", { exact: true }).click();
    await page.getByRole("button", { name: "突出显示单元格", exact: true }).click();
    expect(await page.locator(".ssr__wide-popup").evaluate(element => element.scrollWidth <= element.clientWidth)).toBe(true);
    await page.getByLabel("条件类型").selectOption("lessThan");
    await page.getByLabel("小于", { exact: true }).fill("10");
    await page.getByRole("button", { name: "应用", exact: true }).click();
    await expect(page.locator("[data-cell='0:0']")).toHaveCSS("background-color", "rgb(255, 241, 240)");
    await expect(page.locator("[data-cell='1:0']")).not.toHaveCSS("background-color", "rgb(255, 241, 240)");
    await page.getByRole("button", { name: "‹ 条件格式", exact: true }).click();
    await page.getByRole("button", { name: "管理条件格式", exact: true }).click();
    await expect(page.getByRole("list", { name: "现有规则" })).toContainText("小于 10");
    await page.keyboard.press("Escape");
    await page.locator("[data-cell='3:0']").click();
    await page.locator("[data-cell='3:1']").click({ modifiers: ["Shift"] });
    await page.getByTitle("合并选项", { exact: true }).click();
    await page.getByRole("menuitem", { name: "合并并居中", exact: true }).click();
    await expect(page.locator("[data-cell='3:0']")).toHaveCSS("width", "200px");
    await expect(page.locator("[data-cell='3:0']")).toHaveCSS("text-align", "center");
  });

  test("data ribbon persists column filters and enforces editable validation rules", async ({ page }) => {
    await enter(page, 0, 0, "5"); await enter(page, 1, 0, "15"); await enter(page, 2, 0, "25");
    await page.locator("[data-cell='0:0']").click();
    await page.locator("[data-cell='2:0']").click({ modifiers: ["Shift"] });
    await page.getByRole("tab", { name: "数据", exact: true }).click();
    const ribbon = page.getByRole("toolbar", { name: "数据", exact: true });
    await ribbon.getByLabel("筛选条件").selectOption("greaterThan");
    await ribbon.getByLabel("筛选值").fill("10");
    await ribbon.getByRole("button", { name: "应用筛选", exact: true }).click();
    await expect(page.locator("[data-cell='0:0']")).toHaveCount(0);
    await expect(page.locator("[data-cell='1:0']")).toHaveText("15");
    await ribbon.getByRole("button", { name: /清除（A 列）/ }).click();
    await expect(page.locator("[data-cell='0:0']")).toHaveText("5");

    await page.locator("[data-cell='0:1']").click();
    await page.locator("[data-cell='1:1']").click({ modifiers: ["Shift"] });
    await ribbon.getByLabel("验证类型").selectOption("wholeNumber");
    await ribbon.getByLabel("最小值").fill("0");
    await ribbon.getByLabel("最大值").fill("10");
    await ribbon.getByLabel("错误提示").fill("请输入 0 到 10 的整数");
    await ribbon.getByRole("button", { name: "应用验证", exact: true }).click();
    await expect(ribbon.getByLabel("现有数据验证")).toContainText("B1:B2 · wholeNumber");

    await page.locator("[data-cell='0:1']").dblclick();
    await page.locator("input.ss-grid__editor").fill("11");
    await page.locator("input.ss-grid__editor").press("Enter");
    await expect(page.getByRole("alert")).toContainText("请输入 0 到 10 的整数");
    await expect(page.locator("[data-cell='0:1']")).toHaveText("");

    await enter(page, 0, 1, "5");
    await ribbon.getByRole("button", { name: "删除 B1:B2 · wholeNumber", exact: true }).click();
    await enter(page, 0, 1, "11");
  });

  test("formula projection covers lookup, text, statistics, financial, and typed unsupported errors", async ({ page }) => {
    await enter(page, 0, 1, "apple"); await enter(page, 0, 2, "banana");
    await enter(page, 1, 1, "11"); await enter(page, 1, 2, "22");
    const formula = async (row: number, value: string, expected: string) => {
      await page.locator(`[data-cell='${row}:3']`).dblclick();
      await page.locator("input.ss-grid__editor").fill(value);
      await page.locator("input.ss-grid__editor").press("Enter");
      await expect(page.locator(`[data-cell='${row}:3']`)).toHaveText(expected, { timeout: 10_000 });
    };
    await formula(0, '=XLOOKUP("banana", B1:C1, B2:C2)', "22");
    await formula(1, '=TEXTJOIN("-", TRUE, "alpha", "beta")', "alpha-beta");
    await formula(2, "=AVERAGEIF(B2:C2, \">10\")", "16.5");
    await formula(3, "=NPV(0.1, 110)", "99.99999999999999");
    await formula(4, "=IFERROR(FOO(), 42)", "42");
    await formula(5, "=FOO()", "#unsupportedFunction");
  });
});
