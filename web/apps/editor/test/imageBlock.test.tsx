import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import type { DocumentBlock } from "@open-office/schema/artifact";

import { ImageBlockView } from "../src/blocks/renderers.js";
import type { BlockSessionApi } from "../src/hooks/useBlockSession.js";

const imageBlock: DocumentBlock = {
  id: "image-1",
  kind: { type: "image" },
  presentation: {
    align: "center",
    list: null,
    indentStart: 0,
    indentEnd: 0,
    spacingBefore: 0,
    spacingAfter: 0,
    lineHeight: 1,
    namedStyle: null,
  },
  content: { text: "", runs: [] },
  children: [],
  data: {
    type: "image",
    data: {
      assetId: "asset-1",
      alt: "产品截图",
      originalAssetId: null,
      transform: { crop: { top: 0, right: 0, bottom: 0, left: 0 }, flipHorizontal: false, flipVertical: false },
      caption: "",
    },
  },
};

const imageSession = {
  assetUrl: (assetId: string) => `/api/assets/${assetId}`,
  setBlockPresentation: () => undefined,
  deleteBlock: () => undefined,
  setActiveBlock: () => undefined,
  reportError: () => undefined,
} as unknown as BlockSessionApi;

describe("ImageBlockView", () => {
  it("renders object feedback and its own toolbar only while selected", () => {
    const selected = renderToStaticMarkup(<ImageBlockView block={imageBlock} session={imageSession} selected />);
    const idle = renderToStaticMarkup(<ImageBlockView block={imageBlock} session={imageSession} selected={false} />);

    expect(selected).toContain("block-image is-selected");
    expect(selected).toContain('aria-label="图片工具栏"');
    expect(selected).toContain('aria-label="图片文件操作"');
    expect(selected).toContain('aria-label="下载图片"');
    expect(selected).toContain('aria-label="裁剪图片"');
    expect(selected).toContain('aria-label="翻转图片"');
    expect(selected).toContain('aria-label="恢复原图"');
    expect(selected).toContain('aria-label="提取图片文字"');
    expect(selected).toContain('aria-label="压缩图片"');
    expect(selected).toContain('aria-label="图片题注"');
    expect(selected).toContain('aria-label="删除图片"');
    expect(selected).toContain('src="/api/assets/asset-1"');
    expect(idle).not.toContain('aria-label="图片工具栏"');
  });
});
