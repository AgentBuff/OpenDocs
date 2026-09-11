import { expect, test } from "@playwright/test";

import { deleteFixture } from "../support/fixtures.js";

/**
 * 渲染消费回归：服务端投影数据必须在 Grid 上可见——
 * ① numberFormat 显示派生（货币/千分位），canonical 值不因格式而变；
 * ② 条件格式命中格以红色加粗背景高亮渲染。
 */

test.describe("Spreadsheet render consumption", () => {
  let artifactId: string;

  test.beforeEach(async ({ request }) => {
    const response = await request.post("http://127.0.0.1:8788/api/artifacts", {
      data: { kind: "spreadsheet", title: "E2E 渲染消费" },
    });
    expect(response.ok()).toBeTruthy();
    artifactId = (await response.json() as { id: string }).id;
  });

  test.afterEach(async ({ request }) => {
    await deleteFixture(request, { artifactId, revision: 1 });
  });

  test("currency number format changes the displayed text", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    const a1 = page.locator("[data-cell='0:0']");
    await expect(a1).toBeVisible();

    await a1.dblclick();
    const editor = page.locator("input.ss-grid__editor");
    await editor.fill("1234.5");
    await editor.press("Enter");
    await expect(a1).toHaveText("1234.5", { timeout: 10_000 });

    // 应用货币格式 → 显示派生为 ¥1,234.50。
    await a1.click();
    await page.locator(".ssr__homebar").locator('button[title="货币"]').first().click();
    await expect(a1).toHaveText("¥1,234.50", { timeout: 10_000 });

    // 千分位同样可见。
    await page.locator(".ssr__homebar").locator('button[title="千分位"]').first().click();
    await expect(a1).toHaveText("1,235", { timeout: 10_000 });
  });

  test("conditional format hit renders highlighted", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    const a1 = page.locator("[data-cell='0:0']");
    await expect(a1).toBeVisible();

    await a1.dblclick();
    const editor = page.locator("input.ss-grid__editor");
    await editor.fill("100");
    await editor.press("Enter");
    await expect(a1).toHaveText("100", { timeout: 10_000 });

    // 创建"大于 50"条件格式规则（选区当前为 A1）。
    await a1.click();
    await page.locator(".ssr__homebar").getByRole("button", { name: "条件格式" }).click();
    await page.getByRole("button", { name: "突出显示单元格", exact: true }).click();
    const panel = page.locator(".ssr__cformat");
    await expect(panel).toBeVisible();
    await panel.locator("input[type='number']").fill("50");
    await panel.getByRole("button", { name: "应用" }).click();

    // 命中格渲染高亮（背景色由条件样式注入）。
    await expect(a1).toHaveCSS("background-color", "rgb(255, 241, 240)", { timeout: 10_000 });
    await expect(a1).toHaveCSS("color", "rgb(245, 63, 63)");
  });

  test("boots and commits through structure plus bounded viewport projections only", async ({ page }) => {
    const snapshotRequests: string[] = [];
    const projectionResponses: Array<{ url: string; size: number }> = [];
    page.on("request", (request) => {
      if (request.url().includes(`/api/artifacts/${artifactId}/snapshot`)) snapshotRequests.push(request.url());
    });
    page.on("response", async (response) => {
      if (!response.url().includes(`/api/artifacts/${artifactId}/projection/spreadsheet`)) return;
      projectionResponses.push({ url: response.url(), size: (await response.body()).byteLength });
    });

    await page.goto(`/?doc=${artifactId}`);
    const a1 = page.locator("[data-cell='0:0']");
    await expect(a1).toBeVisible();
    await expect.poll(() => projectionResponses.some((entry) => entry.url.includes("sheetId="))).toBe(true);
    expect(snapshotRequests).toEqual([]);
    expect(projectionResponses.every((entry) => entry.size < 256 * 1024)).toBe(true);

    await a1.dblclick();
    const editor = page.locator("input.ss-grid__editor");
    await editor.fill("projection-only");
    await editor.press("Enter");
    await expect(a1).toHaveText("projection-only");
    await expect(page.locator(".ss__status")).toContainText("revision 2");
    expect(snapshotRequests).toEqual([]);
    expect(projectionResponses.filter((entry) => !entry.url.includes("?")).length).toBeGreaterThanOrEqual(2);
  });
});
