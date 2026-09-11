import { expect, test, type APIRequestContext } from "@playwright/test";

import {
  createDocumentFixture,
  deleteFixture,
  openDocument,
  type DocumentFixture,
} from "../support/fixtures.js";

async function pageSemantics(request: APIRequestContext, artifactId: string) {
  const response = await request.get(`http://127.0.0.1:8788/api/artifacts/${artifactId}/snapshot`);
  const snapshot = await response.json() as {
    artifact: { payload: { data: { pageSemantics: {
      sections: Array<{ header: { default: { segments: Array<{ type: string; content?: { text: string } }> } } | null }>;
      footnotes: Array<{ content: Array<{ text: string }> }>;
    } } } };
  };
  return snapshot.artifact.payload.data.pageSemantics;
}

test.describe("Document page semantics", () => {
  let fixture: DocumentFixture;

  test.beforeEach(async ({ request }) => {
    fixture = await createDocumentFixture(request, "E2E page semantics");
  });

  test.afterEach(async ({ request }) => {
    await deleteFixture(request, fixture);
  });

  test("persists section header, page numbering and footnotes through semantic commands", async ({ page, request }) => {
    await openDocument(page, fixture.artifactId);
    await page.getByRole("button", { name: "页眉与注释" }).click();
    const panel = page.getByRole("complementary", { name: "页面语义" });
    await panel.getByLabel("页眉").fill("季度报告");
    await panel.getByLabel("页脚").fill("机密");
    await panel.getByLabel("显示页码").check();
    await panel.getByLabel("起始页码").fill("3");
    await panel.getByRole("button", { name: "应用到当前节" }).click();

    await expect.poll(async () => (await pageSemantics(request, fixture.artifactId)).sections.length).toBe(1);
    await expect(page.locator(".block-editor__header")).toContainText("季度报告");
    await expect(page.locator(".block-editor__footer")).toContainText("机密");

    await panel.getByLabel("注释内容").fill("来源说明");
    await panel.getByRole("button", { name: "添加脚注" }).click();
    await expect.poll(async () => (await pageSemantics(request, fixture.artifactId)).footnotes[0]?.content[0]?.text).toBe("来源说明");

    await page.reload();
    await expect(page.locator(".block-editor__header")).toContainText("季度报告");
    await expect(page.locator(".block-editor__notes").filter({ hasText: "脚注" })).toContainText("来源说明");
  });
});
