import { expect, test, type Locator, type Page } from "@playwright/test";
import { resolve } from "node:path";
import { readFile } from "node:fs/promises";
import { deleteFixture } from "../support/fixtures.js";

async function performMindmapTransaction(page: Page, action: () => Promise<unknown>) {
  const responsePromise = page.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/transactions"));
  await action();
  expect((await responsePromise).ok()).toBeTruthy();
  await expect(page.locator(".mindmap-sync")).toContainText("已保存");
}

async function clickPathMidpoint(page: Page, path: Locator) {
  const point = await path.evaluate((element) => {
    const curve = element as SVGPathElement;
    const local = curve.getPointAtLength(curve.getTotalLength() / 2);
    const matrix = curve.getScreenCTM();
    if (!matrix) throw new Error("SVG path 没有 screen transform");
    return { x: matrix.a * local.x + matrix.c * local.y + matrix.e, y: matrix.b * local.x + matrix.d * local.y + matrix.f };
  });
  const hit = await page.evaluate(({ x, y }) => {
    const element = document.elementFromPoint(x, y);
    return { tag: element?.tagName ?? null, className: element?.getAttribute("class") ?? null };
  }, point);
  expect(hit, `projection path midpoint must hit its transparent target at ${point.x},${point.y}`).toMatchObject({ tag: "path", className: "mindmap-link-hit" });
  await page.mouse.click(point.x, point.y);
}

async function dragEdgeEndpoint(page: Page, handle: Locator, target: Locator) {
  const handleBox = await handle.boundingBox();
  const targetBox = await target.boundingBox();
  expect(handleBox).not.toBeNull();
  expect(targetBox).not.toBeNull();
  const responsePromise = page.waitForResponse((response) => response.request().method() === "POST" && response.url().endsWith("/transactions"));
  await page.mouse.move(handleBox!.x + handleBox!.width / 2, handleBox!.y + handleBox!.height / 2);
  await page.mouse.down();
  await page.mouse.move(targetBox!.x + targetBox!.width / 2, targetBox!.y + targetBox!.height / 2, { steps: 6 });
  await expect(target).toHaveClass(/is-edge-target/);
  await page.mouse.up();
  expect((await responsePromise).ok()).toBeTruthy();
  await expect(page.locator(".mindmap-sync")).toContainText("已保存");
}

