import { expect, test, type APIRequestContext, type Locator } from "@playwright/test";

import {
  createDocumentFixture,
  deleteFixture,
  openDocument,
  waitForPersistedText,
  type DocumentFixture,
} from "../support/fixtures.js";

async function snapshot(request: APIRequestContext, artifactId: string) {
  const response = await request.get(`http://127.0.0.1:8788/api/artifacts/${artifactId}/snapshot`);
  return await response.json() as {
    artifact: { revision: number; payload: { data: { root: string[]; blocks: Array<{ id: string; content?: { text: string } }> } } };
  };
}

async function reviews(request: APIRequestContext, artifactId: string) {
  const response = await request.get(`http://127.0.0.1:8788/api/artifacts/${artifactId}/reviews`);
  return await response.json() as { threads: Array<{ kind: string; state: string; messages: Array<{ mentions: string[] }> }> };
}

async function selectScalars(editor: Locator, start: number, end: number) {
  await editor.evaluate((element, range) => {
    const walker = document.createTreeWalker(element, NodeFilter.SHOW_TEXT);
    const text = walker.nextNode();
    if (!text) throw new Error("editable text node missing");
    const selection = window.getSelection();
    const nativeRange = document.createRange();
    nativeRange.setStart(text, range.start);
    nativeRange.setEnd(text, range.end);
    selection?.removeAllRanges();
    selection?.addRange(nativeRange);
    document.dispatchEvent(new Event("selectionchange"));
  }, { start, end });
}

test.describe("Document review and presence", () => {
  let fixture: DocumentFixture;

  test.beforeEach(async ({ request }) => {
    fixture = await createDocumentFixture(request, "E2E review document");
  });

  test.afterEach(async ({ request }) => {
    await deleteFixture(request, fixture);
  });

  test("persists mentions, accepts a semantic suggestion and renders remote selection", async ({ page, request }) => {
    await openDocument(page, fixture.artifactId);
    const editor = page.locator('.block-row__content[contenteditable="true"]').first();
    await editor.fill("A中🙂Z");
    await waitForPersistedText(request, fixture.artifactId, "A中🙂Z");
    await selectScalars(editor, 1, 4);

    await page.getByRole("button", { name: "审阅" }).click();
    const panel = page.getByRole("complementary", { name: "文档审阅" });
    await expect(panel).toBeVisible();
    await panel.getByPlaceholder("评论内容，可使用 @用户ID").fill("请 @reviewer 检查");
    await panel.getByRole("button", { name: "添加评论" }).click();
    await expect.poll(async () => (await reviews(request, fixture.artifactId)).threads[0]?.messages[0]?.mentions[0]).toBe("reviewer");

    await selectScalars(editor, 1, 4);
    await panel.getByPlaceholder("评论内容，可使用 @用户ID").fill("建议调整");
    await panel.getByPlaceholder("建议替换为…").fill("reviewed");
    await panel.getByRole("button", { name: "提出建议" }).click();
    const suggestion = panel.locator('[data-review-thread-id]').filter({ hasText: "建议调整" });
    await expect(suggestion).toContainText("中🙂");
    await suggestion.getByRole("button", { name: "接受" }).click();
    await waitForPersistedText(request, fixture.artifactId, "AreviewedZ");
    await expect.poll(async () => (await reviews(request, fixture.artifactId)).threads.find((thread) => thread.kind === "suggestion")?.state).toBe("accepted");

    const current = await snapshot(request, fixture.artifactId);
    const blockId = current.artifact.payload.data.root[0]!;
    const presenceResponse = await request.put(
      `http://127.0.0.1:8788/api/artifacts/${fixture.artifactId}/presence/remote-test`,
      { data: {
        revision: current.artifact.revision,
        blockId,
        selectedNodeIds: [],
        selection: { anchor: { blockId, offset: 1 }, focus: { blockId, offset: 4 } },
      } },
    );
    expect(presenceResponse.status()).toBe(204);
    await expect(page.locator(`[data-block-id="${blockId}"]`)).toHaveAttribute("data-remote-presence", "开发用户", { timeout: 5_000 });

    await page.reload();
    await page.getByRole("button", { name: "审阅" }).click();
    await expect(page.getByRole("complementary", { name: "文档审阅" })).toContainText("请 @reviewer 检查");
    await expect(editor).toContainText("AreviewedZ");
  });
});
