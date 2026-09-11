import { expect, test, type APIRequestContext } from "@playwright/test";

import {
  createDocumentFixture,
  deleteFixture,
  openDocument,
  waitForPersistedText,
  type DocumentFixture,
} from "../support/fixtures.js";

async function snapshotText(request: APIRequestContext, artifactId: string): Promise<string> {
  const response = await request.get(`http://127.0.0.1:8788/api/artifacts/${artifactId}/snapshot`);
  const snapshot = await response.json() as {
    artifact: { payload: { data: { blocks: Array<{ content?: { text: string } | null }> } } };
  };
  return snapshot.artifact.payload.data.blocks.map((block) => block.content?.text ?? "").join("\n");
}

test.describe("Document search, replace and navigation", () => {
  let fixture: DocumentFixture;

  test.beforeEach(async ({ request }) => {
    fixture = await createDocumentFixture(request, "E2E search document");
  });

  test.afterEach(async ({ request }) => {
    await deleteFixture(request, fixture);
  });

  test("navigates Unicode matches and keeps replacement undoable across refresh", async ({ page, request }) => {
    await openDocument(page, fixture.artifactId);
    const editor = page.locator('.block-row__content[contenteditable="true"]').first();
    await editor.fill("路线图 😀 路线图");
    await waitForPersistedText(request, fixture.artifactId, "路线图 😀 路线图");

    await page.keyboard.press("ControlOrMeta+f");
    const find = page.getByRole("searchbox", { name: "查找内容" });
    await find.fill("路线图");
    await expect(page.locator(".document-find output")).toHaveText("1/2");
    await page.getByRole("button", { name: "下一个匹配" }).click();
    await expect.poll(() => page.evaluate(() => window.getSelection()?.toString())).toBe("路线图");

    await page.getByRole("textbox", { name: "替换内容" }).fill("计划");
    await page.getByRole("button", { name: "替换", exact: true }).click();
    await waitForPersistedText(request, fixture.artifactId, "路线图 😀 计划");
    await page.getByRole("button", { name: "全部替换" }).click();
    await waitForPersistedText(request, fixture.artifactId, "计划 😀 计划");

    await page.getByRole("button", { name: "关闭查找" }).click();
    await page.keyboard.press("ControlOrMeta+z");
    await expect.poll(async () => snapshotText(request, fixture.artifactId)).toContain("路线图 😀 计划");

    await page.reload();
    await expect(page.locator('.block-row__content[contenteditable="true"]').first()).toContainText("路线图 😀 计划");
    await expect(page.getByRole("complementary", { name: "文档目录" })).toContainText("文档中还没有标题");
  });
});
