import { expect, test } from "@playwright/test";

import {
  createDocumentFixture,
  deleteFixture,
  openDocument,
  type DocumentFixture,
} from "../support/fixtures.js";
import { blockMenuTrigger } from "../support/selectors.js";

const testImage = Buffer.from(
  '<svg xmlns="http://www.w3.org/2000/svg" width="160" height="100"><rect width="160" height="100" fill="#2684ff"/></svg>',
);

test.describe("Document table and image interactions", () => {
  let fixture: DocumentFixture;

  test.beforeEach(async ({ request }) => {
    fixture = await createDocumentFixture(request, "E2E table and image document");
  });

  test.afterEach(async ({ request }) => {
    await deleteFixture(request, fixture);
  });

  test("inserts a table, edits a cell, and keeps row context actions available", async ({ page }) => {
    await openDocument(page, fixture.artifactId);
    await blockMenuTrigger(page, "插入块").click();
    await page.getByLabel("插入表格").click();
    await page.getByRole("gridcell", { name: "2行2列表格" }).click();

    const table = page.getByRole("table", { name: "表格块" });
    await expect(table).toBeVisible();
    const firstCell = table.locator("td").first();
    await firstCell.click();
    await page.keyboard.type("editable cell");
    await expect(firstCell).toContainText("editable cell");

    const secondCell = table.locator("td").nth(1);
    await secondCell.click({ modifiers: ["Shift"] });
    await expect(page.getByRole("button", { name: "合并单元格" })).toBeVisible();
    await page.getByRole("button", { name: "合并单元格" }).click();
    await expect(table.locator("td")).toHaveCount(3);
    await expect(page.getByRole("button", { name: "拆分单元格" })).toBeVisible();
    await page.getByRole("button", { name: "拆分单元格" }).click();
    await expect(table.locator("td")).toHaveCount(4);

    const firstRow = page.getByRole("button", { name: "选择第 1 行" });
    await firstRow.click();
    await expect(firstRow).toHaveAttribute("aria-pressed", "true");
    await firstRow.click({ button: "right" });
    await expect(page.getByRole("menu", { name: "行操作菜单" })).toBeVisible();
  });

  test("expands a stable-id table range with surface drag and Shift+Arrow", async ({ page }) => {
    await openDocument(page, fixture.artifactId);
    await blockMenuTrigger(page, "插入块").click();
    await page.getByLabel("插入表格").click();
    await page.getByRole("gridcell", { name: "2行2列表格" }).click();

    const table = page.getByRole("table", { name: "表格块" });
    const firstCell = table.locator("td").first();
    const secondCell = table.locator("td").nth(1);
    const firstBox = await firstCell.boundingBox();
    const secondBox = await secondCell.boundingBox();
    if (!firstBox || !secondBox) throw new Error("表格单元格未布局");
    await page.mouse.move(firstBox.x + firstBox.width / 2, firstBox.y + firstBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(secondBox.x + secondBox.width / 2, secondBox.y + secondBox.height / 2);
    await page.mouse.up();
    await expect(page.getByRole("button", { name: "合并单元格" })).toBeVisible();

    await firstCell.click();
    await firstCell.press("Shift+ArrowRight");
    await expect(page.getByRole("button", { name: "合并单元格" })).toBeVisible();
  });

  test("resizes only the two columns and the row owned by a table boundary", async ({ page }) => {
    await openDocument(page, fixture.artifactId);
    await blockMenuTrigger(page, "插入块").click();
    await page.getByLabel("插入表格").click();
    await page.getByRole("gridcell", { name: "2行2列表格" }).click();

    const table = page.getByRole("table", { name: "表格块" });
    const firstCell = table.locator("td").first();
    const secondCell = table.locator("td").nth(1);
    const belowCell = table.locator("td").nth(2);
    const beforeFirst = await firstCell.boundingBox();
    const beforeSecond = await secondCell.boundingBox();
    const beforeBelow = await belowCell.boundingBox();
    const columnHandle = page.getByRole("button", { name: "调整第 1 与第 2 列宽度" });
    const columnBox = await columnHandle.boundingBox();
    if (!beforeFirst || !beforeSecond || !beforeBelow || !columnBox) throw new Error("表格边界未布局");
    await page.mouse.move(columnBox.x + columnBox.width / 2, columnBox.y + 8);
    await page.mouse.down();
    await page.mouse.move(columnBox.x + columnBox.width / 2 + 24, columnBox.y + 8);
    await page.mouse.up();
    await expect.poll(async () => (await firstCell.boundingBox())?.width ?? 0).toBeGreaterThan(beforeFirst.width + 10);
    await expect.poll(async () => (await secondCell.boundingBox())?.width ?? Number.MAX_SAFE_INTEGER).toBeLessThan(beforeSecond.width - 10);

    const rowHandle = page.getByRole("button", { name: "调整第 1 与第 2 行高度" });
    const rowBox = await rowHandle.boundingBox();
    if (!rowBox) throw new Error("表格行边界未布局");
    await page.mouse.move(rowBox.x + 12, rowBox.y + rowBox.height / 2);
    await page.mouse.down();
    await page.mouse.move(rowBox.x + 12, rowBox.y + rowBox.height / 2 + 20);
    await page.mouse.up();
    await expect.poll(async () => (await firstCell.boundingBox())?.height ?? 0).toBeGreaterThan(beforeFirst.height + 10);
    await expect.poll(async () => (await belowCell.boundingBox())?.height ?? 0).toBeCloseTo(beforeBelow.height, 0);
  });

  test("selects an inserted image and exposes its object toolbar", async ({ page }) => {
    await openDocument(page, fixture.artifactId);
    await blockMenuTrigger(page, "插入块").click();
    const menu = page.getByRole("menu", { name: "块菜单" });
    await menu.getByRole("menuitem", { name: "图片" }).click();
    await menu.locator('input[type="file"]').setInputFiles({
      name: "test-image.svg",
      mimeType: "image/svg+xml",
      buffer: testImage,
    });

    const image = page.getByRole("group", { name: "test-image.svg" });
    await expect(image).toBeVisible();
    await image.click();
    await expect(page.getByRole("toolbar", { name: "图片工具栏" })).toBeVisible();
    await expect(page.getByRole("button", { name: "裁剪图片" })).toBeVisible();

    await page.keyboard.press("ArrowRight");
    await expect(page.locator("[data-block-id]")).toHaveCount(2);
    await expect(page.locator('[contenteditable="true"]')).toHaveCount(1);

    await image.click();
    await page.keyboard.press("ArrowLeft");
    await expect(page.locator("[data-block-id]")).toHaveCount(3);

    // Object navigation is dispatched by the editor root. Enter is the
    // explicit "continue after object" affordance: when a trailing paragraph
    // already exists it moves the caret there instead of adding a duplicate.
    await image.click();
    await expect(image).toBeFocused();
    await page.keyboard.press("Enter");
    await expect(page.locator("[data-block-id]")).toHaveCount(3);
    await expect(page.locator('[contenteditable="true"]').last()).toBeFocused();
  });

  test("creates caret paragraphs on both object sides and explicitly cancels image selection", async ({ page }) => {
    await openDocument(page, fixture.artifactId);
    await blockMenuTrigger(page, "插入块").click();
    const menu = page.getByRole("menu", { name: "块菜单" });
    await menu.getByRole("menuitem", { name: "图片" }).click();
    await menu.locator('input[type="file"]').setInputFiles({
      name: "test-image.svg",
      mimeType: "image/svg+xml",
      buffer: testImage,
    });

    const image = page.getByRole("group", { name: "test-image.svg" });
    await image.click();
    await page.keyboard.press("ArrowLeft");
    await expect(page.locator("[data-block-id]")).toHaveCount(2);
    const beforeImage = page.locator('[contenteditable="true"]').first();
    await expect(beforeImage).toBeFocused();

    // Enter while parked to the left inserts another paragraph before the
    // atomic block, moving the image down as a user expects.
    await beforeImage.press("Enter");
    await expect(page.locator("[data-block-id]")).toHaveCount(3);

    await image.click();
    await expect(page.getByRole("toolbar", { name: "图片工具栏" })).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByRole("toolbar", { name: "图片工具栏" })).toBeHidden();
    await expect(image).toHaveAttribute("aria-selected", "false");
  });

  test("Enter after an isolated image creates the following paragraph", async ({ page }) => {
    await openDocument(page, fixture.artifactId);
    await blockMenuTrigger(page, "插入块").click();
    const menu = page.getByRole("menu", { name: "块菜单" });
    await menu.getByRole("menuitem", { name: "图片" }).click();
    await menu.locator('input[type="file"]').setInputFiles({
      name: "test-image.svg",
      mimeType: "image/svg+xml",
      buffer: testImage,
    });

    const image = page.getByRole("group", { name: "test-image.svg" });
    await image.click();
    await page.keyboard.press("Enter");
    await expect(page.locator("[data-block-id]")).toHaveCount(2);
    await expect(page.locator('[contenteditable="true"]')).toBeFocused();
  });

  test("moves, resizes, crops, commits and cancels image object edits", async ({ page }) => {
    await openDocument(page, fixture.artifactId);
    await blockMenuTrigger(page, "插入块").click();
    const menu = page.getByRole("menu", { name: "块菜单" });
    await menu.getByRole("menuitem", { name: "图片" }).click();
    await menu.locator('input[type="file"]').setInputFiles({
      name: "test-image.svg",
      mimeType: "image/svg+xml",
      buffer: testImage,
    });

    const image = page.getByRole("group", { name: "test-image.svg" });
    await image.click();
    const frame = image.locator(".block-image__frame");
    const beforeMove = await frame.boundingBox();
    if (!beforeMove) throw new Error("图片框未布局");
    await page.mouse.move(beforeMove.x + beforeMove.width / 2, beforeMove.y + beforeMove.height / 2);
    await page.mouse.down();
    await page.mouse.move(beforeMove.x + beforeMove.width / 2 + 24, beforeMove.y + beforeMove.height / 2 + 8);
    await page.mouse.up();
    await expect.poll(async () => (await frame.boundingBox())?.x ?? 0).toBeGreaterThan(beforeMove.x + 10);

    const beforeResize = await frame.boundingBox();
    if (!beforeResize) throw new Error("图片框未布局");
    const southEast = page.getByRole("button", { name: "拖动调整图片se" });
    await southEast.hover();
    await page.mouse.down();
    await page.mouse.move(beforeResize.x + beforeResize.width + 40, beforeResize.y + beforeResize.height + 30);
    await page.mouse.up();
    await expect.poll(async () => (await frame.boundingBox())?.width ?? 0).toBeGreaterThan(beforeResize.width + 15);

    await page.getByRole("button", { name: "裁剪图片" }).click();
    await expect(page.getByLabel("图片裁剪区域")).toBeVisible();
    await page.getByRole("button", { name: "裁剪图片" }).click();
    await expect(page.getByLabel("图片裁剪区域")).toBeHidden();

    await page.getByRole("button", { name: "裁剪图片" }).click();
    await expect(page.getByLabel("图片裁剪区域")).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(page.getByLabel("图片裁剪区域")).toBeHidden();
  });
});
