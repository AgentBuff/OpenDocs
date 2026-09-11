import { expect, test, type Page } from "@playwright/test";
import { deleteFixture } from "../support/fixtures.js";

async function selectRange(page: Page, from: string, to: string) {
  const first = await page.locator(`[data-cell='${from}']`).boundingBox();
  const last = await page.locator(`[data-cell='${to}']`).boundingBox();
  await page.mouse.move(first!.x + 30, first!.y + 12);
  await page.mouse.down();
  await page.mouse.move(last!.x + 30, last!.y + 12);
  await page.mouse.up();
}

async function chooseSize(page: Page, size: string) {
  await page.getByTitle("字号", { exact: true }).click();
  await page.getByRole("option", { name: size, exact: true }).click();
}

test.describe("Spreadsheet range formatting", () => {
  let artifactId: string;
  test.beforeEach(async ({ request }) => {
    const response = await request.post("http://127.0.0.1:8788/api/artifacts", {
      data: { kind: "spreadsheet", title: "E2E 范围格式" },
    });
    expect(response.ok()).toBeTruthy();
    artifactId = (await response.json() as { id: string }).id;
  });
  test.afterEach(async ({ request }) => {
    await deleteFixture(request, { artifactId, revision: 1 });
  });

  test("editing formatted numbers and formulas uses their original input", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    const a1 = page.locator("[data-cell='0:0']");
    await a1.dblclick();
    let editor = page.locator("input.ss-grid__editor");
    await editor.fill("0.1");
    await editor.press("Enter");
    await expect(a1).toHaveText("0.1");
    await a1.click();
    await page.getByTitle("百分比", { exact: true }).click();
    await expect(a1).toHaveText("10.00%");
    await a1.dblclick();
    editor = page.locator("input.ss-grid__editor");
    await expect(editor).toHaveValue("0.1");
    await editor.fill("=SUM(1,2)");
    await editor.press("Enter");
    await expect(a1).toHaveText("300.00%");
    await a1.dblclick();
    editor = page.locator("input.ss-grid__editor");
    await expect(editor).toHaveValue("=SUM(1,2)");
    await editor.press("Escape");
    await expect(a1).toHaveText("300.00%");
    await page.reload();
    await expect(a1).toHaveText("300.00%");
    await a1.dblclick();
    await expect(page.locator("input.ss-grid__editor")).toHaveValue("=SUM(1,2)");
    await page.locator("input.ss-grid__editor").fill("");
    await page.locator("input.ss-grid__editor").press("Enter");
    await expect(a1).toHaveText("");
    await a1.dblclick();
    await page.locator("input.ss-grid__editor").fill("0.5");
    await page.locator("input.ss-grid__editor").press("Enter");
    await expect(a1).toHaveText("50.00%");
  });

  test("table style applies header and alternating rows as one undoable action", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    const b2 = page.locator("[data-cell='1:1']");
    await b2.dblclick();
    const editor = page.locator("input.ss-grid__editor");
    await editor.fill("0.1");
    await editor.press("Enter");
    await expect(b2).toHaveText("0.1");
    await b2.click();
    await page.getByTitle("百分比", { exact: true }).click();
    await expect(b2).toHaveText("10.00%");
    await selectRange(page, "0:0", "2:1");
    await page.getByTitle("表格样式", { exact: true }).click();
    await page.getByRole("menuitem", { name: "蓝色表格样式", exact: true }).click();
    const a1 = page.locator("[data-cell='0:0']");
    const a3 = page.locator("[data-cell='2:0']");
    await expect(a1).toHaveCSS("background-color", "rgb(22, 93, 255)");
    await expect(a1).toHaveCSS("font-weight", "700");
    await expect(a1).toHaveCSS("color", "rgb(255, 255, 255)");
    await expect(b2).toHaveCSS("background-color", "rgb(232, 243, 255)");
    await expect(b2).toHaveText("10.00%");
    await expect(a3).toHaveCSS("background-color", "rgb(255, 255, 255)");
    await page.getByRole("tab", { name: "开始", exact: true }).click();
    await page.getByTitle("撤销", { exact: true }).click();
    await expect(a1).not.toHaveCSS("background-color", "rgb(22, 93, 255)");
    await expect(b2).toHaveText("10.00%");
    await page.getByTitle("重做", { exact: true }).click();
    await expect(a1).toHaveCSS("background-color", "rgb(22, 93, 255)");
    await page.reload();
    await expect(a1).toHaveCSS("background-color", "rgb(22, 93, 255)");
    await expect(b2).toHaveCSS("background-color", "rgb(232, 243, 255)");
    await expect(b2).toHaveText("10.00%");
  });

  test("same-anchor font size applies to the rest of a mixed range without overwriting other styles", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    const a1 = page.locator("[data-cell='0:0']");
    const b1 = page.locator("[data-cell='0:1']");
    await a1.click();
    await chooseSize(page, "18");
    await expect(a1).toHaveCSS("font-size", "24px");
    await b1.dblclick();
    const editor = page.locator("input.ss-grid__editor");
    await editor.fill("0.1");
    await editor.press("Enter");
    await expect(b1).toHaveText("0.1");
    await b1.click();
    await chooseSize(page, "10");
    await expect(b1).toHaveCSS("font-size", "13.3333px");
    await page.getByTitle("斜体", { exact: true }).click();
    await expect(b1).toHaveCSS("font-style", "italic");
    await page.getByTitle("百分比", { exact: true }).click();
    await expect(b1).toHaveText("10.00%");
    await page.getByTitle("字体颜色", { exact: true }).click();
    await page.getByRole("option", { name: "#165dff", exact: true }).click();
    await expect(b1).toHaveCSS("color", "rgb(22, 93, 255)");
    await page.keyboard.press("Escape");
    await selectRange(page, "0:0", "0:1");
    await chooseSize(page, "18");
    await expect(b1).toHaveCSS("font-size", "24px");
    await expect(b1).toHaveCSS("font-style", "italic");
    await expect(b1).toHaveCSS("color", "rgb(22, 93, 255)");
    await expect(b1).toHaveText("10.00%");
    await page.getByTitle("撤销", { exact: true }).click();
    await expect(b1).toHaveCSS("font-size", "13.3333px");
    await expect(a1).toHaveCSS("font-size", "24px");
    await page.getByTitle("重做", { exact: true }).click();
    await expect(b1).toHaveCSS("font-size", "24px");
    await page.reload();
    await expect(b1).toHaveCSS("font-size", "24px");
    await expect(b1).toHaveCSS("font-style", "italic");
    await expect(b1).toHaveText("10.00%");
  });

  test("outer borders leave the inside clear and persist across reload", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    await expect(page.locator("[data-cell='0:0']")).toBeVisible();
    await selectRange(page, "0:0", "2:2");
    await page.getByTitle("边框", { exact: true }).click();
    await page.getByRole("menuitem", { name: "粗外侧框线", exact: true }).click();
    const a1 = page.locator("[data-cell='0:0']");
    const b1 = page.locator("[data-cell='0:1']");
    const b2 = page.locator("[data-cell='1:1']");
    const c3 = page.locator("[data-cell='2:2']");
    await expect(a1).toHaveCSS("border-top-width", "2px");
    await expect(a1).toHaveCSS("border-left-width", "2px");
    await expect(b1).toHaveCSS("border-top-width", "2px");
    await expect(b2).toHaveCSS("border-top-width", "0px");
    await expect(b2).toHaveCSS("border-right-width", "1px");
    await expect(c3).toHaveCSS("border-right-width", "2px");
    await page.keyboard.press("Escape");
    await page.getByTitle("撤销", { exact: true }).click();
    await expect(a1).toHaveCSS("border-top-width", "0px");
    await page.getByTitle("重做", { exact: true }).click();
    await expect(a1).toHaveCSS("border-top-width", "2px");
    await page.reload();
    await expect(c3).toHaveCSS("border-bottom-width", "2px");
    await expect(b2).toHaveCSS("border-top-width", "0px");
  });

  test("real Chinese fonts survive editing, undo and reload", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    const cell = page.locator("[data-cell='0:0']");
    await cell.dblclick();
    await page.locator("input.ss-grid__editor").fill("中文字体检查 Font 123");
    await page.locator("input.ss-grid__editor").press("Enter");
    await cell.click();
    await page.getByRole("combobox", { name: "字体", exact: true }).click();
    await page.getByRole("textbox", { name: "搜索字体" }).fill("思源宋体");
    await page.getByRole("option", { name: "思源宋体", exact: true }).click();
    await expect(cell).toHaveCSS("font-family", '"Noto Serif SC", serif');
    await expect(page.getByRole("combobox", { name: "字体", exact: true })).toBeEnabled();
    await chooseSize(page, "24");
    await expect(cell).toHaveCSS("font-size", "32px");
    await expect(cell).toHaveCSS("height", "50px");
    await page.getByTitle("加粗", { exact: true }).click();
    await expect(cell).toHaveCSS("font-family", '\"Noto Serif SC\", serif');
    await expect.poll(() => page.evaluate(() => Array.from(document.fonts).some(face => face.family.replaceAll('"', '') === 'Noto Serif SC' && face.status === 'loaded'))).toBe(true);
    await cell.dblclick();
    const editor = page.locator("input.ss-grid__editor");
    await expect(editor).toHaveCSS("font-family", '\"Noto Serif SC\", serif');
    await expect(editor).toHaveCSS("font-size", "32px");
    await expect(editor).toHaveCSS("font-weight", "700");
    await editor.fill("修改后仍保留字体 Font 456");
    await editor.press("Enter");
    await page.getByTitle("撤销", { exact: true }).click();
    await expect(cell).toHaveText("中文字体检查 Font 123");
    await page.getByTitle("重做", { exact: true }).click();
    await expect(cell).toHaveText("修改后仍保留字体 Font 456");
    await page.reload();
    await expect(cell).toHaveCSS("font-family", '\"Noto Serif SC\", serif');
    await expect(cell).toHaveCSS("font-size", "32px");
    await expect(cell).toHaveCSS("height", "50px");
    await expect.poll(() => page.evaluate(() => Array.from(document.fonts).some(face => face.family.replaceAll('"', '') === 'Noto Serif SC' && face.status === 'loaded'))).toBe(true);
    await page.screenshot({ path: "test-results/spreadsheet-font-check.png" });
  });

  test("font family and vertical alignment affect rendered text geometry", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    const a1 = page.locator("[data-cell='0:0']");
    await a1.dblclick();
    const editor = page.locator("input.ss-grid__editor");
    await editor.fill("Alignment");
    await editor.press("Enter");
    await expect(a1).toHaveText("Alignment");
    await a1.click();
    await page.getByTitle("字体", { exact: true }).click();
    await page.getByRole("option", { name: "Lora", exact: true }).click();
    await expect(a1).toHaveCSS("font-family", "Lora, serif");
    await page.getByTitle("顶端对齐", { exact: true }).click();
    await expect(a1).toHaveCSS("justify-content", "flex-start");
    const top = (await a1.locator(".ss-grid__value").boundingBox())!.y;
    await page.getByTitle("底端对齐", { exact: true }).click();
    await expect(a1).toHaveCSS("justify-content", "flex-end");
    expect((await a1.locator(".ss-grid__value").boundingBox())!.y).toBeGreaterThan(top + 5);
    await page.getByTitle("垂直居中", { exact: true }).click();
    await expect(a1).toHaveCSS("justify-content", "center");
  });
});