test.describe("Mindmap canonical editing", () => {
  let artifactId: string;

  test.beforeEach(async ({ request }) => {
    const response = await request.post("http://127.0.0.1:8788/api/artifacts", {
      data: { kind: "mindmap", title: "E2E 思维脑图" },
    });
    expect(response.ok()).toBeTruthy();
    artifactId = (await response.json() as { id: string }).id;
  });

  test.afterEach(async ({ request }) => {
    await deleteFixture(request, { artifactId, revision: 1 });
  });

  test("creates, restructures, styles and restores topics through semantic history", async ({ page, request }) => {
    await page.goto(`/?doc=${artifactId}`);
    await expect(page.locator(".mindmap-studio")).toBeVisible();
    await page.getByRole("button", { name: "中心主题" }).click();
    const editor = page.getByRole("textbox", { name: "主题文字" });
    await expect(editor).toBeVisible();
    await editor.fill("产品路线图");
    await editor.press("Enter");
    await expect(page.locator(".mindmap-node")).toContainText("产品路线图");

    await page.keyboard.press("Tab");
    await expect(editor).toBeVisible();
    await editor.fill("第一个里程碑");
    await editor.press("Enter");
    await expect(page.locator(".mindmap-node")).toHaveCount(2);

    await page.getByLabel("形状").selectOption("pill");
    await expect(page.locator(".mindmap-node.is-selected")).toHaveClass(/mindmap-node--pill/);

    await page.keyboard.press("ControlOrMeta+z");
    await expect(page.locator(".mindmap-node.is-selected")).not.toHaveClass(/mindmap-node--pill/);
    await page.keyboard.press("ControlOrMeta+Shift+z");
    await expect(page.locator(".mindmap-node.is-selected")).toHaveClass(/mindmap-node--pill/);

    const markdown = await request.get(`http://127.0.0.1:8788/api/artifacts/${artifactId}/export/md`);
    expect(markdown.ok()).toBeTruthy();
    expect(await markdown.text()).toContain("产品路线图");
    const json = await request.get(`http://127.0.0.1:8788/api/artifacts/${artifactId}/export/json`);
    expect(json.ok()).toBeTruthy();
    expect(await json.json()).toMatchObject({ format: "open-office-mindmap", version: 1, model: { root: expect.any(String) }, assets: [] });
  });

  test("downloads canonical exports and reimports the JSON backup from the browser", async ({ page, request }) => {
    await page.goto(`/?doc=${artifactId}`);
    await page.getByRole("button", { name: "新建中心节点" }).click();
    await page.getByRole("textbox", { name: "主题文字" }).fill("可移植脑图");
    await page.getByRole("textbox", { name: "主题文字" }).press("Enter");
    await page.getByRole("button", { name: "导出", exact: true }).click();
    const jsonDownload = page.waitForEvent("download");
    await page.getByRole("menuitem", { name: /JSON/ }).click();
    const downloaded = await jsonDownload;
    expect(downloaded.suggestedFilename()).toMatch(/\.mindmap\.json$/);
    const downloadPath = await downloaded.path();
    expect(downloadPath).not.toBeNull();

    for (const name of [/SVG/, /PDF.*适合单页/]) {
      await page.getByRole("button", { name: "导出", exact: true }).click();
      const download = page.waitForEvent("download");
      await page.getByRole("menuitem", { name }).click();
      expect((await download).suggestedFilename()).toMatch(name.source.includes("SVG") ? /\.svg$/ : /\.pdf$/);
    }

    await page.goto("/");
    await page.getByLabel("导入策略").selectOption("audit");
    await page.locator('input[type="file"]').setInputFiles({ name: downloaded.suggestedFilename(), mimeType: "application/json", buffer: await readFile(downloadPath!) });
    await expect(page.locator(".mindmap-studio")).toBeVisible();
    await expect(page.locator(".mindmap-node")).toContainText("可移植脑图");
    const copyId = new URL(page.url()).searchParams.get("doc");
    expect(copyId).toBeTruthy();
    await request.delete(`http://127.0.0.1:8788/api/artifacts/${copyId}`);
  });

  test("strict browser import rejects a lossy FreeMind file before persistence", async ({ page }) => {
    await page.goto("/");
    await page.getByLabel("导入策略").selectOption("strict");
    await page.locator('input[type="file"]').setInputFiles(resolve(process.cwd(), "../fixtures/mindmap/freemind/basic.mm"));
    await expect(page.locator(".alert")).toContainText("strict 导入拒绝有损内容");
    await expect(page.locator(".mindmap-studio")).toHaveCount(0);
  });

  test("pushes remote revisions and clears a selection deleted by another browser", async ({ page, browser }) => {
    await page.goto(`/?doc=${artifactId}`);
    await page.getByRole("button", { name: "新建中心节点" }).click();
    await page.getByRole("textbox", { name: "主题文字" }).fill("中心");
    await page.getByRole("textbox", { name: "主题文字" }).press("Enter");
    await page.getByRole("button", { name: "子主题", exact: true }).click();
    await page.getByRole("textbox", { name: "主题文字" }).fill("远端删除目标");
    await page.getByRole("textbox", { name: "主题文字" }).press("Enter");

    const remote = await browser.newPage();
    await remote.goto(`/?doc=${artifactId}`);
    await remote.locator(".mindmap-node").filter({ hasText: "远端删除目标" }).click();
    await expect(remote.locator(".mindmap-node.is-selected")).toContainText("远端删除目标");
    await page.locator(".mindmap-node").filter({ hasText: "远端删除目标" }).click();
    await page.getByRole("button", { name: "删除", exact: true }).click();
    await expect(remote.locator(".mindmap-node").filter({ hasText: "远端删除目标" })).toHaveCount(0, { timeout: 10_000 });
    await expect(remote.locator(".mindmap-node.is-selected")).toHaveCount(0);
    await remote.close();
  });

  test("keeps a 10k map DOM-bounded while selected and edited topics stay mounted", async ({ page, request }) => {
    test.setTimeout(60_000);
    const style = { shape: "roundedRectangle", fillColor: null, borderColor: null, textColor: null, borderWidth: 1, textAlign: "start", minWidth: 96, maxWidth: 320 };
    const supplement = { note: null, hyperlink: null, image: null, markers: [] };
    const nodes = Array.from({ length: 10_000 }, (_, index) => ({
      id: `node-${index}`,
      parentId: index === 0 ? null : "node-0",
      content: { text: `Topic ${index}`, runs: [] },
      style,
      supplement,
      attrs: {},
      collapsed: false,
    }));
    const portable = {
      format: "open-office-mindmap",
      version: 1,
      schemaVersion: 10,
      model: { settings: { layout: "logicalRight", themeId: null, connector: { shape: "orthogonal", color: null, width: 2, dashed: false } }, root: "node-0", nodes, edges: [], summaries: [], boundaries: [], formulas: [] },
      assets: [],
    };
    const imported = await request.post("http://127.0.0.1:8788/api/artifacts/import", {
      multipart: { file: { name: "large.mindmap.json", mimeType: "application/json", buffer: Buffer.from(JSON.stringify(portable)) }, mode: "audit" },
    });
    expect(imported.ok()).toBeTruthy();
    const largeId = (await imported.json() as { id: string }).id;
    await page.goto(`/?doc=${largeId}`);
    await expect(page.locator(".mindmap-studio")).toBeVisible({ timeout: 30_000 });
    await expect.poll(() => page.locator(".mindmap-node").count()).toBeLessThan(250);
    await page.getByLabel("搜索主题", { exact: true }).fill("Topic 9999");
    await page.getByLabel("下一个结果").click();
    await expect(page.locator(".mindmap-node.is-selected")).toContainText("Topic 9999");
    expect(await page.locator(".mindmap-node").count()).toBeLessThan(250);
    await page.locator(".mindmap-node.is-selected").dispatchEvent("dblclick");
    await expect(page.getByRole("textbox", { name: "主题文字" })).toHaveCount(1);
    await page.getByRole("button", { name: "放大", exact: true }).click();
    expect(await page.locator(".mindmap-node").count()).toBeLessThan(250);
    await request.delete(`http://127.0.0.1:8788/api/artifacts/${largeId}`);
  });
  test("empty canvas creates a central node, deletes, restores and recreates after refresh", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    await expect(page.getByRole("heading", { name: "从一个中心主题开始" })).toBeVisible();
    await page.getByRole("button", { name: "新建中心节点" }).click();
    const editor = page.getByRole("textbox", { name: "主题文字" });
    await expect(editor).toHaveText("中心主题");
    await editor.fill("我的中心");
    await editor.press("Enter");
    await expect(page.getByRole("button", { name: "删除", exact: true })).toBeEnabled();
    await page.getByRole("button", { name: "删除", exact: true }).click();
    await expect(page.getByRole("button", { name: "新建中心节点" })).toBeVisible();
    await page.getByRole("button", { name: "撤销", exact: true }).click();
    await expect(page.locator(".mindmap-node > span").first()).toHaveText("我的中心");
    await page.getByRole("button", { name: "重做", exact: true }).click();
    await expect(page.locator(".mindmap-node")).toHaveCount(0);
    await page.reload();
    await page.getByLabel("脑图画布", { exact: true }).focus();
    await page.keyboard.press("Enter");
    await expect(editor).toBeVisible();
    await editor.fill("重新开始");
    await editor.press("Enter");
    await expect(page.getByRole("button", { name: "删除", exact: true })).toBeEnabled();
    await page.reload();
    await expect(page.locator(".mindmap-node > span").first()).toHaveText("重新开始");
  });

  test("Tab creates the first node and adding to a collapsed branch reveals the editor", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    await page.getByLabel("脑图画布", { exact: true }).focus();
    await page.keyboard.press("Tab");
    const editor = page.getByRole("textbox", { name: "主题文字" });
    await expect(editor).toBeVisible();
    await editor.press("Enter");
    await page.getByRole("button", { name: "子主题", exact: true }).click();
    await editor.fill("原有分支");
    await editor.press("Enter");
    await expect(page.getByRole("button", { name: "子主题", exact: true })).toBeEnabled();
    await page.getByLabel("折叠分支", { exact: true }).click();
    await expect(page.locator(".mindmap-node")).toHaveCount(1);
    await page.locator(".mindmap-node").click();
    await page.getByRole("button", { name: "子主题", exact: true }).click();
    await expect(editor).toBeInViewport();
    await editor.fill("可见的新分支");
    await editor.press("Enter");
    await expect(page.locator(".mindmap-node")).toHaveCount(3);
    await expect(page.getByRole("button", { name: "撤销", exact: true })).toBeEnabled();
    await page.getByRole("button", { name: "撤销", exact: true }).click();
    await expect(page.locator(".mindmap-node").filter({ hasText: "新主题" })).toHaveCount(1);
    await page.getByRole("button", { name: "撤销", exact: true }).click();
    await expect(page.locator(".mindmap-node")).toHaveCount(1);
    await expect(page.getByLabel("展开分支", { exact: true })).toBeVisible();
  });

  test("search reveals collapsed descendants and text styles compose", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    await page.getByRole("button", { name: "新建中心节点" }).click();
    const editor = page.getByRole("textbox", { name: "主题文字" });
    await editor.press("Enter");
    await page.getByRole("button", { name: "子主题", exact: true }).click();
    await editor.fill("需要找到的分支");
    await editor.press("Enter");
    await expect(page.getByRole("button", { name: "子主题", exact: true })).toBeEnabled();
    await page.getByLabel("折叠分支", { exact: true }).click();
    await expect(page.locator(".mindmap-node")).toHaveCount(1);
    await page.getByLabel("搜索主题", { exact: true }).fill("需要找到");
    await page.getByLabel("下一个结果").click();
    await expect(page.locator(".mindmap-node.is-selected")).toContainText("需要找到的分支");
    await expect(page.locator(".mindmap-node.is-selected")).toBeInViewport();
    await page.getByRole("button", { name: "粗体", exact: true }).click();
    await expect(page.getByRole("button", { name: "斜体", exact: true })).toBeEnabled();
    await page.getByRole("button", { name: "斜体", exact: true }).click();
    await expect(page.locator(".mindmap-node.is-selected span").first()).toHaveCSS("font-weight", "700");
    await expect(page.locator(".mindmap-node.is-selected span").first()).toHaveCSS("font-style", "italic");
    await page.reload();
    await expect(page.locator(".mindmap-node").filter({ hasText: "需要找到的分支" })).toBeVisible();
  });

  test("sibling topics, clipboard, relationships and all layout choices remain editable", async ({ page, request }) => {
    await page.goto(`/?doc=${artifactId}`);
    await page.getByRole("button", { name: "新建中心节点" }).click();
    const editor = page.getByRole("textbox", { name: "主题文字" });
    await editor.press("Enter");
    await page.getByRole("button", { name: "子主题", exact: true }).click();
    await editor.fill("分支 A");
    await editor.press("Enter");
    await page.getByRole("button", { name: "同级主题", exact: true }).click();
    await editor.fill("分支 B");
    await editor.press("Enter");
    await expect(page.getByRole("button", { name: "同级主题", exact: true })).toBeEnabled();
    await page.locator(".mindmap-node").filter({ hasText: "分支 A" }).click();
    await page.keyboard.press("ControlOrMeta+c");
    await page.locator(".mindmap-node").filter({ hasText: "分支 B" }).click();
    await page.keyboard.press("ControlOrMeta+v");
    await expect(page.locator(".mindmap-node")).toHaveCount(4);
    await page.locator(".mindmap-node").filter({ hasText: "分支 B" }).click({ modifiers: ["ControlOrMeta"] });
    await page.getByRole("button", { name: "添加关联", exact: true }).click();
    await expect(page.locator(".mindmap-link--explicit")).toHaveCount(1);
    for (const layout of ["logicalLeft", "mindMap", "organization", "catalog", "timelineHorizontal", "timelineVertical", "fishbone", "logicalRight"]) {
      await page.getByLabel("布局", { exact: true }).selectOption(layout);
      await expect(page.getByLabel("布局", { exact: true })).toBeEnabled();
      await expect(page.getByLabel("布局", { exact: true })).toHaveValue(layout);
      await expect(page.locator(".mindmap-node")).toHaveCount(4);
    }
    for (const shape of ["curve", "straight", "orthogonal"]) {
      await page.getByLabel("连线", { exact: true }).selectOption(shape);
      await expect(page.getByLabel("连线", { exact: true })).toBeEnabled();
      await expect(page.getByLabel("连线", { exact: true })).toHaveValue(shape);
    }
    await page.reload();
    await expect(page.locator(".mindmap-node")).toHaveCount(4);
    await expect(page.locator(".mindmap-link--explicit")).toHaveCount(1);
    const response = await request.get(`http://127.0.0.1:8788/api/artifacts/${artifactId}`);
    expect(response.ok()).toBeTruthy();
    await expect(page.locator(".mindmap-error")).toHaveCount(0);
  });

  test("selects and edits relationship endpoints, label and style with reversible cleanup", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    await page.getByRole("button", { name: "新建中心节点" }).click();
    const editor = page.getByRole("textbox", { name: "主题文字" });
    await editor.fill("中心");
    await editor.press("Enter");
    await page.getByRole("button", { name: "子主题", exact: true }).click();
    await editor.fill("分支 A");
    await editor.press("Enter");
    await page.getByRole("button", { name: "同级主题", exact: true }).click();
    await editor.fill("分支 B");
    await editor.press("Enter");

    await page.locator(".mindmap-node").filter({ hasText: "分支 A" }).click();
    await page.locator(".mindmap-node").filter({ hasText: "分支 B" }).click({ modifiers: ["ControlOrMeta"] });
    await page.getByRole("button", { name: "添加关联", exact: true }).click();
    const edge = page.getByRole("button", { name: "关联线：分支 A 到 分支 B" });
    await expect(edge).toHaveAttribute("aria-pressed", "true");
    await page.keyboard.press("Escape");
    await clickPathMidpoint(page, edge);
    await expect(page.getByRole("heading", { name: "所选关联线" })).toBeVisible();

    await page.getByLabel("关联线标签").fill("依赖关系");
    await performMindmapTransaction(page, () => page.getByLabel("关联线标签").press("Tab"));
    await expect(page.getByText("依赖关系", { exact: true })).toBeVisible();
    await performMindmapTransaction(page, () => page.getByLabel("关联线线型").selectOption("curve"));
    await expect(page.getByLabel("关联线线型")).toHaveValue("curve");
    await performMindmapTransaction(page, () => page.getByLabel("关联线颜色").fill("#2457c5"));
    await performMindmapTransaction(page, () => page.getByLabel("关联线宽度").fill("4"));
    await performMindmapTransaction(page, () => page.getByLabel("关联线虚线").click());
    await expect(page.getByLabel("关联线虚线")).toBeChecked();
    await dragEdgeEndpoint(page, page.getByRole("button", { name: "拖动关联线起点" }), page.locator(".mindmap-node").filter({ hasText: "中心" }));
    await expect(page.getByRole("button", { name: "关联线：中心 到 分支 B" })).toBeVisible();

    await page.reload();
    const persistedEdge = page.getByRole("button", { name: "关联线：中心 到 分支 B" });
    await persistedEdge.focus();
    await persistedEdge.press("Enter");
    await expect(page.getByLabel("关联线标签")).toHaveValue("依赖关系");
    await expect(page.getByLabel("关联线线型")).toHaveValue("curve");
    await expect(page.getByLabel("关联线宽度")).toHaveValue("4");
    await expect(page.getByLabel("关联线虚线")).toBeChecked();

    await persistedEdge.focus();
    await page.keyboard.press("Delete");
    await expect(page.locator(".mindmap-link-hit")).toHaveCount(0);
    await page.getByRole("button", { name: "撤销", exact: true }).click();
    await expect(page.getByRole("button", { name: "关联线：中心 到 分支 B" })).toBeVisible();
    await page.locator(".mindmap-node").filter({ hasText: "分支 B" }).click();
    await page.getByRole("button", { name: "删除", exact: true }).click();
    await expect(page.locator(".mindmap-link-hit")).toHaveCount(0);
    await page.getByRole("button", { name: "撤销", exact: true }).click();
    await expect(page.getByRole("button", { name: "关联线：中心 到 分支 B" })).toBeVisible();
  });

  test("notes, links, markers and image assets persist after refresh", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    await page.getByRole("button", { name: "新建中心节点" }).click();
    await page.getByRole("textbox", { name: "主题文字" }).press("Enter");
    const note = page.getByLabel("备注", { exact: true });
    await note.fill("项目说明");
    await note.blur();
    await expect(note).toBeEnabled();
    await page.getByLabel("链接", { exact: true }).fill("https://example.com/");
    await page.getByLabel("链接", { exact: true }).blur();
    await expect(page.locator(".mindmap-node a")).toHaveAttribute("href", "https://example.com/");
    await page.getByLabel("标记", { exact: true }).fill("重点, 待办");
    await page.getByLabel("标记", { exact: true }).blur();
    await expect(page.locator(".mindmap-node")).toContainText("待办");
    await page.getByLabel("图片", { exact: true }).setInputFiles({ name: "pixel.png", mimeType: "image/png", buffer: Buffer.from("iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+aP1sAAAAASUVORK5CYII=", "base64") });
    await expect(page.locator(".mindmap-node img")).toBeVisible();
    await page.reload();
    await page.locator(".mindmap-node").click();
    await expect(note).toHaveValue("项目说明");
    await expect(page.getByLabel("链接", { exact: true })).toHaveValue("https://example.com/");
    await expect(page.getByLabel("标记", { exact: true })).toHaveValue("重点, 待办");
    await expect(page.locator(".mindmap-node img")).toBeVisible();
    await page.getByRole("button", { name: "移除图片" }).click();
    await expect(page.locator(".mindmap-node img")).toHaveCount(0);
    await expect(page.locator(".mindmap-error")).toHaveCount(0);
  });

  test("toolbar stays accessible on narrow screens and export and zoom controls work", async ({ page }, testInfo) => {
    for (const width of [390, 768, 1280]) {
      await page.setViewportSize({ width, height: 844 });
      await page.goto(`/?doc=${artifactId}`);
      const toolbar = page.getByRole("toolbar", { name: "思维脑图工具栏" });
      await expect(toolbar).toBeVisible();
      for (const button of await toolbar.getByRole("button").all()) {
        const box = await button.boundingBox();
        expect(box).not.toBeNull();
        expect(box!.x).toBeGreaterThanOrEqual(0);
        expect(box!.x + box!.width).toBeLessThanOrEqual(width);
      }
      if (width <= 900) {
        await expect(page.getByLabel("布局", { exact: true })).toHaveCount(0);
        await page.getByRole("button", { name: "格式", exact: true }).click();
        await expect(page.getByLabel("布局", { exact: true })).toBeVisible();
        await page.getByRole("button", { name: "格式", exact: true }).click();
        await expect(page.getByLabel("布局", { exact: true })).toHaveCount(0);
      }
      await page.getByRole("button", { name: "导出", exact: true }).click();
      await expect(page.getByRole("menuitem", { name: "Markdown 层级文本" })).toBeVisible();
      const menu = await page.getByRole("menu").boundingBox();
      expect(menu!.x).toBeGreaterThanOrEqual(0);
      expect(menu!.x + menu!.width).toBeLessThanOrEqual(width);
      await page.keyboard.press("Escape");
      await expect(page.getByRole("menu")).toHaveCount(0);
      await page.getByRole("button", { name: "放大", exact: true }).click();
      await expect(page.getByRole("button", { name: "重置缩放" })).toHaveText("110%");
      await page.getByRole("button", { name: "重置缩放" }).click();
      await expect(page.getByRole("button", { name: "重置缩放" })).toHaveText("100%");
      await page.screenshot({ path: testInfo.outputPath(`toolbar-${width}.png`) });
    }
  });

  test("toolbar Tab navigation does not create topics and IME Enter does not save early", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    await page.getByRole("button", { name: "中心主题", exact: true }).focus();
    await page.keyboard.press("Tab");
    await expect(page.locator(".mindmap-node")).toHaveCount(0);
    await page.getByRole("button", { name: "新建中心节点" }).click();
    const editor = page.getByRole("textbox", { name: "主题文字" });
    await editor.fill("中文输入");
    await editor.dispatchEvent("keydown", { key: "Enter", code: "Enter", isComposing: true, bubbles: true });
    await expect(editor).toBeVisible();
    await editor.press("Enter");
    await expect(page.locator(".mindmap-node > span").first()).toHaveText("中文输入");
    await page.getByRole("button", { name: "粗体", exact: true }).click();
    await expect(page.getByRole("button", { name: "斜体", exact: true })).toBeEnabled();
    await page.locator(".mindmap-node").dblclick();
    await editor.fill("修改后😀");
    await editor.press("Enter");
    await expect(page.locator(".mindmap-node > span").first()).toHaveCSS("font-weight", "700");
    await expect(page.getByRole("button", { name: "粗体", exact: true })).toBeEnabled();
    await page.reload();
    await expect(page.locator(".mindmap-node > span").first()).toHaveCSS("font-weight", "700");
  });

  test("edits a Unicode range and defers a blurred IME draft until composition ends", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    await page.getByRole("button", { name: "新建中心节点" }).click();
    const editor = page.getByRole("textbox", { name: "主题文字" });
    await editor.fill("A😀中Z");
    await editor.press("Enter");
    await page.locator(".mindmap-node").dblclick();
    await editor.evaluate((element) => {
      const root = element as HTMLElement;
      const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
      const boundary = (wanted: number) => {
        let scalar = 0;
        let node: Text | null;
        while ((node = walker.nextNode() as Text | null)) {
          const characters = [...node.data];
          if (scalar + characters.length >= wanted) {
            return { node, offset: characters.slice(0, wanted - scalar).join("").length };
          }
          scalar += characters.length;
        }
        throw new Error(`找不到 scalar boundary ${wanted}`);
      };
      const start = boundary(1);
      walker.currentNode = root;
      const end = boundary(3);
      const range = document.createRange();
      range.setStart(start.node, start.offset);
      range.setEnd(end.node, end.offset);
      const selection = window.getSelection()!;
      selection.removeAllRanges();
      selection.addRange(range);
      root.dispatchEvent(new PointerEvent("pointerup", { bubbles: true }));
    });
    await performMindmapTransaction(page, () => page.getByRole("button", { name: "加粗所选主题文字" }).click());
    await expect(editor.locator("strong")).toHaveText("😀中");
    await editor.press("Escape");
    await expect(page.locator(".mindmap-node__text span").filter({ hasText: "😀中" })).toHaveCSS("font-weight", "700");

    await page.locator(".mindmap-node").dblclick();
    let transactions = 0;
    page.on("request", (request) => { if (request.method() === "POST" && request.url().endsWith("/transactions")) transactions += 1; });
    await editor.dispatchEvent("compositionstart", { data: "" });
    await editor.evaluate((element) => {
      element.textContent = "中文🙂";
      element.dispatchEvent(new InputEvent("input", { bubbles: true, inputType: "insertCompositionText", data: "中文🙂", isComposing: true }));
    });
    await page.getByRole("button", { name: "大纲", exact: true }).click();
    expect(transactions).toBe(0);
    const response = page.waitForResponse((item) => item.request().method() === "POST" && item.url().endsWith("/transactions"));
    await editor.dispatchEvent("compositionend", { data: "中文🙂" });
    expect((await response).ok()).toBeTruthy();
    expect(transactions).toBe(1);
    await expect(page.locator(".mindmap-node")).toContainText("中文🙂");
    await page.reload();
    await expect(page.locator(".mindmap-node")).toContainText("中文🙂");
  });

  test("creates, edits, reloads and structurally cleans summaries, boundaries and formulas", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    await page.getByRole("button", { name: "新建中心节点" }).click();
    const editor = page.getByRole("textbox", { name: "主题文字" });
    await editor.fill("中心");
    await editor.press("Enter");
    await page.getByRole("button", { name: "子主题", exact: true }).click();
    await editor.fill("分支 A");
    await editor.press("Enter");
    await page.getByRole("button", { name: "同级主题", exact: true }).click();
    await editor.fill("分支 B");
    await editor.press("Enter");

    const branchA = page.locator(".mindmap-node").filter({ hasText: "分支 A" });
    const branchB = page.locator(".mindmap-node").filter({ hasText: "分支 B" });
    await branchA.click();
    await branchB.click({ modifiers: ["ControlOrMeta"] });
    await performMindmapTransaction(page, () => page.getByRole("button", { name: "添加概要" }).click());
    await expect(page.getByRole("button", { name: "概要：概要" })).toBeVisible();
    await page.getByLabel("概要标签").fill("阶段结论");
    await performMindmapTransaction(page, () => page.getByLabel("概要标签").press("Tab"));
    await expect(page.getByRole("button", { name: "概要：阶段结论" })).toBeVisible();

    await branchA.click();
    await performMindmapTransaction(page, () => page.getByRole("button", { name: "添加外框" }).click());
    await page.getByLabel("外框标签").fill("重点范围");
    await performMindmapTransaction(page, () => page.getByLabel("外框标签").press("Tab"));
    await expect(page.getByRole("button", { name: "外框：重点范围" })).toBeVisible();

    await branchA.click();
    await performMindmapTransaction(page, () => page.getByRole("button", { name: "添加公式" }).click());
    await page.getByLabel("公式 LaTeX").fill("E=mc^2");
    await performMindmapTransaction(page, () => page.getByLabel("公式 LaTeX").press("Tab"));
    await performMindmapTransaction(page, () => page.getByLabel("公式显示方式").selectOption("block"));
    await expect(page.getByRole("button", { name: "公式：E=mc^2" })).toContainText("$$E=mc^2$$");

    await page.reload();
    await expect(page.getByRole("button", { name: "概要：阶段结论" })).toBeVisible();
    await expect(page.getByRole("button", { name: "外框：重点范围" })).toBeVisible();
    await expect(page.getByRole("button", { name: "公式：E=mc^2" })).toBeVisible();

    await page.locator(".mindmap-node").filter({ hasText: "分支 A" }).click();
    await page.keyboard.press("ControlOrMeta+c");
    await page.locator(".mindmap-node").filter({ hasText: "中心" }).click();
    await performMindmapTransaction(page, () => page.keyboard.press("ControlOrMeta+v"));
    await expect(page.getByRole("button", { name: "外框：重点范围" })).toHaveCount(2);
    await expect(page.getByRole("button", { name: "公式：E=mc^2" })).toHaveCount(2);
    await expect(page.getByRole("button", { name: "概要：阶段结论" })).toHaveCount(1);

    await page.locator(".mindmap-node").filter({ hasText: "分支 A" }).first().click();
    await performMindmapTransaction(page, () => page.getByRole("button", { name: "删除", exact: true }).click());
    await expect(page.getByRole("button", { name: "概要：阶段结论" })).toHaveCount(0);
    await expect(page.getByRole("button", { name: "外框：重点范围" })).toHaveCount(1);
    await expect(page.getByRole("button", { name: "公式：E=mc^2" })).toHaveCount(1);
    await page.getByRole("button", { name: "撤销", exact: true }).click();
    await expect(page.getByRole("button", { name: "概要：阶段结论" })).toBeVisible();
    await expect(page.getByRole("button", { name: "外框：重点范围" })).toHaveCount(2);
    await expect(page.getByRole("button", { name: "公式：E=mc^2" })).toHaveCount(2);
  });

  test("search supports Enter and Shift Enter without adding siblings", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    await page.getByRole("button", { name: "新建中心节点" }).click();
    const editor = page.getByRole("textbox", { name: "主题文字" });
    await editor.fill("查找 A");
    await editor.press("Enter");
    await page.getByRole("button", { name: "子主题", exact: true }).click();
    await editor.fill("查找 B");
    await editor.press("Enter");
    const search = page.getByLabel("搜索主题", { exact: true });
    await search.fill("查找");
    await expect(page.locator(".mindmap-search output")).toHaveText("0/2");
    await search.press("Shift+Enter");
    await expect(page.locator(".mindmap-search output")).toHaveText("2/2");
    await expect(page.locator(".mindmap-node.is-selected > span").first()).toHaveText("查找 B");
    await search.press("Enter");
    await expect(page.locator(".mindmap-search output")).toHaveText("1/2");
    await expect(page.locator(".mindmap-node.is-selected")).toContainText("查找 A");
    await search.press("Escape");
    await expect(search).toHaveValue("");
    await expect(page.locator(".mindmap-node")).toHaveCount(2);
  });

  test("map themes persist, undo and preserve individually styled topics", async ({ page }, testInfo) => {
    await page.goto(`/?doc=${artifactId}`);
    await page.getByRole("button", { name: "新建中心节点" }).click();
    const editor = page.getByRole("textbox", { name: "主题文字" });
    await editor.fill("产品计划");
    await editor.press("Enter");
    await page.getByRole("button", { name: "子主题", exact: true }).click();
    await editor.fill("设计方向");
    await editor.press("Enter");
    await expect(page.getByLabel("填充", { exact: true })).toBeEnabled();
    await page.getByLabel("填充", { exact: true }).evaluate((element: HTMLInputElement) => { Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(element, "#fff0cc"); element.dispatchEvent(new Event("input", { bubbles: true })); element.dispatchEvent(new Event("change", { bubbles: true })); });
    await expect(page.locator(".mindmap-node--branch")).toHaveCSS("background-color", "rgb(255, 240, 204)");
    await page.getByRole("button", { name: "主题风格", exact: true }).click();
    await page.getByRole("button", { name: "青竹绿", exact: true }).click();
    await expect(page.locator(".mindmap-node--root")).toHaveCSS("background-color", "rgb(40, 118, 93)");
    await expect(page.locator(".mindmap-node--branch")).toHaveCSS("background-color", "rgb(255, 240, 204)");
    await page.getByRole("button", { name: "撤销", exact: true }).click();
    await expect(page.locator(".mindmap-node--root")).toHaveCSS("background-color", "rgb(50, 101, 217)");
    await page.getByRole("button", { name: "重做", exact: true }).click();
    await expect(page.locator(".mindmap-node--root")).toHaveCSS("background-color", "rgb(40, 118, 93)");
    await page.reload();
    await expect(page.locator(".mindmap-node--root")).toHaveCSS("background-color", "rgb(40, 118, 93)");
    // Quick add must target the hovered root even when its child is selected.
    await page.locator(".mindmap-node--branch").click();
    await page.locator(".mindmap-node--root").hover();
    await page.locator(".mindmap-node--root").getByRole("button", { name: "快捷添加子主题" }).click();
    await expect(editor).toBeVisible();
    await editor.fill("工程实现");
    await editor.press("Enter");
    await expect(page.locator(".mindmap-node--branch")).toHaveCount(2);
    await page.getByRole("button", { name: "主题风格", exact: true }).click();
    await expect(page.getByRole("button", { name: "青竹绿", exact: true })).toHaveAttribute("aria-pressed", "true");
    await page.screenshot({ path: testInfo.outputPath("theme-picker.png") });
    await page.keyboard.press("Escape");
    await page.locator(".mindmap-node").filter({ hasText: "设计方向" }).click();
    await expect(page.getByLabel("填充", { exact: true })).toHaveValue("#fff0cc");
    await page.getByRole("button", { name: "恢复主题配色" }).click();
    await expect(page.locator(".mindmap-node.is-selected")).not.toHaveCSS("background-color", "rgb(255, 240, 204)");
    await expect(page.getByRole("button", { name: "恢复主题配色" })).toBeDisabled();
  });

  test("modifier wheel zoom keeps the pointed canvas location stationary", async ({ page }) => {
    await page.goto(`/?doc=${artifactId}`);
    await page.getByRole("button", { name: "新建中心节点" }).click();
    await page.getByRole("textbox", { name: "主题文字" }).press("Enter");
    const root = page.locator(".mindmap-node--root");
    await expect.poll(async () => {
      const node = await root.boundingBox();
      const viewport = await page.getByLabel("脑图画布", { exact: true }).boundingBox();
      return Math.abs(node!.x + node!.width / 2 - viewport!.x - viewport!.width / 2);
    }).toBeLessThan(1);
    const before = await root.boundingBox();
    const x = before!.x + before!.width / 2;
    const y = before!.y + before!.height / 2;
    await page.mouse.move(x, y);
    await page.keyboard.down("Control");
    await page.mouse.wheel(0, -100);
    await page.keyboard.up("Control");
    await expect(page.getByRole("button", { name: "重置缩放" })).toHaveText("110%");
    await expect.poll(async () => {
      const after = await root.boundingBox();
      return Math.abs(after!.x + after!.width / 2 - x) + Math.abs(after!.y + after!.height / 2 - y);
    }).toBeLessThan(2);
  });

});
