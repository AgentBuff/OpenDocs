import { expect, test } from "@playwright/test";

import {
  createDocumentFixture,
  deleteFixture,
  openDocument,
  waitForPersistedText,
  type DocumentFixture,
} from "../support/fixtures.js";
import { activeEditor } from "../support/selectors.js";

test.describe("Block kind selector", () => {
  let fixture: DocumentFixture;

  test.beforeEach(async ({ request }) => {
    fixture = await createDocumentFixture(request, "E2E kind-select document");
  });

  test.afterEach(async ({ request }) => {
    await deleteFixture(request, fixture);
  });

  test("converts to a quote block through the toolbar kind dropdown (shows the value, not a bare label)", async ({ page, request }) => {
    await openDocument(page, fixture.artifactId);
    const editor = activeEditor(page);
    await editor.click();
    await page.keyboard.type("quote me");
    await waitForPersistedText(request, fixture.artifactId, "quote me");

    // Open the block-style dropdown and pick 引用块.
    await page.getByRole("combobox", { name: "块样式" }).click();
    await page.locator('[role="option"]', { hasText: "引用块" }).first().click();

    // The block persisted as a quote kind.
    await expect
      .poll(
        async () => {
          const response = await request.get(
            `http://127.0.0.1:8788/api/artifacts/${fixture.artifactId}/snapshot`,
          );
          const payload = (await response.json()) as {
            artifact: { payload: { data: { blocks?: Array<{ kind?: { type?: string } }> } } };
          };
          return payload.artifact.payload.data.blocks?.[0]?.kind?.type ?? "";
        },
        { timeout: 12_000 },
      )
      .toBe("quote");

    // The toolbar keeps showing the active value "引用块" inside the dropdown
    // (not a standalone label), and the doc text is unchanged.
    await expect(page.getByRole("combobox", { name: "块样式" })).toContainText("引用块");
    await expect(activeEditor(page)).toContainText("quote me");
  });
});
