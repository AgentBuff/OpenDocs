import { expect, test, type APIRequestContext } from "@playwright/test";

import {
  createDocumentFixture,
  deleteFixture,
  openDocument,
  waitForPersistedText,
  type DocumentFixture,
} from "../support/fixtures.js";

const EDITOR = '[contenteditable="true"]';
const FONT_STACK = '"Noto Sans SC", "Source Han Sans SC", "PingFang SC", "Microsoft YaHei", sans-serif';

async function snapshotFontFamilies(request: APIRequestContext, artifactId: string): Promise<string[]> {
  const response = await request.get(`http://127.0.0.1:8788/api/artifacts/${artifactId}/snapshot`);
  const payload = (await response.json()) as {
    artifact: { payload: { data: { blocks?: Array<{ content?: { runs?: Array<{ style?: { fontFamily?: string | null } }> } | null }> } } };
  };
  return (payload.artifact.payload.data.blocks ?? []).flatMap(
    (block) => (block.content?.runs ?? []).flatMap((run) => (run.style?.fontFamily ? [run.style.fontFamily] : [])),
  );
}

test.describe("Document font persistence", () => {
  let fixture: DocumentFixture;

  test.beforeEach(async ({ request }) => {
    fixture = await createDocumentFixture(request, "E2E font document");
  });

  test.afterEach(async ({ request }) => {
    await deleteFixture(request, fixture);
  });

  test("applying a Chinese font from the toolbar persists and survives reload", async ({ page, request }) => {
    await openDocument(page, fixture.artifactId);
    const editor = page.locator(EDITOR).first();
    await editor.click();
    await page.keyboard.type("思源黑体测试");
    await waitForPersistedText(request, fixture.artifactId, "思源黑体测试");

    // Select the text, open the font combobox, pick 思源黑体.
    await page.keyboard.press("ControlOrMeta+a");
    const trigger = page.locator('[aria-label="字体"]').first();
    await expect(trigger).toBeEnabled();
    await trigger.click();
    await page.locator('[role="option"]', { hasText: "思源黑体" }).first().click();
    await page.keyboard.press("Escape");

    // The chosen stack is persisted verbatim (not a token).
    await expect
      .poll(async () => snapshotFontFamilies(request, fixture.artifactId), { timeout: 12_000 })
      .toContain(FONT_STACK);

    // The full stack survives reload, confirming the round-trip constraint.
    await page.reload();
    const afterReload = await snapshotFontFamilies(request, fixture.artifactId);
    expect(afterReload.filter((name) => name === FONT_STACK).length).toBeGreaterThanOrEqual(1);
  });
});
