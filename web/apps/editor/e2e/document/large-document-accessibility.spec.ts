import { expect, test, type APIRequestContext } from "@playwright/test";

import {
  createDocumentFixture,
  deleteFixture,
  openDocument,
  waitForPersistedText,
  type DocumentFixture,
} from "../support/fixtures.js";

async function seedLargeDocument(request: APIRequestContext, fixture: DocumentFixture): Promise<void> {
  const snapshotResponse = await request.get(`http://127.0.0.1:8788/api/artifacts/${fixture.artifactId}/snapshot`);
  expect(snapshotResponse.ok()).toBeTruthy();
  const snapshot = await snapshotResponse.json() as {
    artifact: { revision: number; payload: { data: { root: string[] } } };
  };
  const firstId = snapshot.artifact.payload.data.root[0];
  const ids = [firstId, ...Array.from({ length: 999 }, (_, index) => `large-block-${index + 2}`)];
  const commands = [
    {
      commandId: crypto.randomUUID(),
      typeId: "document.replaceBlockText",
      payload: { type: "replaceBlockText", blockId: firstId, content: { text: "大文档第 1 块", runs: [] } },
    },
    ...ids.slice(1).map((id, index) => ({
      commandId: crypto.randomUUID(),
      typeId: "document.insertBlock",
      payload: {
        type: "insertBlock",
        index: index + 1,
        block: {
          id,
          kind: { type: "paragraph" },
          presentation: { align: "left", list: null, indentStart: 0, indentEnd: 0, spacingBefore: 0, spacingAfter: 0, lineHeight: 1, namedStyle: null },
          content: { text: `大文档第 ${index + 2} 块`, runs: [] },
          children: [],
          data: { type: "none" },
        },
      },
    })),
    ...Array.from({ length: 100 }, (_, index) => ({
      commandId: crypto.randomUUID(),
      typeId: "document.upsertSection",
      payload: {
        type: "upsertSection",
        index,
        section: {
          id: `large-section-${index + 1}`,
          startBlockId: ids[index * 10],
          pageSetup: null,
          header: null,
          footer: null,
          pageNumbering: null,
        },
      },
    })),
  ];
  const transactionId = crypto.randomUUID();
  const response = await request.post(`http://127.0.0.1:8788/api/artifacts/${fixture.artifactId}/transactions`, {
    headers: {
      "If-Match": `"${snapshot.artifact.revision}"`,
      "x-transaction-id": transactionId,
    },
    data: {
      protocolVersion: 1,
      transactionId,
      intentId: crypto.randomUUID(),
      artifactId: fixture.artifactId,
      actorId: "dev-user",
      baseRevision: snapshot.artifact.revision,
      origin: "local",
      commands,
    },
  });
  expect(response.ok(), await response.text()).toBeTruthy();
}

test.describe("Document large-file and accessibility contract", () => {
  let fixture: DocumentFixture;

  test.afterEach(async ({ request }) => {
    if (fixture) await deleteFixture(request, fixture);
  });

  test("mounts only the visible region of a 100-page/1000-block document and keeps active focus", async ({ page, request }) => {
    fixture = await createDocumentFixture(request, "E2E 100-page document");
    await seedLargeDocument(request, fixture);
    await openDocument(page, fixture.artifactId);

    const slots = page.locator("[data-virtual-block-id]");
    await expect(slots).toHaveCount(1000);
    await expect(page.getByRole("document", { name: "文档正文" })).toBeVisible();
    await expect(page.locator(".block-editor__page")).toHaveCount(100);
    await expect.poll(() => page.locator("[data-block-id]").count()).toBeLessThan(80);

    const first = page.locator('[contenteditable="true"]').first();
    await first.focus();
    const firstBlockId = await first.locator("xpath=ancestor::*[@data-block-id][1]").getAttribute("data-block-id");
    expect(firstBlockId).toBeTruthy();

    await page.evaluate(() => {
      const surface = document.querySelector<HTMLElement>(".editor__surface--blocks");
      const target = document.querySelector<HTMLElement>('[data-virtual-block-id="large-block-902"]');
      target?.scrollIntoView({ block: "center" });
      surface?.dispatchEvent(new Event("scroll"));
    });
    await expect(page.locator('[data-virtual-block-id="large-block-902"] [data-block-id]')).toBeVisible();
    await expect(page.locator(`[data-block-id="${firstBlockId}"]`)).toBeAttached();
    await expect.poll(() => page.locator("[data-block-id]").count()).toBeLessThan(80);

    const performance = await page.evaluate(async () => {
      const longTasks: Array<{ start: number; duration: number }> = [];
      const observer = new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) longTasks.push({ start: entry.startTime, duration: entry.duration });
      });
      if (PerformanceObserver.supportedEntryTypes.includes("longtask")) observer.observe({ type: "longtask" });
      const surface = document.querySelector<HTMLElement>(".editor__surface--blocks");
      if (!surface) throw new Error("document surface missing");
      for (let index = 0; index <= 20; index += 1) {
        surface.scrollTop = surface.scrollHeight * index / 20;
        await new Promise<void>((resolve) => requestAnimationFrame(() => resolve()));
      }
      observer.disconnect();
      const sustained = longTasks.some((entry, index) => index > 0 && entry.start - (longTasks[index - 1].start + longTasks[index - 1].duration) < 250);
      return { longTasks, sustained };
    });
    expect(performance.sustained, JSON.stringify(performance.longTasks)).toBe(false);

    const latencies = await first.evaluate(async (element) => {
      element.focus({ preventScroll: true });
      const samples: number[] = [];
      for (let index = 0; index < 20; index += 1) {
        element.textContent = `输入延迟样本 ${index}`;
        const started = performance.now();
        element.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertText", data: String(index) }));
        samples.push(performance.now() - started);
        await Promise.resolve();
      }
      return samples.sort((left, right) => left - right);
    });
    const p95 = latencies[Math.ceil(latencies.length * 0.95) - 1];
    expect(p95).toBeLessThan(16);

    await page.evaluate(() => window.dispatchEvent(new Event("beforeprint")));
    await expect(page.locator("[data-block-id]")).toHaveCount(1000);
    await page.evaluate(() => window.dispatchEvent(new Event("afterprint")));
    await expect.poll(() => page.locator("[data-block-id]").count()).toBeLessThan(80);
  });

  test("commits IME text only after composition ends", async ({ page, request }) => {
    fixture = await createDocumentFixture(request, "E2E Document IME");
    const transactionRequests: string[] = [];
    page.on("request", (requestEvent) => {
      if (requestEvent.method() === "POST" && requestEvent.url().endsWith("/transactions")) transactionRequests.push(requestEvent.url());
    });
    await openDocument(page, fixture.artifactId);
    const editable = page.locator('[contenteditable="true"]').first();
    await editable.evaluate((element) => {
      element.focus();
      element.dispatchEvent(new CompositionEvent("compositionstart", { bubbles: true, data: "" }));
      element.textContent = "中";
      element.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertCompositionText", data: "中", isComposing: true }));
      element.textContent = "中文🙂";
      element.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertCompositionText", data: "文🙂", isComposing: true }));
    });
    await page.waitForTimeout(700);
    expect(transactionRequests).toHaveLength(0);
    await editable.evaluate((element) => {
      element.dispatchEvent(new CompositionEvent("compositionend", { bubbles: true, data: "中文🙂" }));
    });
    await waitForPersistedText(request, fixture.artifactId, "中文🙂");
    expect(transactionRequests).toHaveLength(1);
  });
});
