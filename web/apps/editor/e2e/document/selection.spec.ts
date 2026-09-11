import { expect, test, type APIRequestContext, type Page } from "@playwright/test";

import {
  createDocumentFixture,
  deleteFixture,
  openDocument,
  waitForPersistedText,
  type DocumentFixture,
} from "../support/fixtures.js";

const EDITOR = '[contenteditable="true"]';

async function snapshotBlocks(request: APIRequestContext, artifactId: string) {
  const response = await request.get(`http://127.0.0.1:8788/api/artifacts/${artifactId}/snapshot`);
  return (await response.json()) as {
    artifact: {
      payload: {
        data: {
          blocks?: Array<{
            content?: { text?: string; runs?: Array<{ style?: { bold?: boolean } }> } | null;
          }>;
        };
      };
    };
  };
}

test.describe("Document cross-block semantic selection", () => {
  let fixture: DocumentFixture;

  test.beforeEach(async ({ request }) => {
    fixture = await createDocumentFixture(request, "E2E cross-block document");
  });

  test.afterEach(async ({ request }) => {
    await deleteFixture(request, fixture);
  });

  /** Types three paragraphs and waits until they are durable. */
  async function seedThreeParagraphs(page: Page, request: APIRequestContext, lines: string[]) {
    await openDocument(page, fixture.artifactId);
    const editor = page.locator(EDITOR).first();
    await editor.click();
    for (const [index, line] of lines.entries()) {
      if (index > 0) {
        await page.keyboard.press("Enter");
        // The new block mounts asynchronously; typing before it exists lets
        // the first characters land in the previous block.
        await expect(page.locator(EDITOR)).toHaveCount(index + 1);
        await page.locator(EDITOR).nth(index).focus();
      }
      await page.keyboard.type(line);
    }
    await expect(async () => {
      const payload = await snapshotBlocks(request, fixture.artifactId);
      const texts = (payload.artifact.payload.data.blocks ?? [])
        .map((block) => block.content?.text ?? "")
        .filter((text) => text.length > 0);
      expect(texts).toEqual(lines);
    }).toPass({ timeout: 12_000 });
  }

  test("deleting a range spanning three blocks merges the outer remainders atomically", async ({ page, request }) => {
    await seedThreeParagraphs(page, request, ["first block", "middle block", "last block"]);

    // Build a native range spanning from the start of block 1 into the tail
    // of the last block (bypassing pointer-selection quirks under test).
    await page.evaluate(() => {
      const blocks = document.querySelectorAll('[contenteditable="true"]');
      const startNode = blocks[0].firstChild ?? blocks[0];
      const endNode = blocks[blocks.length - 1].firstChild ?? blocks[blocks.length - 1];
      const selection = window.getSelection()!;
      const range = document.createRange();
      range.setStart(startNode, 0);
      range.setEnd(endNode, Math.max(0, (endNode.textContent?.length ?? 0) - 3));
      selection.removeAllRanges();
      selection.addRange(range);
    });
    await page.keyboard.press("Backspace");

    await expect
      .poll(
        async () => {
          const payload = await snapshotBlocks(request, fixture.artifactId);
          return (payload.artifact.payload.data.blocks ?? []).filter((b) => (b.content?.text ?? "").length > 0)
            .length;
        },
        { timeout: 12_000 },
      )
      .toBe(1);

    const payload = await snapshotBlocks(request, fixture.artifactId);
    const texts = (payload.artifact.payload.data.blocks ?? []).map((b) => b.content?.text ?? "");
    // The remainder of the LAST block ("ock") is merged into the first block;
    // the middle and last blocks are deleted in the same atomic batch.
    // The tail of the last block ("ock") merges into the surviving block;
    // middle/last blocks are deleted outright instead of leaving husks.
    expect(texts.filter((text) => text.length > 0)).toEqual(["ock"]);
    expect(texts).toHaveLength(1);
  });

  test("bold applies to every selected block through one batched command set", async ({ page, request }) => {
    await seedThreeParagraphs(page, request, ["alpha one", "beta two", "gamma three"]);
    await expect(page.locator(EDITOR)).toHaveCount(3);

    // Build a native cross-block range directly (block1 tail -> block2 head)
    // so the format intent is genuinely multi-block.
    await page.evaluate(() => {
      const blocks = document.querySelectorAll('[contenteditable="true"]');
      const first = blocks[0].firstChild ?? blocks[0];
      const last = blocks[blocks.length - 1].firstChild ?? blocks[blocks.length - 1];
      const selection = window.getSelection()!;
      const range = document.createRange();
      range.setStart(first, Math.max(0, (first.textContent?.length ?? 0) - 2));
      range.setEnd(last, Math.min(2, last.textContent?.length ?? 0));
      selection.removeAllRanges();
      selection.addRange(range);
    });
    await page.keyboard.press("ControlOrMeta+b");
    await waitForPersistedText(request, fixture.artifactId, "gamma three");
    await expect(async () => {
      const payload = await snapshotBlocks(request, fixture.artifactId);
      const texts = (payload.artifact.payload.data.blocks ?? []).map((b) => b.content?.text ?? "");
      expect(texts.filter((t) => t.length > 0)).toEqual(["alpha one", "beta two", "gamma three"]);
    }).toPass({ timeout: 12_000 });

    const assertBoldEverywhere = async () => {
      const payload = await snapshotBlocks(request, fixture.artifactId);
      const blocks = payload.artifact.payload.data.blocks ?? [];
      return blocks.filter((block) =>
        (block.content?.runs ?? []).some((run) => run.style?.bold === true),
      ).length;
    };
    await expect.poll(assertBoldEverywhere, { timeout: 12_000 }).toBe(3);

    // Selection survives reconciliation: continued typing stays in place.
    await page.locator(EDITOR).nth(2).click();
    await page.keyboard.press("End");
    await page.keyboard.type("!");
    await waitForPersistedText(request, fixture.artifactId, "gamma three!");
  });
});
