import { expect, test, type APIRequestContext, type Page } from "@playwright/test";
import { readFile } from "node:fs/promises";

import { deleteFixture } from "../support/fixtures.js";

type Deck = {
  pageSpec: unknown;
  slides: Array<{
    orderKey: string;
    name: string;
    notes?: string | null;
    transition?: unknown;
    nodes: Array<Record<string, unknown> & { id: string; orderKey: string }>;
    timeline: { entries: Array<Record<string, unknown> & { targetNodeId: string; orderKey: string }> };
  }>;
};

async function readDeck(request: APIRequestContext, artifactId: string): Promise<Deck> {
  const response = await request.get(`http://127.0.0.1:8788/api/artifacts/${artifactId}/snapshot`);
  expect(response.ok(), await response.text()).toBeTruthy();
  const snapshot = await response.json() as { artifact: { payload: { kind: string; data: Deck } } };
  expect(snapshot.artifact.payload.kind).toBe("presentation");
  return snapshot.artifact.payload.data;
}

async function openSlideInspector(page: Page) {
  await page.locator(".presentation-studio__stage").click({ position: { x: 10, y: 10 } });
  await expect(page.getByLabel("演讲者备注")).toBeVisible();
}

function strictPptxSemantics(deck: Deck) {
  return {
    pageSpec: deck.pageSpec,
    slides: [...deck.slides].sort((a, b) => a.orderKey.localeCompare(b.orderKey)).map((slide) => {
      const nodes = [...slide.nodes].sort((a, b) => a.orderKey.localeCompare(b.orderKey));
      const nodeIndex = new Map(nodes.map((node, index) => [node.id, index]));
      return {
        name: slide.name,
        notes: slide.notes ?? null,
        transition: slide.transition ?? null,
        nodes: nodes.map(({ id: _id, orderKey: _orderKey, parentId: _parentId, ...node }) => node),
        timeline: [...slide.timeline.entries]
          .sort((a, b) => a.orderKey.localeCompare(b.orderKey))
          .map(({ id: _id, orderKey: _orderKey, targetNodeId, ...entry }) => ({
            ...entry,
            targetNodeIndex: nodeIndex.get(targetNodeId),
          })),
      };
    }),
  };
}

test.describe("Presentation PPTX fidelity", () => {
  let artifactId: string;
  let importedArtifactId: string | undefined;

  test.beforeEach(async ({ request }) => {
    const response = await request.post("http://127.0.0.1:8788/api/artifacts", {
      data: { kind: "presentation", title: "PPTX fidelity E2E" },
    });
    expect(response.ok(), await response.text()).toBeTruthy();
    artifactId = (await response.json() as { id: string }).id;
    importedArtifactId = undefined;
  });

  test.afterEach(async ({ request }) => {
    if (importedArtifactId) {
      await deleteFixture(request, { artifactId: importedArtifactId, revision: 1 });
    }
    await deleteFixture(request, { artifactId, revision: 1 });
  });

  test("creates, edits, undoes, reloads, plays, exports and imports a strict semantic roundtrip", async ({ page, request }) => {
    test.setTimeout(90_000);
    await page.goto(`/?doc=${artifactId}`);
    await page.getByRole("button", { name: "创建首张幻灯片" }).click();
    await page.getByRole("button", { name: "文本框", exact: true }).first().click();
    const textNode = page.locator(".presentation-studio__node").filter({ hasText: "双击输入文本" });
    await textNode.dblclick();
    await page.getByLabel("编辑文本对象").fill("PPTX 严格往返 😀");
    await page.getByLabel("编辑文本对象").press("ControlOrMeta+Enter");
    await openSlideInspector(page);
    await page.getByLabel("演讲者备注").fill("  导出演讲备注\n第二行  ");
    await page.getByRole("button", { name: "保存备注" }).click();
    await page.getByLabel("新动画效果").selectOption("wipe");
    await page.getByRole("button", { name: "添加动画" }).click();
    await page.getByLabel("页面切换效果").selectOption("push");
    await page.getByLabel("页面切换时长").fill("725");
    await page.getByRole("button", { name: "应用切换" }).click();
    await expect(page.locator(".presentation-studio__status")).toContainText("已同步");

    await page.getByRole("button", { name: "撤销", exact: true }).click();
    await expect(page.getByLabel("页面切换效果")).toHaveValue("none");
    await page.getByRole("button", { name: "重做", exact: true }).click();
    await expect(page.getByLabel("页面切换效果")).toHaveValue("push");
    await page.reload();
    await openSlideInspector(page);
    await expect(page.getByLabel("演讲者备注")).toHaveValue("  导出演讲备注\n第二行  ");
    await expect(page.getByLabel("1 个动画", { exact: true })).toBeVisible();
    await expect(page.getByLabel("页面切换效果")).toHaveValue("push");

    await page.getByRole("button", { name: "播放演示" }).click();
    await page.getByRole("group", { name: "当前幻灯片，点击播放下一步" }).click();
    await expect(page.locator(".presentation-playback__text")).toContainText("PPTX 严格往返 😀");
    await page.getByRole("button", { name: /退出播放/ }).click();

    const exportResponse = await request.get(`http://127.0.0.1:8788/api/artifacts/${artifactId}/export/pptx`);
    expect(exportResponse.ok(), await exportResponse.text()).toBeTruthy();
    expect(exportResponse.headers()["content-type"]).toBe("application/vnd.openxmlformats-officedocument.presentationml.presentation");
    expect(exportResponse.headers()["content-disposition"]).toMatch(/\.pptx(?:"|$)/i);
    const downloadPromise = page.waitForEvent("download");
    await page.getByRole("link", { name: "下载 PPTX" }).click();
    const download = await downloadPromise;
    expect(download.suggestedFilename()).toMatch(/\.pptx$/i);
    const downloadPath = await download.path();
    expect(downloadPath).not.toBeNull();
    const downloadedPptx = await readFile(downloadPath!);

    const expected = strictPptxSemantics(await readDeck(request, artifactId));
    await page.goto("/");
    await page.locator('input[type="file"][accept*=".pptx"]').last().setInputFiles({
      name: download.suggestedFilename(),
      mimeType: "application/vnd.openxmlformats-officedocument.presentationml.presentation",
      buffer: downloadedPptx,
    });
    await expect(page.locator(".presentation-studio__status")).toContainText("已同步");
    importedArtifactId = new URL(page.url()).searchParams.get("doc") ?? undefined;
    expect(importedArtifactId).toBeTruthy();

    await expect(page.locator(".presentation-studio__paragraph").first()).toContainText("PPTX 严格往返 😀");
    await openSlideInspector(page);
    await expect(page.getByLabel("演讲者备注")).toHaveValue("  导出演讲备注\n第二行  ");
    await expect(page.getByLabel("页面切换效果")).toHaveValue("push");
    await expect(page.getByLabel("页面切换时长")).toHaveValue("725");
    await expect(page.getByLabel("1 个动画", { exact: true })).toBeVisible();
    expect(strictPptxSemantics(await readDeck(request, importedArtifactId!))).toEqual(expected);

    await page.getByRole("button", { name: "播放演示" }).click();
    await page.getByRole("group", { name: "当前幻灯片，点击播放下一步" }).click();
    await expect(page.locator(".presentation-playback__text")).toContainText("PPTX 严格往返 😀");
  });
});
