import { randomUUID } from "node:crypto";

import { expect, test, type APIRequestContext } from "@playwright/test";

import { deleteFixture } from "../support/fixtures.js";

async function submit(request: APIRequestContext, artifactId: string, baseRevision: number, commands: Array<{ typeId: string; payload: Record<string, unknown> }>) {
  const transactionId = randomUUID();
  const response = await request.post(`http://127.0.0.1:8788/api/artifacts/${artifactId}/transactions`, {
    headers: { "If-Match": `"${baseRevision}"`, "x-transaction-id": transactionId },
    data: {
      protocolVersion: 1,
      transactionId,
      intentId: randomUUID(),
      artifactId,
      actorId: "spreadsheet-e2e",
      baseRevision,
      origin: "local",
      commands: commands.map((command) => ({ commandId: randomUUID(), ...command })),
    },
  });
  expect(response.ok(), await response.text()).toBeTruthy();
  return response.json() as Promise<{ revision: number }>;
}

test.describe("Spreadsheet paste/fill domain command", () => {
  let artifactId: string;

  test.beforeEach(async ({ request }) => {
    const response = await request.post("http://127.0.0.1:8788/api/artifacts", {
      data: { kind: "spreadsheet", title: "E2E paste" },
    });
    expect(response.ok()).toBeTruthy();
    artifactId = (await response.json() as { id: string }).id;
  });

  test.afterEach(async ({ request }) => {
    await deleteFixture(request, { artifactId, revision: 1 });
  });

  test("copies formulas with A1 translation and retries one range command after conflict", async ({ page, request }) => {
    await submit(request, artifactId, 1, [{
      typeId: "spreadsheet.setCell",
      payload: {
        type: "setCell",
        sheetId: "sheet-1",
        row: 0,
        column: 0,
        value: null,
        formula: "=A2+$A2+A$2+$A$2+'Sheet 1'!A2",
        attrs: {},
      },
    }]);
    const pasteRequests: Array<{ baseRevision: number; commands: unknown[] }> = [];
    let injected = false;
    await page.route("**/transactions", async (route) => {
      const body = route.request().postDataJSON() as { baseRevision: number; commands: Array<{ typeId: string }> };
      if (body.commands.some((command) => command.typeId === "spreadsheet.pasteRange")) {
        pasteRequests.push({ baseRevision: body.baseRevision, commands: body.commands });
        if (!injected) {
          injected = true;
          await submit(request, artifactId, body.baseRevision, [{
            typeId: "spreadsheet.setCell",
            payload: { type: "setCell", sheetId: "sheet-1", row: 0, column: 10, value: "concurrent", formula: null, attrs: {} },
          }]);
        }
      }
      await route.continue();
    });

    await page.goto(`/?doc=${artifactId}`);
    const a1 = page.locator("[data-cell='0:0']");
    await a1.click();
    await page.getByRole("button", { name: "复制", exact: true }).click();
    await page.locator("[data-cell='1:1']").click();
    await page.getByRole("button", { name: "粘贴", exact: true }).click();
    await expect(page.locator(".ss__formula-input")).toHaveValue("=B3+$A3+B$2+$A$2+'Sheet 1'!B3");
    await expect.poll(() => pasteRequests.length).toBe(2);
    expect(pasteRequests.map((entry) => entry.baseRevision)).toEqual([2, 3]);
    expect(pasteRequests.every((entry) => entry.commands.length === 1)).toBe(true);
    await page.getByRole("button", { name: "撤销", exact: true }).click();
    await expect(page.locator("[data-cell='1:1']")).toHaveText("");
    await expect(page.locator("[data-cell='0:10']")).toHaveText("concurrent");
  });

  test("pastes quoted TSV from the system clipboard as one command", async ({ page, context }) => {
    const payloads: Array<{ commands: Array<{ typeId: string }> }> = [];
    page.on("request", (request) => {
      if (request.url().endsWith("/transactions") && request.method() === "POST") {
        payloads.push(request.postDataJSON() as { commands: Array<{ typeId: string }> });
      }
    });
    await context.grantPermissions(["clipboard-read", "clipboard-write"], { origin: "http://127.0.0.1:5175" });
    await page.goto(`/?doc=${artifactId}`);
    await page.locator("[data-cell='2:2']").click();
    await page.evaluate(() => navigator.clipboard.writeText('7\t"hello\tworld"\n=SUM(3,4)\tTRUE'));
    await page.keyboard.press("Control+V");
    await expect(page.locator("[data-cell='2:2']")).toHaveText("7");
    await expect(page.locator("[data-cell='2:3']")).toHaveText("hello\tworld");
    await expect(page.locator("[data-cell='3:2']")).toHaveText("7");
    await expect(page.locator("[data-cell='3:3']")).toHaveText("TRUE");
    expect(payloads.at(-1)?.commands).toEqual([{ typeId: "spreadsheet.pasteRange", commandId: expect.any(String), payload: expect.any(Object) }]);
  });

  test("pastes 10k system cells as one undoable history entry", async ({ page, context, request }) => {
    await context.grantPermissions(["clipboard-read", "clipboard-write"], { origin: "http://127.0.0.1:5175" });
    const matrix = Array.from({ length: 100 }, (_, row) =>
      Array.from({ length: 100 }, (_, column) => String(row * 100 + column)).join("\t"),
    ).join("\n");
    let commandCount = 0;
    let clipboardCellCount = 0;
    page.on("request", (outgoing) => {
      if (!outgoing.url().endsWith("/transactions") || outgoing.method() !== "POST") return;
      const body = outgoing.postDataJSON() as { commands: Array<{ typeId: string; payload: { cells?: unknown[] } }> };
      const paste = body.commands.find((command) => command.typeId === "spreadsheet.pasteRange");
      if (paste) {
        commandCount = body.commands.length;
        clipboardCellCount = paste.payload.cells?.length ?? 0;
      }
    });
    await page.goto(`/?doc=${artifactId}`);
    await page.locator("[data-cell='0:0']").click();
    await page.evaluate((text) => navigator.clipboard.writeText(text), matrix);
    await page.keyboard.press("Control+V");
    await expect(page.locator(".ss__status")).toContainText("revision 2");
    expect(commandCount).toBe(1);
    expect(clipboardCellCount).toBe(10_000);
    const projection = await request.get(
      `http://127.0.0.1:8788/api/artifacts/${artifactId}/projection/spreadsheet?sheetId=sheet-1&startRow=99&endRow=100&startColumn=99&endColumn=100`,
    );
    expect(projection.ok(), await projection.text()).toBeTruthy();
    expect((await projection.json() as { data: { cells: Array<{ value: number }> } }).data.cells[0].value).toBe(9_999);
    await page.getByRole("button", { name: "撤销", exact: true }).click();
    await expect(page.locator(".ss__status")).toContainText("revision 3");
    const undone = await request.get(
      `http://127.0.0.1:8788/api/artifacts/${artifactId}/projection/spreadsheet?sheetId=sheet-1&startRow=99&endRow=100&startColumn=99&endColumn=100`,
    );
    expect((await undone.json() as { data: { cells: unknown[] } }).data.cells).toEqual([]);
  });
});
