import { expect, test, type APIRequestContext, type Page } from "@playwright/test";

import {
  createDocumentFixture,
  deleteFixture,
  openDocument,
  readArtifactRevision,
  waitForPersistedText,
  type DocumentFixture,
} from "../support/fixtures.js";

const EDITOR = '[contenteditable="true"]';

async function snapshotText(request: APIRequestContext, artifactId: string): Promise<string> {
  const response = await request.get(`http://127.0.0.1:8788/api/artifacts/${artifactId}/snapshot`);
  const payload = (await response.json()) as {
    artifact: { payload: { data: { blocks?: Array<{ content?: { text?: string } | null }> } } };
  };
  return (
    payload.artifact.payload.data.blocks?.map((block) => block.content?.text ?? "").join("\n") ?? ""
  );
}

async function typeParagraphs(page: Page, lines: string[]) {
  const editor = page.locator(EDITOR).first();
  await editor.click();
  for (const [index, line] of lines.entries()) {
    if (index > 0) await page.keyboard.press("Enter");
    await page.keyboard.type(line);
  }
}

test.describe("Document server-authoritative history", () => {
  let fixture: DocumentFixture;

  test.beforeEach(async ({ request }) => {
    fixture = await createDocumentFixture(request, "E2E history document");
  });

  test.afterEach(async ({ request }) => {
    await deleteFixture(request, fixture);
  });

  test("undo removes typed text and redo restores it through the server journal", async ({ page, request }) => {
    const revisionBefore = await readArtifactRevision(request, fixture.artifactId);
    await openDocument(page, fixture.artifactId);
    await typeParagraphs(page, ["undo me"]);
    await waitForPersistedText(request, fixture.artifactId, "undo me");

    // Mod+Z routes into the registered undo action; the intent goes to the
    // server and the canonical projection reloads from the committed result.
    await page.keyboard.press("ControlOrMeta+z");
    await expect
      .poll(async () => (await snapshotText(request, fixture.artifactId)).includes("undo me"), {
        timeout: 12_000,
      })
      .toBe(false);
    // Undo is itself a committed transaction: revision must advance, never rewind.
    const revisionAfterUndo = await readArtifactRevision(request, fixture.artifactId);
    expect(revisionAfterUndo).toBeGreaterThan(revisionBefore);

    // Wait until the undo transaction is fully settled (status back to idle),
    // otherwise the redo shortcut is legitimately disabled while the
    // projection reload is still in flight.
    await expect(page.locator(".editor__status")).toHaveText("所有更改已保存", { timeout: 15_000 });
    const revisionBeforeRedo = await readArtifactRevision(request, fixture.artifactId);
    await page.keyboard.press("ControlOrMeta+Shift+z");
    // Prove the server actually applied the redo intent.
    await waitForPersistedText(request, fixture.artifactId, "undo me");
    const revisionAfterRedo = await readArtifactRevision(request, fixture.artifactId);
    expect(revisionAfterRedo).toBeGreaterThan(revisionBeforeRedo);
    await expect(page.locator(EDITOR).first()).toContainText("undo me");
  });

  test("committed text survives an offline autosave window and reload", async ({ page, request }) => {
    await openDocument(page, fixture.artifactId);
    await typeParagraphs(page, ["offline survivor"]);
    await page.waitForTimeout(150);

    // Sever connectivity after the first burst but keep typing; coming back
    // online must still deliver everything through the autosave outbox.
    await page.context().setOffline(true);
    await page.locator(EDITOR).first().click();
    await page.keyboard.press("End");
    await page.keyboard.type("!");
    // Give the (now blocked) autosave attempt time to fail once.
    await page.waitForTimeout(600);
    await page.context().setOffline(false);

    await waitForPersistedText(request, fixture.artifactId, "offline survivor!");
    await page.reload();
    await expect(page.locator(EDITOR).first()).toContainText("offline survivor!");
  });
});
