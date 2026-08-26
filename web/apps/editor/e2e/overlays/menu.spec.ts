import { expect, test } from "@playwright/test";

import {
  createDocumentFixture,
  deleteFixture,
  openDocument,
  type DocumentFixture,
} from "../support/fixtures.js";
import { activeEditor, blockMenu, blockMenuTrigger } from "../support/selectors.js";

test.describe("Block menu dismissal", () => {
  let fixture: DocumentFixture;

  test.beforeEach(async ({ request }) => {
    fixture = await createDocumentFixture(request, "E2E menu document");
  });

  test.afterEach(async ({ request }) => {
    await deleteFixture(request, fixture);
  });

  test("closes on Escape and outside pointer interaction", async ({ page }) => {
    await openDocument(page, fixture.artifactId);
    const trigger = blockMenuTrigger(page, "插入块");
    await trigger.click();
    await expect(blockMenu(page)).toBeVisible();
    await page.keyboard.press("Escape");
    await expect(blockMenu(page)).toBeHidden();

    await trigger.click();
    await expect(blockMenu(page)).toBeVisible();
    await activeEditor(page).click();
    await expect(blockMenu(page)).toBeHidden();
  });

  test("keeps a table context menu above a cross-block selection layer through the portal", async ({ page }) => {
    await openDocument(page, fixture.artifactId);
    await blockMenuTrigger(page, "插入块").click();
    await page.getByLabel("插入表格").click();
    await page.getByRole("gridcell", { name: "2行2列表格" }).click();

    const firstCell = page.getByRole("table", { name: "表格块" }).locator("td").first();
    await firstCell.click({ button: "right" });
    const menu = page.getByRole("menu", { name: "单元格操作菜单" });
    await expect(menu).toBeVisible();
    expect(await menu.evaluate((node) => node.closest("[data-block-id]") === null)).toBeTruthy();

    const box = await menu.boundingBox();
    if (!box) throw new Error("表格右键菜单未布局");
    const portalIsTopmost = await page.evaluate(({ x, y }) => {
      const top = document.elementFromPoint(x, y);
      return top?.closest('[data-table-menu="context"]') !== null;
    }, { x: box.x + 24, y: box.y + 24 });
    expect(portalIsTopmost).toBeTruthy();
  });
});
