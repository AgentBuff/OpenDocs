import { test, expect } from "@playwright/test";

import {
  createDocumentFixture,
  deleteFixture,
  openDocument,
  readArtifactRevision,
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

  test("edits a blank document, bumps the revision exactly once and preserves content after reload", async ({ page, request }) => {
    const revisionBefore = await readArtifactRevision(request, fixture.artifactId);
    await openDocument(page, fixture.artifactId);
    const editor = page.locator('[contenteditable="true"]').first();
    await editor.click();
    await page.keyboard.type("browser persistence");

    await waitForPersistedText(request, fixture.artifactId, "browser persistence");
    // One logical edit burst must commit as exactly one idempotent transaction:
    // the autosave outbox coalesces keystrokes instead of publishing one
    // revision per input event.
    const revisionAfter = await readArtifactRevision(request, fixture.artifactId);
    expect(revisionAfter).toBe(revisionBefore + 1);

    await page.reload();
    await expect(page.locator('[contenteditable="true"]').first()).toContainText("browser persistence");
  });
});
