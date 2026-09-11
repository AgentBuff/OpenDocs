import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { PresentationCommandBar, resolvePresentationCommandBarAvailability } from "./PresentationCommandBar.js";

const handlers = {
  onUndo: () => undefined,
  onRedo: () => undefined,
  onCreateSlide: () => undefined,
  onDuplicateSlide: () => undefined,
  onInsertText: () => undefined,
  onInsertShape: () => undefined,
  onInsertConnector: () => undefined,
  onInsertChart: () => undefined,
  onInsertImage: () => undefined,
  onPlay: () => undefined,
  onPresenter: () => undefined,
  onOpenDeckInspector: () => undefined,
  onMoveSlideBackward: () => undefined,
  onMoveSlideForward: () => undefined,
  onDeleteSlide: () => undefined,
  canMoveSlideBackward: false,
  canMoveSlideForward: true,
};

describe("PresentationCommandBar", () => {
  it("renders only the product commands backed by advertised capabilities", () => {
    const html = renderToStaticMarkup(
      <PresentationCommandBar
        hasSlide
        saving={false}
        canUndo
        canRedo={false}
        availableCapabilities={new Set([
          "presentation.history",
          "presentation.createSlide",
          "presentation.duplicateSlide",
          "presentation.insertNode",
          "presentation.registerAsset",
          "presentation.setPageSpec",
          "presentation.moveSlide",
          "presentation.deleteSlide",
        ])}
        exportHref="/api/artifacts/deck/export/pptx"
        {...handlers}
      />,
    );

    expect(html).toContain('aria-label="撤销"');
    expect(html).toContain('aria-label="插入"');
    expect(html).toContain("新建幻灯片");
    expect(html).toContain('aria-label="复制当前幻灯片"');
    expect(html).toContain("设计");
    expect(html).toContain('aria-label="删除当前幻灯片"');
    expect(html).toContain('aria-label="下载 PPTX"');
    expect(html).toContain('href="/api/artifacts/deck/export/pptx"');
    expect(html).toContain('download=""');
    expect(html).not.toContain("对齐");
    expect(html).not.toContain("分布");
    expect(html).not.toContain("锁定");
  });

  it("does not render history controls when the server has not declared history", () => {
    const html = renderToStaticMarkup(
      <PresentationCommandBar
        hasSlide={false}
        saving={false}
        canUndo={false}
        canRedo={false}
        availableCapabilities={new Set(["presentation.createSlide"])}
        {...handlers}
      />,
    );

    expect(html).not.toContain('aria-label="撤销"');
    expect(html).toContain("创建首张幻灯片");
    expect(html).not.toContain('aria-label="插入"');
    expect(html).not.toContain("播放");
  });

  it("keeps insert and image actions behind their independent server capabilities", () => {
    expect(resolvePresentationCommandBarAvailability(true, new Set([
      "presentation.insertNode",
      "presentation.setChartSpec",
    ]))).toMatchObject({
      insert: true,
      insertImage: false,
      insertChart: true,
      play: true,
    });

    expect(resolvePresentationCommandBarAvailability(true, new Set([
      "presentation.insertNode",
      "presentation.registerAsset",
    ]))).toMatchObject({
      insert: true,
      insertImage: true,
    });

    expect(resolvePresentationCommandBarAvailability(false, new Set([
      "presentation.insertNode",
      "presentation.registerAsset",
      "presentation.moveSlide",
      "presentation.deleteSlide",
      "presentation.duplicateSlide",
    ]))).toMatchObject({
      insert: false,
      insertImage: false,
      moveSlide: false,
      deleteSlide: false,
      duplicateSlide: false,
      play: false,
    });
  });

  it("exposes named controls to assistive technology and avoids unavailable groups", () => {
    const html = renderToStaticMarkup(
      <PresentationCommandBar
        hasSlide
        saving={false}
        canUndo={false}
        canRedo={false}
        availableCapabilities={new Set([
          "presentation.createSlide",
          "presentation.setTheme",
        ])}
        {...handlers}
      />,
    );

    expect(html).toContain('aria-label="新建幻灯片"');
    expect(html).toContain('aria-label="设计：页面比例与主题"');
    expect(html).toContain('aria-label="播放演示"');
    expect(html).not.toContain('aria-label="插入"');
    expect(html).not.toContain('aria-label="上移幻灯片"');
  });

  it("keeps the insert catalogue behind an accessible, capability-safe menu trigger", () => {
    const html = renderToStaticMarkup(
      <PresentationCommandBar
        hasSlide
        saving={false}
        canUndo={false}
        canRedo={false}
        availableCapabilities={new Set([
          "presentation.insertNode",
          "presentation.setChartSpec",
          "presentation.registerAsset",
        ])}
        {...handlers}
      />,
    );

    expect(html).toContain('aria-label="插入"');
    expect(html).toContain('aria-haspopup="menu"');
    expect(html).toContain("presentation-command-bar__insert-label");
  });

  it("renders slide order and destructive actions as separately announced controls", () => {
    const html = renderToStaticMarkup(
      <PresentationCommandBar
        hasSlide
        saving={false}
        canUndo={false}
        canRedo={false}
        availableCapabilities={new Set([
          "presentation.createSlide",
          "presentation.moveSlide",
          "presentation.deleteSlide",
        ])}
        {...handlers}
      />,
    );

    expect(html).toContain('aria-label="幻灯片操作"');
    expect(html).toContain('aria-label="上移幻灯片"');
    expect(html).toContain('aria-label="下移幻灯片"');
    expect(html).toContain('aria-label="删除当前幻灯片"');
    expect(html).toContain("presentation-command-bar__group-separator");
  });
});
