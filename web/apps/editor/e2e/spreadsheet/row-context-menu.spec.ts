import { expect, test } from "@playwright/test";
import { deleteFixture } from "../support/fixtures.js";

test.describe("Spreadsheet row context menu", () => {
  let artifactId: string;
  test.beforeEach(async ({ request, page }) => {
    const response = await request.post("http://127.0.0.1:8788/api/artifacts", { data: { kind: "spreadsheet", title: "E2E 行菜单" } });
    expect(response.ok()).toBeTruthy();
    artifactId = (await response.json()).id;
    await page.goto(`/?doc=${artifactId}`);
    await expect(page.locator('[data-row="0"]')).toBeVisible();
  });
  test.afterEach(async ({ request }) => { await deleteFixture(request, { artifactId, revision: 1 }); });

  test("row height persists, undo restores it, hidden rows can be recovered", async ({ page }) => {
    const row = page.locator('[data-row="6"]');
    await row.click({ button: "right" });
    await expect(page.getByRole("menu", { name: "行操作" })).toBeVisible();
    await expect(row).toHaveClass(/is-highlighted/);
    await page.getByRole("menuitem", { name: "设置行高…", exact: true }).click();
    await page.getByRole("spinbutton", { name: "行高（磅）" }).fill("42");
    await page.getByRole("button", { name: "确定", exact: true }).click();
    await expect(row).toHaveCSS("height", "56px");
    await page.reload();
    await expect(row).toHaveCSS("height", "56px");
    await page.getByTitle("撤销", { exact: true }).click();
    await expect(row).toHaveCSS("height", "28px");
    await page.getByTitle("重做", { exact: true }).click();
    await expect(row).toHaveCSS("height", "56px");
    await row.click({ button: "right" });
    await page.getByRole("menuitem", { name: "隐藏行", exact: true }).click();
    await expect(row).toHaveCount(0);
    await expect(page.locator('[data-row="7"]')).toHaveCSS("top", "168px");
    await page.reload();
    await expect(row).toHaveCount(0);
    await page.getByRole("button", { name: "显示上方隐藏行" }).click();
    await expect(row).toHaveCSS("height", "56px");
  });

  test("multi-row selection survives right click and inserts/deletes the chosen count", async ({ page }) => {
    await page.locator('[data-cell="2:0"]').dblclick();
    await page.locator("input.ss-grid__editor").fill("保留数据");
    await page.locator("input.ss-grid__editor").press("Enter");
    await expect(page.locator('[data-cell="2:0"]')).toHaveText("保留数据");
    await page.locator('[data-row="1"]').click();
    await page.locator('[data-row="3"]').click({ modifiers: ["Shift"] });
    await page.locator('[data-row="2"]').click({ button: "right" });
    await expect(page.getByRole("menu", { name: "行操作" })).toContainText("第 2–4 行");
    await expect(page.getByRole("spinbutton", { name: "插入行数" })).toHaveValue("3");
    await page.getByRole("spinbutton", { name: "插入行数" }).fill("2");
    await page.getByRole("menuitem", { name: "在上方插入", exact: true }).click();
    await expect(page.locator('[data-cell="4:0"]')).toHaveText("保留数据");
    await page.getByTitle("撤销", { exact: true }).click();
    await expect(page.locator('[data-cell="2:0"]')).toHaveText("保留数据");
    await page.locator('[data-row="2"]').click({ button: "right" });
    await page.getByRole("menuitem", { name: "删除所在行", exact: true }).click();
    await expect(page.locator('[data-cell="2:0"]')).not.toHaveText("保留数据");
    await page.getByTitle("撤销", { exact: true }).click();
    await expect(page.locator('[data-cell="2:0"]')).toHaveText("保留数据");
  });

  test("clipboard actions target the complete row and selective formats preserve values", async ({ page }) => {
    await page.locator('[data-cell="1:0"]').dblclick();
    await page.locator("input.ss-grid__editor").fill("源行");
    await page.locator("input.ss-grid__editor").press("Enter");
    await expect(page.locator('[data-cell="1:0"]')).toHaveText("源行");
    await page.getByTitle("加粗", { exact: true }).click();
    await expect(page.locator('[data-cell="1:0"]')).toHaveCSS("font-weight", "700");
    await page.locator('[data-row="1"]').click({ button: "right" });
    await page.getByRole("menuitem", { name: "复制", exact: true }).click();
    await page.locator('[data-row="4"]').click({ button: "right" });
    await page.getByRole("menuitem", { name: "粘贴", exact: true }).click();
    await expect(page.locator('[data-cell="4:0"]')).toHaveText("源行");
    await expect(page.locator('[data-cell="4:0"]')).toHaveCSS("font-weight", "700");
    await page.locator('[data-row="6"]').click({ button: "right" });
    await page.getByText("选择性粘贴", { exact: true }).click();
    await page.getByRole("menuitem", { name: "仅粘贴格式", exact: true }).click();
    await expect(page.locator('[data-cell="6:0"]')).toHaveCSS("font-weight", "700");
    await expect(page.locator('[data-cell="6:0"]')).toHaveText("");
  });

  test("menu fits a small viewport and supports Escape and keyboard navigation", async ({ page }) => {
    await page.setViewportSize({ width: 820, height: 650 });
    await page.locator('[data-row="11"]').click({ button: "right" });
    const menu = page.getByRole("menu", { name: "行操作" });
    const rect = await menu.boundingBox();
    expect(rect!.y).toBeGreaterThanOrEqual(0);
    expect(rect!.y + rect!.height).toBeLessThanOrEqual(650);
    await page.keyboard.press("ArrowDown");
    await expect(page.getByRole("menuitem", { name: "复制", exact: true })).toBeFocused();
    await page.screenshot({ path: "test-results/spreadsheet-row-context-menu.png" });
    await page.keyboard.press("Escape");
    await expect(menu).toHaveCount(0);
    await expect(page.locator(".ss-grid__viewport")).toBeFocused();
  });
});
