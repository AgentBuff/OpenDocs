import { expect, test } from "@playwright/test";
import { deleteFixture } from "../support/fixtures.js";

for (const width of [900, 1280, 1680]) {
  test(`ribbon stays within two rows and menus remain reachable at ${width}px`, async ({ page, request }) => {
    const response = await request.post("http://127.0.0.1:8788/api/artifacts", {
      data: { kind: "spreadsheet", title: "E2E toolbar layout" },
    });
    expect(response.ok()).toBeTruthy();
    const { id } = await response.json() as { id: string };
    try {
      await page.setViewportSize({ width, height: 800 });
      await page.goto(`/?doc=${id}`);
      await page.locator("[data-cell='0:0']").click();
      for (const tab of ["开始", "插入"]) {
        await page.getByRole("tab", { name: tab, exact: true }).click();
        const ribbon = page.getByRole("toolbar", { name: tab, exact: true });
        const geometry = await ribbon.evaluate(element => {
          const box = element.getBoundingClientRect();
          const buttons = Array.from(element.querySelectorAll("button")).filter(button => button.getClientRects().length > 0);
          return {
            overflow: element.scrollWidth > element.clientWidth,
            clipped: buttons.filter(button => { const b = button.getBoundingClientRect(); return b.top < box.top || b.bottom > box.bottom; }).map(button => button.title),
            overlapping: buttons.some((button, index) => buttons.slice(index + 1).some(other => {
              const a = button.getBoundingClientRect(), b = other.getBoundingClientRect();
              return Math.min(a.right, b.right) - Math.max(a.left, b.left) > 1 && Math.min(a.bottom, b.bottom) - Math.max(a.top, b.top) > 1;
            })),
          };
        });
        expect(geometry.clipped).toEqual([]);
        expect(geometry.overlapping).toBe(false);
        if (width >= 1280) expect(geometry.overflow).toBe(false);
        await ribbon.getByRole("button", { name: tab === "开始" ? "插入" : "行列", exact: true }).click();
        await expect(page.getByRole("menuitem", { name: "在右侧插入列", exact: true })).toBeVisible();
        await page.keyboard.press("Escape");
        await page.screenshot({ path: `test-results/toolbar-${width}-${tab}.png` });
      }
      await page.getByRole("tab", { name: "开始", exact: true }).click();
      await page.getByTitle("填充颜色", { exact: true }).click();
      await page.getByRole("option", { name: "#165dff", exact: true }).click();
      await expect(page.getByRole("option", { name: "#165dff", exact: true })).toBeHidden();
      await expect(page.getByTitle("填充颜色", { exact: true }).locator(".ssr__color-indicator")).toHaveCSS("background-color", "rgb(22, 93, 255)");
    } finally {
      await deleteFixture(request, { artifactId: id, revision: 1 });
    }
  });
}
