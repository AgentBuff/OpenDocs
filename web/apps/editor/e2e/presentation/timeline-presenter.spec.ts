import { expect, test } from "@playwright/test";

import { deleteFixture, readArtifactRevision } from "../support/fixtures.js";

test.describe("Presentation timeline and presenter mode", () => {
  let artifactId: string;

  test.beforeEach(async ({ request }) => {
    const response = await request.post("http://127.0.0.1:8788/api/artifacts", {
      data: { kind: "presentation", title: "E2E presenter" },
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    artifactId = (await response.json() as { id: string }).id;
  });

  test.afterEach(async ({ request }) => {
    await deleteFixture(request, { artifactId, revision: 1 });
  });

  test("edits notes and timeline, seeks deterministically and syncs an audience window without transactions", async ({ page, request }) => {
    const transactionRequests: string[][] = [];
    page.on("request", (entry) => {
      if (entry.method() === "POST" && entry.url().endsWith("/transactions")) {
        transactionRequests.push((entry.postDataJSON() as { commands: Array<{ typeId: string }> }).commands.map((command) => command.typeId));
      }
    });
    await page.goto(`/?doc=${artifactId}`);
    await page.getByRole("button", { name: "创建首张幻灯片" }).click();
    await page.getByRole("button", { name: "文本框", exact: true }).first().click();
    await expect(page.locator(".presentation-studio__status")).toContainText("已同步");
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: "打开幻灯片属性" }).click();

    const notes = page.getByLabel("演讲者备注");
    await notes.dispatchEvent("compositionstart");
    await notes.fill("  中文😀演讲提示  ");
    await expect(page.getByRole("button", { name: "保存备注" })).toBeDisabled();
    await notes.dispatchEvent("compositionend");
    await page.getByRole("button", { name: "保存备注" }).click();
    await page.getByRole("button", { name: "撤销", exact: true }).click();
    await expect(notes).toHaveValue("");
    await page.getByRole("button", { name: "重做", exact: true }).click();
    await expect(notes).toHaveValue("  中文😀演讲提示  ");
    await page.getByRole("button", { name: "添加动画" }).click();
    await expect(page.getByLabel("1 个动画", { exact: true })).toBeVisible();
    await page.getByRole("button", { name: "添加动画" }).click();
    await expect(page.getByLabel("2 个动画", { exact: true })).toBeVisible();
    const timelineEntries = page.locator(".presentation-timeline__entry");
    const transfer = await page.evaluateHandle(() => new DataTransfer());
    await page.getByLabel("拖动第 1 个动画").dispatchEvent("dragstart", { dataTransfer: transfer });
    await timelineEntries.last().dispatchEvent("dragover", { dataTransfer: transfer });
    await timelineEntries.last().dispatchEvent("drop", { dataTransfer: transfer });
    await page.getByLabel("拖动第 1 个动画").dispatchEvent("dragend", { dataTransfer: transfer });
    await expect.poll(() => transactionRequests.some((commands) => commands.includes("presentation.moveAnimation"))).toBe(true);
    await page.getByLabel("页面切换效果").selectOption("fade");
    await page.getByLabel("页面切换时长").fill("400");
    await page.getByRole("button", { name: "应用切换" }).click();
    await expect(page.locator(".presentation-studio__status")).toContainText("已同步");
    const revisionBeforePlayback = await readArtifactRevision(request, artifactId);
    const transactionCountBeforePlayback = transactionRequests.length;

    await page.getByRole("button", { name: "播放演示" }).click();
    const slide = page.getByRole("group", { name: "当前幻灯片，点击播放下一步" });
    const playbackText = page.locator(".presentation-playback__text");
    await expect(playbackText).toHaveCSS("visibility", "hidden");
    await page.getByRole("button", { name: "暂停" }).click();
    await page.getByLabel("当前动画进度").fill("200");
    await expect(slide).toHaveCSS("opacity", "0.55");
    await slide.click();
    await page.getByRole("button", { name: "暂停" }).click();
    await page.getByLabel("当前动画进度").fill("150");
    await expect(playbackText).toHaveCSS("visibility", "visible");
    await expect(playbackText).toHaveCSS("opacity", "0.5");
    expect(transactionRequests).toHaveLength(transactionCountBeforePlayback);
    await page.getByRole("button", { name: /退出播放/ }).click();

    await page.getByRole("tab", { name: "放映" }).click();
    const popupPromise = page.waitForEvent("popup");
    await page.getByRole("button", { name: "演讲者视图" }).click();
    const audience = await popupPromise;
    await expect(page.getByRole("complementary", { name: "演讲者控制台" })).toContainText("  中文😀演讲提示  ");
    await expect(audience.getByRole("group", { name: "观众幻灯片" })).toBeVisible();
    await expect(page.getByText("已连接")).toBeVisible();
    await audience.reload();
    await expect(audience.getByRole("group", { name: "观众幻灯片" })).toBeVisible();
    await page.getByRole("button", { name: "下一页" }).click();
    await expect(audience.locator(".presentation-playback__text")).toHaveCSS("visibility", "visible");
    expect(transactionRequests).toHaveLength(transactionCountBeforePlayback);
    expect(await readArtifactRevision(request, artifactId)).toBe(revisionBeforePlayback);
    await page.getByRole("button", { name: /退出播放/ }).click();
    await expect.poll(() => audience.isClosed()).toBe(true);
    await page.reload();
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: "打开幻灯片属性" }).click();
    await expect(page.getByLabel("演讲者备注")).toHaveValue("  中文😀演讲提示  ");
    await expect(page.getByLabel("2 个动画", { exact: true })).toBeVisible();
  });

  test("falls back to single-window playback when the presenter popup is blocked", async ({ page, request }) => {
    await page.goto(`/?doc=${artifactId}`);
    await page.getByRole("button", { name: "创建首张幻灯片" }).click();
    await expect(page.locator(".presentation-studio__status")).toContainText("已同步");
    const revision = await readArtifactRevision(request, artifactId);
    await page.evaluate(() => Object.defineProperty(window, "open", { configurable: true, value: () => null }));
    await page.getByRole("tab", { name: "放映" }).click();
    await page.getByRole("button", { name: "演讲者视图" }).click();
    await expect(page.getByRole("alert")).toContainText("浏览器阻止了观众窗口");
    await expect(page.getByRole("group", { name: "当前幻灯片，点击播放下一步" })).toBeVisible();
    expect(await readArtifactRevision(request, artifactId)).toBe(revision);
  });

  test("renders transitions and entrance animations immediately for reduced motion", async ({ page }) => {
    await page.emulateMedia({ reducedMotion: "reduce" });
    await page.goto(`/?doc=${artifactId}`);
    await page.getByRole("button", { name: "创建首张幻灯片" }).click();
    await page.getByRole("button", { name: "文本框", exact: true }).first().click();
    await page.keyboard.press("Escape");
    await page.getByRole("button", { name: "打开幻灯片属性" }).click();
    await page.getByRole("button", { name: "添加动画" }).click();
    await page.getByLabel("页面切换效果").selectOption("wipe");
    await page.getByRole("button", { name: "应用切换" }).click();
    await expect(page.locator(".presentation-studio__status")).toContainText("已同步");
    await page.getByRole("button", { name: "播放演示" }).click();
    const slide = page.getByRole("group", { name: "当前幻灯片，点击播放下一步" });
    await expect(slide).toHaveCSS("clip-path", "none");
    await slide.click();
    await expect(page.locator(".presentation-playback__text")).toHaveCSS("opacity", "1");
  });
});
