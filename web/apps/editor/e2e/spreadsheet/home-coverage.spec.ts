import { expect, test } from "@playwright/test";

import { deleteFixture } from "../support/fixtures.js";

/**
 * 开始标签页功能覆盖：
 * ① AutoSum——选中含数字的列 → 点求和 → 下方格出现 =SUM 且投影算出结果；
 * ② 条件格式——大于阈值规则创建 → 投影 conditionalStyles 命中 → 面板删除。
 */

test.describe("Spreadsheet home tab coverage", () => {
  let artifactId: string;

  test.beforeEach(async ({ request }) => {
    const response = await request.post("http://127.0.0.1:8788/api/artifacts", {
      data: { kind: "spreadsheet", title: "E2E 开始页" },
    });
    expect(response.ok()).toBeTruthy();
    artifactId = (await response.json() as { id: string }).id;
  });

  test.afterEach(async ({ request }) => {
    await deleteFixture(request, { artifactId, revision: 1 });
  });

  test("home sorting menu supports descending and ascending", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    for (const [row, value] of [[0, "10"], [1, "30"], [2, "20"]] as const) {
      const cell = page.locator(`[data-cell='${row}:0']`);
      await cell.dblclick();
      const editor = page.locator("input.ss-grid__editor");
      await editor.fill(value);
      await editor.press("Enter");
      await expect(cell).toHaveText(value);
    }
    const first = await page.locator("[data-cell='0:0']").boundingBox();
    const last = await page.locator("[data-cell='2:0']").boundingBox();
    await page.mouse.move(first!.x + 30, first!.y + 12);
    await page.mouse.down();
    await page.mouse.move(last!.x + 30, last!.y + 12);
    await page.mouse.up();
    const ribbon = page.locator(".ssr__homebar");
    await ribbon.getByRole("button", { name: "排序", exact: true }).click();
    await page.getByRole("menuitem", { name: "降序", exact: true }).click();
    await expect(page.locator("[data-cell='0:0']")).toHaveText("30");
    await expect(page.locator("[data-cell='2:0']")).toHaveText("10");
    await ribbon.getByRole("button", { name: "排序", exact: true }).click();
    await page.getByRole("menuitem", { name: "升序", exact: true }).click();
    await expect(page.locator("[data-cell='0:0']")).toHaveText("10");
    await expect(page.locator("[data-cell='2:0']")).toHaveText("30");
  });

  test("home clipboard copies and pastes selected cells", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    const a1 = page.locator("[data-cell='0:0']");
    await a1.dblclick();
    const editor = page.locator("input.ss-grid__editor");
    await editor.fill("clipboard-marker");
    await editor.press("Enter");
    await expect(a1).toHaveText("clipboard-marker");
    await a1.click();
    const ribbon = page.locator(".ssr__homebar");
    await ribbon.getByRole("button", { name: "复制", exact: true }).click();
    const b1 = page.locator("[data-cell='0:1']");
    await b1.click();
    await ribbon.getByRole("button", { name: "粘贴", exact: true }).click();
    await expect(b1).toHaveText("clipboard-marker");
    await ribbon.getByRole("button", { name: "撤销", exact: true }).click();
    await expect(b1).toHaveText("");
    await expect(a1).toHaveText("clipboard-marker");
  });

  test("date format shows a calendar date, selects its preset, and survives reload", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    const a1 = page.locator("[data-cell='0:0']");
    await a1.dblclick();
    const editor = page.locator("input.ss-grid__editor");
    await editor.fill("=DATE(2026,9,2)");
    await editor.press("Enter");
    await expect(a1).toHaveText("46267");
    await a1.click();
    await page.getByTitle("数字格式", { exact: true }).click();
    await page.getByRole("option", { name: "日期（2026-09-01）", exact: true }).click();
    await expect(a1).toHaveText("2026-09-02");
    await expect(page.getByRole("option")).toHaveCount(0);
    await page.getByTitle("数字格式", { exact: true }).click();
    await expect(page.getByRole("option", { name: "日期（2026-09-01）", exact: true })).toHaveAttribute("aria-selected", "true");
    await page.keyboard.press("Escape");
    await page.reload();
    await expect(a1).toHaveText("2026-09-02");
    await a1.click();
    await page.getByTitle("数字格式", { exact: true }).click();
    await page.getByRole("option", { name: "人民币（¥1,234.50）", exact: true }).click();
    await expect(a1).toHaveText("¥46,267.00");
    await page.getByTitle("数字格式", { exact: true }).click();
    await page.getByRole("option", { name: "百分比两位小数（123450.00%）", exact: true }).click();
    await expect(a1).toHaveText("4626700.00%");
  });

  test("insert tab row and column commands persist and can be undone", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    const a1 = page.locator("[data-cell='0:0']");
    await a1.dblclick();
    const editor = page.locator("input.ss-grid__editor");
    await editor.fill("row-column-marker");
    await editor.press("Enter");
    await expect(a1).toHaveText("row-column-marker");
    await a1.click();
    await page.getByRole("tab", { name: "插入", exact: true }).click();
    await page.getByRole("button", { name: "行列", exact: true }).click();
    await page.getByRole("menuitem", { name: "在上方插入行", exact: true }).click();
    await expect(page.locator("[data-cell='1:0']")).toHaveText("row-column-marker");
    await expect(page.getByRole("menuitem")).toHaveCount(0);
    await page.getByRole("button", { name: "行列", exact: true }).click();
    await page.getByRole("menuitem", { name: "在左侧插入列", exact: true }).click();
    await expect(page.locator("[data-cell='1:1']")).toHaveText("row-column-marker");
    await page.getByRole("tab", { name: "开始", exact: true }).click();
    await page.getByRole("button", { name: "撤销", exact: true }).click();
    await expect(page.locator("[data-cell='1:0']")).toHaveText("row-column-marker");
    await page.getByRole("button", { name: "重做", exact: true }).click();
    await expect(page.locator("[data-cell='1:1']")).toHaveText("row-column-marker");
    await page.reload();
    await expect(page.locator("[data-cell='1:1']")).toHaveText("row-column-marker");
  });

  test("autofill sum inserts a real =SUM formula below the selection", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    const viewport = page.locator(".ss-grid__viewport");
    await expect(viewport).toBeVisible();

    // 写入 A1=10, A2=20（双击编辑）。
    for (const [row, value] of [[0, "10"], [1, "20"]] as const) {
      await page.locator(`[data-cell='${row}:0']`).dblclick();
      const editor = page.locator("input.ss-grid__editor");
      await editor.fill(value);
      await editor.press("Enter");
      await expect(page.locator(`[data-cell='${row}:0']`)).toHaveText(value, { timeout: 10_000 });
    }

    // 选中 A1:A2（拖拽选区：pointer down + move + up）。
    const a1 = page.locator("[data-cell='0:0']");
    const a2 = page.locator("[data-cell='1:0']");
    const box1 = await a1.boundingBox();
    const box2 = await a2.boundingBox();
    await page.mouse.move(box1!.x + box1!.width / 2, box1!.y + box1!.height / 2);
    await page.mouse.down();
    await page.mouse.move(box2!.x + box2!.width / 2, box2!.y + box2!.height / 2);
    await page.mouse.up();

    // 点开始页"求和"。
    await page.locator(".ssr__homebar").getByRole("button", { name: "求和" }).click();
    await expect(page.locator(".ss__error")).toHaveCount(0);

    // A3 出现公式计算结果 30（服务端求值投影）。
    await expect(page.locator("[data-cell='2:0']")).toHaveText("30", { timeout: 10_000 });
    // 公式栏确认是 =SUM。
    await page.locator("[data-cell='2:0']").click();
    await expect(page.locator("input.ss__formula-input")).toHaveValue(/=SUM\(A1:A2\)/, { timeout: 10_000 });
  });

  test("conditional format panel creates and deletes a rule", async ({ page }) => {
    const failures: string[] = [];
    page.on("response", (response) => {
      if (response.status() >= 400) failures.push(`${response.status()} ${response.url()}`);
    });
    const transactions: string[] = [];
    page.on("request", (request) => {
      if (request.url().includes("/transactions") && request.method() === "POST") {
        transactions.push(request.postData()?.slice(0, 400) ?? "empty");
      }
    });
    await page.goto(`/?doc=${artifactId}`);
    const a1 = page.locator("[data-cell='0:0']");
    await expect(a1).toBeVisible();

    // A1 = 100（大于阈值 50 的格子）。
    await a1.dblclick();
    const editor = page.locator("input.ss-grid__editor");
    await editor.fill("100");
    await editor.press("Enter");
    await expect(a1).toHaveText("100", { timeout: 10_000 });

    // 选中 A1，打开条件格式面板，创建"大于 50"规则。
    await a1.click();
    const cfTrigger = page.locator(".ssr__homebar").getByRole("button", { name: "条件格式" });
    await cfTrigger.click();
    await page.getByRole("button", { name: "突出显示单元格", exact: true }).click();
    const panel = page.locator(".ssr__cformat");
    await expect(panel).toBeVisible();
    await panel.locator("input[type='number']").fill("50");
    await panel.getByRole("button", { name: "应用" }).click();
    await expect(page.locator(".ss__error")).toHaveCount(0);

    // 规则出现在面板列表中。
    await expect(panel.locator(".ssr__cformat-rule")).toHaveCount(1, { timeout: 10_000 });
    expect(failures, `API 错误：${failures.join("; ")}`).toEqual([]);
    console.log("transactions:", JSON.stringify(transactions, null, 1));

    // 删除规则。
    await panel.locator(".ssr__cformat-delete").click();
    await expect(panel.locator(".ssr__cformat-rule")).toHaveCount(0, { timeout: 10_000 });
  });
});
