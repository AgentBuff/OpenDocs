import { expect, test } from "@playwright/test";

import {
  createDocumentFixture,
  deleteFixture,
  openDocument,
  type DocumentFixture,
} from "../support/fixtures.js";
import { activeEditor, blockMenu, blockMenuTrigger, blockRows, editableBlocks } from "../support/selectors.js";

test.describe("List keyboard semantics", () => {
  let fixture: DocumentFixture;

  test.beforeEach(async ({ request }) => {
    fixture = await createDocumentFixture(request, "E2E list document");
  });

  test.afterEach(async ({ request }) => {
    await deleteFixture(request, fixture);
  });

  test("Enter continues a list and an empty list item exits it", async ({ page }) => {
    await openDocument(page, fixture.artifactId);
    const editor = activeEditor(page);
    await editor.fill("first item");

    await blockMenuTrigger(page, "打开块菜单").click();
    await expect(blockMenu(page)).toBeVisible();
    await page.getByRole("menuitemradio", { name: "编号列表" }).click();

    await editor.press("End");
    await editor.press("Enter");
    await expect(blockRows(page)).toHaveCount(2);
    await expect(page.locator(".block-row__list-marker")).toHaveCount(2);

    await editableBlocks(page).nth(1).press("Enter");
    await expect(blockRows(page)).toHaveCount(3);
    await expect(page.locator(".block-row__list-marker")).toHaveCount(1);
  });
});
