import { expect, test } from "@playwright/test";

import {
  createDocumentFixture,
  deleteFixture,
  openDocument,
  waitForPersistedText,
  type DocumentFixture,
} from "../support/fixtures.js";
import { activeEditor } from "../support/selectors.js";

test.describe("Document native Unicode selection", () => {
  let fixture: DocumentFixture;

  test.beforeEach(async ({ request }) => {
    fixture = await createDocumentFixture(request, "E2E Unicode selection document");
  });

  test.afterEach(async ({ request }) => {
    await deleteFixture(request, fixture);
  });

  test("formats a CJK and Emoji native range without splitting the surrogate pair", async ({ page, request }) => {
    await openDocument(page, fixture.artifactId);
    const editor = activeEditor(page);
    await editor.click();
    await page.keyboard.type("甲🙂乙");
    await waitForPersistedText(request, fixture.artifactId, "甲🙂乙");

    // DOM Range offsets are UTF-16 code units. The end offset 3 therefore
    // selects exactly `甲🙂`; the engine must convert that to two Unicode
    // scalar offsets before issuing its semantic inline-range command.
    await editor.evaluate((element) => {
      const text = element.firstChild;
      if (!text) throw new Error("缺少原生文本节点");
      const range = document.createRange();
      range.setStart(text, 0);
      range.setEnd(text, 3);
      const selection = window.getSelection();
      selection?.removeAllRanges();
      selection?.addRange(range);
      document.dispatchEvent(new Event("selectionchange"));
    });

    const bold = page.getByRole("button", { name: "加粗" });
    await expect(bold).toBeEnabled();
    await bold.click();
    await expect(page.locator(".alert")).toHaveCount(0);
    await expect(editor.locator("strong")).toHaveText("甲🙂");
    await expect(editor).toHaveText("甲🙂乙");
  });
});
