import { expect, test } from "@playwright/test";

import { deleteFixture } from "../support/fixtures.js";

test.describe("Presentation text and object interactions", () => {
  let artifactId: string;

  test.beforeEach(async ({ request }) => {
    const response = await request.post("http://127.0.0.1:8788/api/artifacts", {
      data: { kind: "presentation", title: "E2E presentation interactions" },
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    artifactId = (await response.json() as { id: string }).id;
  });

  test.afterEach(async ({ request }) => {
    await deleteFixture(request, { artifactId, revision: 1 });
  });

  test("persists composition-safe Unicode range and paragraph formatting as one rich-text command", async ({ page }) => {
    const transactions: Array<{ commands: Array<{ typeId: string; payload: Record<string, unknown> }> }> = [];
    page.on("request", (request) => {
      if (request.method() === "POST" && request.url().endsWith("/transactions")) {
        transactions.push(request.postDataJSON() as typeof transactions[number]);
      }
    });
    await page.goto(`/?doc=${artifactId}`);
    await page.getByRole("button", { name: "创建首张幻灯片" }).click();
    await page.getByRole("button", { name: "文本框", exact: true }).first().click();
    const textNode = page.locator(".presentation-studio__node").filter({ hasText: "双击输入文本" });
    await textNode.dblclick();
    const editor = page.getByLabel("编辑文本对象");
    await editor.dispatchEvent("compositionstart");
    await editor.fill("中文😀\n第二段");
    await editor.blur();
    const beforeCompositionEnd = transactions.filter((entry) => entry.commands.some((command) => command.typeId === "presentation.setTextContent")).length;
    expect(beforeCompositionEnd).toBe(0);
    await editor.dispatchEvent("compositionend");
    await expect.poll(() => transactions.filter((entry) => entry.commands.some((command) => command.typeId === "presentation.setTextContent")).length).toBe(1);

    await textNode.dblclick();
    await editor.evaluate((element: HTMLTextAreaElement) => {
      element.focus();
      element.setSelectionRange(0, 2);
      element.dispatchEvent(new Event("select", { bubbles: true }));
    });
    await page.getByRole("button", { name: "加粗所选文字" }).click();
    await page.getByRole("button", { name: "切换所选段落项目符号" }).click();
    await editor.press("ControlOrMeta+Enter");
    await expect.poll(() => transactions.filter((entry) => entry.commands.some((command) => command.typeId === "presentation.setTextContent")).length).toBe(2);
    const command = transactions.at(-1)!.commands.find((candidate) => candidate.typeId === "presentation.setTextContent")!;
    const body = command.payload.body as { text: string; runs: Array<{ start: number; end: number; style: { bold: boolean } }>; paragraphs: unknown[] };
    expect(body.text).toBe("中文😀\n第二段");
    expect(body.runs[0]).toMatchObject({ start: 0, end: 2, style: { bold: true } });
    expect(body.runs).toHaveLength(2);
    expect(body.paragraphs).toMatchObject([
        { start: 0, end: 4, list: { type: "bullet" } },
        { start: 4, end: 7, list: null },
    ]);
    await page.reload();
    await expect(page.locator(".presentation-studio__paragraph").first()).toContainText("中文😀");
    await expect(page.locator(".presentation-studio__paragraph").first().locator("span").filter({ hasText: "中文" })).toHaveCSS("font-weight", "700");
  });

  test("groups sibling objects and commits keyboard nudge and rotate gestures atomically", async ({ page }) => {
    const transactionCommands: Array<Array<{ typeId: string }>> = [];
    page.on("request", (request) => {
      if (request.method() === "POST" && request.url().endsWith("/transactions")) {
        transactionCommands.push((request.postDataJSON() as { commands: Array<{ typeId: string }> }).commands);
      }
    });
    await page.goto(`/?doc=${artifactId}`);
    await page.getByRole("button", { name: "创建首张幻灯片" }).click();
    await page.getByRole("button", { name: "形状", exact: true }).first().click();
    await page.getByRole("button", { name: "形状", exact: true }).first().click();
    await page.getByRole("button", { name: "形状", exact: true }).first().click();
    const nodes = page.locator(".presentation-studio__node--hit-target");
    await expect(nodes).toHaveCount(3);
    await nodes.nth(0).focus();
    await nodes.nth(0).press("Enter");
    await nodes.nth(1).focus();
    await nodes.nth(1).press("Shift+Enter");
    await page.getByRole("button", { name: "置于顶层", exact: true }).click();
    await expect.poll(() => transactionCommands.some((commands) => commands.length === 2 && commands.every((command) => command.typeId === "presentation.reorderNode"))).toBe(true);
    await expect(page.locator(".presentation-studio__status")).toContainText("已同步");
    await page.getByRole("toolbar", { name: "多对象排列工具栏" }).getByRole("button", { name: "组合对象", exact: true }).click();
    await expect.poll(() => transactionCommands.some((commands) => commands.length === 1 && commands[0]?.typeId === "presentation.groupNodes")).toBe(true);
    await expect(page.locator(".presentation-studio__status")).toContainText("已同步");
    const group = page.locator(".presentation-studio__node--hit-target").filter({ has: page.getByLabel("旋转对象") });
    await expect(group).toHaveCount(1);

    await page.locator(".presentation-studio__stage").focus();
    await page.keyboard.press("Shift+ArrowRight");
    await expect.poll(() => transactionCommands.some((commands) => commands.length === 1 && commands[0]?.typeId === "presentation.setNodeTransform")).toBe(true);
    await expect(page.locator(".presentation-studio__status")).toContainText("已同步");

    const rotate = group.getByLabel("旋转对象");
    const box = await rotate.boundingBox();
    const groupBox = await group.boundingBox();
    expect(box).not.toBeNull();
    expect(groupBox).not.toBeNull();
    await page.mouse.move(box!.x + box!.width / 2, box!.y + box!.height / 2);
    await page.mouse.down();
    await page.mouse.move(groupBox!.x + groupBox!.width + 20, groupBox!.y + groupBox!.height / 2, { steps: 8 });
    await page.mouse.up();
    await expect.poll(() => transactionCommands.filter((commands) => commands.length === 1 && commands[0]?.typeId === "presentation.setNodeTransform").length).toBeGreaterThanOrEqual(2);
    await expect(page.locator(".presentation-studio__status")).toContainText("已同步");
    await expect(page.locator(".presentation-studio__error")).toHaveCount(0);
    const lock = page.getByRole("toolbar", { name: "组合工具栏" }).getByRole("button", { name: "锁定对象" });
    await expect(lock).toBeEnabled();
    await lock.click();
    await expect.poll(() => transactionCommands.some((commands) => commands.length === 1 && commands[0]?.typeId === "presentation.setNodeLocked")).toBe(true);
    await expect(group.getByLabel("旋转对象")).toHaveCount(0);
    const transformCount = transactionCommands.filter((commands) => commands.length === 1 && commands[0]?.typeId === "presentation.setNodeTransform").length;
    await page.locator(".presentation-studio__stage").focus();
    await page.keyboard.press("ArrowRight");
    await page.waitForTimeout(100);
    expect(transactionCommands.filter((commands) => commands.length === 1 && commands[0]?.typeId === "presentation.setNodeTransform")).toHaveLength(transformCount);
    await page.reload();
    await expect(page.locator(".presentation-studio__node--hit-target")).toHaveCount(4);
    const persistedGroup = page.locator(".presentation-studio__node--hit-target[aria-label^='组合']");
    await expect(persistedGroup).toHaveCount(1);
    await persistedGroup.focus();
    await persistedGroup.press("Enter");
    await page.getByRole("toolbar", { name: "组合工具栏" }).getByRole("button", { name: "解除锁定" }).click();
    await expect(page.getByRole("toolbar", { name: "组合工具栏" }).getByRole("button", { name: "锁定对象" })).toBeEnabled();
    await page.getByRole("toolbar", { name: "组合工具栏" }).getByRole("button", { name: "打开对象属性" }).click();
    await page.locator(".presentation-studio__inspector").getByRole("button", { name: "取消组合", exact: true }).click();
    await expect(page.locator(".presentation-studio__node--hit-target")).toHaveCount(3);
    await page.getByRole("button", { name: "撤销", exact: true }).click();
    await expect(page.locator(".presentation-studio__node--hit-target[aria-label^='组合']")).toHaveCount(1);
  });

  test("consumes vertical alignment and shrink-text auto-fit in the stage renderer", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    await page.getByRole("button", { name: "创建首张幻灯片" }).click();
    await page.getByRole("button", { name: "文本框", exact: true }).first().click();
    const node = page.locator(".presentation-studio__stage .presentation-studio__node").filter({ hasText: "双击输入文本" });
    await node.dblclick();
    await page.getByLabel("编辑文本对象").fill("这是用于验证溢出后缩小文字的长段落。".repeat(80));
    await page.getByLabel("编辑文本对象").press("ControlOrMeta+Enter");
    await expect(page.locator(".presentation-studio__status")).toContainText("已同步");
    await page.getByRole("toolbar", { name: "文本工具栏" }).getByRole("button", { name: "打开对象属性" }).click();
    await page.getByLabel("垂直对齐").selectOption("bottom");
    await page.getByLabel("自动适应").selectOption("shrinkText");
    await page.getByRole("button", { name: "应用文本框" }).click();
    const frame = page.locator(".presentation-studio__stage .presentation-studio__text-content");
    await expect(frame).toHaveCSS("justify-content", "flex-end");
    await expect.poll(() => frame.locator(".presentation-studio__text-fit-content").evaluate((element) => element.getAttribute("style"))).not.toContain("scale(1)");
    await page.reload();
    await expect(frame).toHaveCSS("justify-content", "flex-end");
    await expect.poll(() => frame.locator(".presentation-studio__text-fit-content").evaluate((element) => element.getAttribute("style"))).not.toContain("scale(1)");
  });
});
