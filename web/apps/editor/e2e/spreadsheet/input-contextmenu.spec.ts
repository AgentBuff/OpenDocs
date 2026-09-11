import { expect, test } from "@playwright/test";

import { deleteFixture } from "../support/fixtures.js";

/**
 * Excel 核心交互回归：
 * ① 单击选中后直接打字 → 覆盖式进入编辑 → Enter 提交 → 服务端持久化；
 * ② 单元格右键菜单可见且"清除内容"生效。
 */

test.describe("Spreadsheet cell input and context menu", () => {
  let artifactId: string;

  test.beforeEach(async ({ request }) => {
    const response = await request.post("http://127.0.0.1:8788/api/artifacts", {
      data: { kind: "spreadsheet", title: "E2E 输入与右键" },
    });
    expect(response.ok()).toBeTruthy();
    artifactId = (await response.json() as { id: string }).id;
  });

  test.afterEach(async ({ request }) => {
    await deleteFixture(request, { artifactId, revision: 1 });
  });

  test("typing directly on a selected cell edits and persists", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    const a1 = page.locator("[data-cell='0:0']");
    await expect(a1).toBeVisible();

    // 直接打字：不双击，Excel 覆盖式编辑。首字符触发进入编辑（值为 h），
    // 后续字符由编辑框自身接收（React 状态传播快于连续 keydown），
    // 最终编辑框承载完整文本——与 Excel 行为一致。
    await a1.click();
    await page.keyboard.type("hello!");
    const editor = page.locator("input.ss-grid__editor");
    await expect(editor).toBeVisible();
    await expect(editor).toHaveValue("hello!");
    await page.keyboard.press("Enter");
    await expect(page.locator(".ss__error")).toHaveCount(0);

    // 持久化验证：等服务端快照写盘后 A1 显示完整文本。
    await expect(a1).toHaveText("hello!", { timeout: 10_000 });
  });

  test("IME composition does not trigger type-to-edit", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    const a1 = page.locator("[data-cell='0:0']");
    await expect(a1).toBeVisible();

    await a1.click();
    // 模拟中文输入法 composition keydown：isComposing=true，key="Process"。
    // 这类事件必须被忽略——不进入编辑、不吞字符。
    await page.locator(".ss-grid__viewport").dispatchEvent("keydown", {
      key: "Process",
      code: "KeyA",
      isComposing: true,
      bubbles: true,
      cancelable: true,
    });
    await page.waitForTimeout(300);
    await expect(page.locator("input.ss-grid__editor")).toHaveCount(0);

    // 非 composition 的普通字符仍然进入编辑（对照）。
    await page.keyboard.type("x");
    await expect(page.locator("input.ss-grid__editor")).toBeVisible();
    await page.keyboard.press("Escape");
  });

  test("cell context menu clears contents", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    const a1 = page.locator("[data-cell='0:0']");
    await expect(a1).toBeVisible();

    // 先写入一个值。
    await a1.dblclick();
    const editor = page.locator("input.ss-grid__editor");
    await editor.fill("temp");
    await editor.press("Enter");
    await expect(a1).toHaveText("temp", { timeout: 10_000 });

    // 右键 → 清除内容。
    await a1.click({ button: "right" });
    const menu = page.locator(".ss__ctxmenu");
    await expect(menu).toBeVisible();
    await menu.getByRole("menuitem", { name: "清除内容" }).click();
    await expect(menu).toHaveCount(0);
    await expect(a1).toHaveText("", { timeout: 10_000 });
  });
});
