import { randomUUID } from "node:crypto";
import { expect, test } from "@playwright/test";
import { deleteFixture } from "../support/fixtures.js";

test.describe("Spreadsheet merged cells", () => {
  let artifactId: string;
  test.beforeEach(async ({ request }) => {
    const response = await request.post("http://127.0.0.1:8788/api/artifacts", { data: { kind: "spreadsheet", title: "E2E 合并" } });
    expect(response.ok()).toBeTruthy();
    artifactId = (await response.json() as { id: string }).id;
  });
  test.afterEach(async ({ request }) => { await deleteFixture(request, { artifactId, revision: 1 }); });

  test("merge spans its rectangle, selects the full area, and supports navigation and undo", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    const a1 = page.locator("[data-cell='0:0']");
    const b2 = page.locator("[data-cell='1:1']");
    await a1.dblclick();
    const editor = page.locator("input.ss-grid__editor");
    await editor.fill("Merged content");
    await editor.press("Enter");
    await expect(a1).toHaveText("Merged content");
    const first = await a1.boundingBox();
    const last = await b2.boundingBox();
    await page.mouse.move(first!.x + 30, first!.y + 12);
    await page.mouse.down();
    await page.mouse.move(last!.x + 30, last!.y + 12);
    await page.mouse.up();
    await page.getByTitle("合并", { exact: true }).click();
    await expect(a1).toHaveCSS("width", "200px");
    await expect(a1).toHaveCSS("height", "56px");
    await expect(b2).toHaveCount(0);
    await a1.click({ position: { x: 160, y: 42 } });
    await expect(page.locator(".ss__formula-cell")).toHaveText("A1");
    await expect(page.locator(".ss-grid__selection")).toHaveCSS("width", "200px");
    await expect(page.locator(".ss-grid__selection")).toHaveCSS("height", "56px");
    await page.keyboard.press("ArrowRight");
    await expect(page.locator(".ss__formula-cell")).toHaveText("C1");
    await page.keyboard.press("ArrowLeft");
    await expect(page.locator(".ss__formula-cell")).toHaveText("A1");
    await page.keyboard.press("ArrowDown");
    await expect(page.locator(".ss__formula-cell")).toHaveText("A3");
    await page.getByTitle("撤销", { exact: true }).click();
    await expect(a1).toHaveCSS("width", "100px");
    await expect(b2).toHaveCount(1);
    await page.getByTitle("重做", { exact: true }).click();
    await expect(a1).toHaveCSS("width", "200px");
    await page.reload();
    await expect(a1).toHaveCSS("height", "56px");
    await expect(a1).toHaveText("Merged content");
    await a1.click();
    await page.getByTitle("合并", { exact: true }).click();
    await expect(a1).toHaveCSS("width", "100px");
    await expect(b2).toHaveCount(1);
  });

  test("a visible merged formula reloads its offscreen anchor after formatting", async ({ page, request }) => {
    const transactionId = randomUUID();
    const response = await request.post(`http://127.0.0.1:8788/api/artifacts/${artifactId}/transactions`, {
      headers: { "If-Match": '"1"', "x-transaction-id": transactionId },
      data: {
        protocolVersion: 1, transactionId, intentId: randomUUID(), artifactId,
        actorId: "spreadsheet-e2e", baseRevision: 1, origin: "local",
        commands: [
          { commandId: randomUUID(), typeId: "spreadsheet.setCell", payload: { type: "setCell", sheetId: "sheet-1", row: 0, column: 0, formula: "=SUM(20,22)" } },
          { commandId: randomUUID(), typeId: "spreadsheet.mergeCells", payload: { type: "mergeCells", sheetId: "sheet-1", range: { startRow: 0, endRow: 59, startColumn: 0, endColumn: 1 } } },
        ],
      },
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    await page.goto(`/?doc=${artifactId}`);
    const a1 = page.locator("[data-cell='0:0']");
    await expect(a1).toHaveText("42");
    const viewport = page.locator(".ss-grid__viewport");
    await viewport.evaluate((el) => { el.scrollTop = 600; });
    await expect(a1).toHaveCount(1);
    await expect(a1).toHaveCSS("height", "1680px");
    const area = await viewport.boundingBox();
    await page.mouse.click(area!.x + 70, area!.y + 80);
    await expect(page.locator(".ss__formula-cell")).toHaveText("A1");
    await page.getByTitle("货币", { exact: true }).click();
    await expect(a1).toHaveText("¥42.00");
    await expect(viewport).toHaveJSProperty("scrollTop", 600);
    await expect(page.locator(".ss__error")).toHaveCount(0);
  });
});
