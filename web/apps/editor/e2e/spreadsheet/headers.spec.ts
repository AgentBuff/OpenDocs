import { expect, test } from "@playwright/test";

import { deleteFixture } from "../support/fixtures.js";

/**
 * 表头滚动回归：行号（左）与列标（顶）必须随视口滚动递增，并且表头条带
 * 保持在原位（剪裁层不动、内层平移）。曾经两个 bug 先后出现过——
 * ① 表头 cell 缺少 left/top 坐标导致全部堆叠在原点；② transform 加在
 * 带 overflow:hidden 的条带自身上导致滚动时整个条带滑走。此测试用真实
 * 浏览器滚动锁住两个问题。
 */

test.describe("Spreadsheet grid headers", () => {
  let artifactId: string;

  test.beforeEach(async ({ request }) => {
    const response = await request.post("http://127.0.0.1:8788/api/artifacts", {
      data: { kind: "spreadsheet", title: "E2E 表头" },
    });
    expect(response.ok()).toBeTruthy();
    artifactId = (await response.json() as { id: string }).id;
  });

  test.afterEach(async ({ request }) => {
    await deleteFixture(request, { artifactId, revision: 1 });
  });

  test("row and column headers increment while scrolling down and right", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    const grid = page.locator(".ss-grid");
    await expect(grid).toBeVisible();
    await expect(grid.locator(".ss-grid__rowhead-cell").first()).toHaveText("1");
    await expect(grid.locator(".ss-grid__colhead-cell").first()).toHaveText("A");

    const viewport = grid.locator(".ss-grid__viewport");
    await expect(viewport.locator(".ss-grid__cell").first()).toBeVisible();

    // 向下滚动 50 行：首个可见行索引 = floor(1400/28) = 50，行号显示 51。
    await viewport.evaluate((element) => {
      (element as HTMLElement).scrollTop = 50 * 28;
    });
    await expect(grid.locator(".ss-grid__rowhead-cell").first()).toHaveText("51");

    // 向右滚动 15 列。
    await viewport.evaluate((element) => {
      (element as HTMLElement).scrollLeft = 15 * 100;
    });
    await expect(grid.locator(".ss-grid__colhead-cell").first()).toHaveText("P");

    // 表头条带自身必须保持不动（剪裁层），平移发生在内层。
    await expect(grid.locator(".ss-grid__rowhead")).not.toHaveAttribute("style", /translate/);
    await expect(grid.locator(".ss-grid__colhead")).not.toHaveAttribute("style", /translate/);
    await expect(grid.locator(".ss-grid__rowhead .ss-grid__headlayer")).toHaveAttribute("style", /translateY/);
    await expect(grid.locator(".ss-grid__colhead .ss-grid__headlayer")).toHaveAttribute("style", /translateX/);
  });
});
