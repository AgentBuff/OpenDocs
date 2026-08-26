import { test, expect } from "@playwright/test";

import {
  createDocumentFixture,
  deleteFixture,
  openDocument,
  waitForPersistedText,
  type DocumentFixture,
} from "../support/fixtures.js";

test.describe("Document boot and persistence", () => {
  let fixture: DocumentFixture;

  test.beforeEach(async ({ request }) => {
    fixture = await createDocumentFixture(request, "E2E boot document");
  });

  test.afterEach(async ({ request }) => {
    await deleteFixture(request, fixture);
  });

  test("edits a blank document and preserves the committed content after reload", async ({ page, request }) => {
    await openDocument(page, fixture.artifactId);
    const editor = page.locator('[contenteditable="true"]').first();
    await editor.click();
    await page.keyboard.type("browser persistence");
    await page.keyboard.press("Enter");

    await waitForPersistedText(request, fixture.artifactId, "browser persistence");
    await page.reload();
    await expect(page.locator('[contenteditable="true"]').first()).toContainText("browser persistence");
  });
});
