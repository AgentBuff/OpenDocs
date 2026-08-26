import { expect, test } from "@playwright/test";

import {
  createDocumentFixture,
  deleteFixture,
  openDocument,
  type DocumentFixture,
} from "../support/fixtures.js";
import { blockMenu, blockMenuTrigger } from "../support/selectors.js";

test.describe("Document chrome visual baselines", () => {
  let fixture: DocumentFixture;

  test.beforeEach(async ({ request }) => {
    fixture = await createDocumentFixture(request, "Visual chrome document");
  });

  test.afterEach(async ({ request }) => {
    await deleteFixture(request, fixture);
  });

  test("keeps the top toolbar readable in light and dark themes", async ({ page }) => {
    await openDocument(page, fixture.artifactId);
    const toolbar = page.getByRole("toolbar", { name: "文档工具栏" });
    await expect(toolbar).toHaveScreenshot("document-toolbar-light.png");

    await page.getByRole("button", { name: "切换深色主题" }).click();
    await expect(toolbar).toHaveScreenshot("document-toolbar-dark.png");
  });

  test("keeps the floating block menu legible in dark theme", async ({ page }) => {
    await openDocument(page, fixture.artifactId);
    await page.getByRole("button", { name: "切换深色主题" }).click();
    await blockMenuTrigger(page, "插入块").click();
    await expect(blockMenu(page)).toHaveScreenshot("block-menu-dark.png");
  });
});
