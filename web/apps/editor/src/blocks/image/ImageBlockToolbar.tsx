import { useState } from "react";

import { Icon, Toolbar, ToolbarButton, ToolbarGroup, ToolbarSeparator } from "@open-office/ui";
import { defaultImageTransform, type ImageBlock, type ImageTransform, type RichText } from "@open-office/schema/artifact";

import type { BlockSessionApi } from "../../hooks/useBlockSession.js";

type ImagePanel = "crop" | "flip" | "caption" | null;
type TextDetectorResult = { rawValue: string };
type TextDetectorLike = { detect(source: ImageBitmapSource): Promise<TextDetectorResult[]> };
type TextDetectorConstructor = new () => TextDetectorLike;

function cropIsDefault(transform: ImageTransform): boolean {
  const { crop } = transform;
  return crop.top === 0 && crop.right === 0 && crop.bottom === 0 && crop.left === 0;
}

function transformIsDefault(transform: ImageTransform): boolean {
  return cropIsDefault(transform) && !transform.flipHorizontal && !transform.flipVertical;
}

function clampCrop(next: ImageTransform, edge: keyof ImageTransform["crop"], value: number): ImageTransform {
  const crop = { ...next.crop, [edge]: Math.max(0, Math.min(0.9, value)) };
  const horizontal = crop.left + crop.right;
  const vertical = crop.top + crop.bottom;
  if (horizontal >= 0.95) crop[edge] = Math.max(0, crop[edge] - (horizontal - 0.94));
  if (vertical >= 0.95) crop[edge] = Math.max(0, crop[edge] - (vertical - 0.94));
  return { ...next, crop };
}

async function compressAsset(assetUrl: string): Promise<File> {
  const response = await fetch(assetUrl);
  if (!response.ok) throw new Error("无法读取图片资源");
  const source = await response.blob();
  const bitmap = await createImageBitmap(source);
  const scale = Math.min(1, 2560 / Math.max(bitmap.width, bitmap.height));
  const width = Math.max(1, Math.round(bitmap.width * scale));
  const height = Math.max(1, Math.round(bitmap.height * scale));
  const canvas = document.createElement("canvas");
  canvas.width = width;
  canvas.height = height;
  const context = canvas.getContext("2d");
  if (!context) throw new Error("浏览器不支持图片压缩");
  context.drawImage(bitmap, 0, 0, width, height);
  bitmap.close();
  const compressed = await new Promise<Blob | null>((resolve) => canvas.toBlob(resolve, "image/webp", 0.78));
  if (!compressed) throw new Error("图片压缩失败");
  if (compressed.size >= source.size) throw new Error("压缩后文件未变小，已保留原图");
  return new File([compressed], "compressed-image.webp", { type: "image/webp" });
}

/**
 * Object-local image commands. The toolbar deliberately maps one-to-one to
 * persisted ImageBlock operations; text alignment and generic link copying
 * stay outside this object surface.
 */
export function ImageBlockToolbar({
  blockId,
  image,
  assetUrl,
  session,
}: {
  blockId: string;
  image: ImageBlock;
  assetUrl: string;
  session: BlockSessionApi;
}) {
  const [panel, setPanel] = useState<ImagePanel>(null);
  const [busy, setBusy] = useState(false);
  const [transformDraft, setTransformDraft] = useState<ImageTransform | null>(null);
  const [captionDraft, setCaptionDraft] = useState(image.caption);

  const transform = transformDraft ?? image.transform;
  const updateTransform = (next: ImageTransform) => setTransformDraft(next);
  const commitTransform = () => {
    if (!transformDraft) return;
    setTransformDraft(null);
    session.setImageConfig(blockId, { transform: transformDraft });
  };
  const commitCaption = () => {
    const next = captionDraft.trim();
    if (next !== image.caption) session.setImageConfig(blockId, { caption: next });
  };
  const reset = () => {
    session.setImageConfig(blockId, {
      assetId: image.originalAssetId ?? image.assetId,
      originalAssetId: null,
      transform: defaultImageTransform(),
    });
    setPanel(null);
  };

  const downloadAsset = () => {
    const anchor = document.createElement("a");
    anchor.href = assetUrl;
    anchor.download = "image";
    anchor.rel = "noopener";
    anchor.click();
    anchor.remove();
  };

  const extractText = async () => {
    const TextDetector = (window as typeof window & { TextDetector?: TextDetectorConstructor }).TextDetector;
    if (!TextDetector || typeof createImageBitmap !== "function") {
      session.reportError("当前浏览器不支持本地图片文字识别");
      return;
    }
    setBusy(true);
    try {
      const response = await fetch(assetUrl);
      if (!response.ok) throw new Error("无法读取图片资源");
      const bitmap = await createImageBitmap(await response.blob());
      const results = await new TextDetector().detect(bitmap);
      bitmap.close();
      const text = results.map((result) => result.rawValue.trim()).filter(Boolean).join("\n");
      if (!text) throw new Error("图片中未识别到文字");
      const next = session.insertAfter(blockId);
      if (!next) throw new Error("无法插入识别结果");
      const content: RichText = { text, runs: [] };
      session.updateContent(next, content);
      session.setActiveBlock(next);
    } catch (error) {
      session.reportError(error);
    } finally {
      setBusy(false);
    }
  };

  const compress = async () => {
    setBusy(true);
    try {
      const compressed = await compressAsset(assetUrl);
      await session.replaceImageAsset(blockId, compressed);
    } catch (error) {
      session.reportError(error);
    } finally {
      setBusy(false);
    }
  };

  const openPanel = (next: Exclude<ImagePanel, null>) => {
    if (panel === next) {
      if (next === "crop") commitTransform();
      if (next === "caption") commitCaption();
      setPanel(null);
      return;
    }
    if (panel === "crop") commitTransform();
    if (panel === "caption") commitCaption();
    if (next === "crop" || next === "flip") setTransformDraft(image.transform);
    if (next === "caption") setCaptionDraft(image.caption);
    setPanel(next);
  };
  const crop = transform.crop;

  return (
    <Toolbar className="block-image__toolbar" density="compact" aria-label="图片工具栏">
      <ToolbarGroup aria-label="图片文件操作">
        <ToolbarButton aria-label="下载图片" title="下载图片" onClick={downloadAsset}>
          <Icon name="download" />
        </ToolbarButton>
      </ToolbarGroup>
      <ToolbarSeparator />
      <ToolbarGroup aria-label="图片变换">
        <ToolbarButton active={panel === "crop"} aria-label="裁剪图片" title="裁剪图片" onClick={() => openPanel("crop")}>
          <Icon name="crop" />
        </ToolbarButton>
        <ToolbarButton active={panel === "flip"} aria-label="翻转图片" title="翻转图片" onClick={() => openPanel("flip")}>
          <Icon name="flip" />
        </ToolbarButton>
        <ToolbarButton disabled={transformIsDefault(transform) && !image.originalAssetId} aria-label="恢复原图" title="恢复原图" onClick={reset}>
          <Icon name="restore" />
        </ToolbarButton>
      </ToolbarGroup>
      <ToolbarSeparator />
      <ToolbarGroup aria-label="图片内容操作">
        <ToolbarButton disabled={busy} aria-label="提取图片文字" title="提取图片文字" onClick={() => void extractText()}>
          <Icon name="text-extract" />
        </ToolbarButton>
        <ToolbarButton disabled={busy} aria-label="压缩图片" title="压缩图片" onClick={() => void compress()}>
          <Icon name="compress" />
        </ToolbarButton>
        <ToolbarButton active={panel === "caption"} aria-label="图片题注" title="图片题注" onClick={() => openPanel("caption")}>
          <Icon name="caption" />
        </ToolbarButton>
      </ToolbarGroup>
      <ToolbarSeparator />
      <ToolbarGroup aria-label="图片危险操作">
        <ToolbarButton tone="danger" aria-label="删除图片" title="删除图片" onClick={() => session.deleteBlock(blockId)}>
          <Icon name="delete" />
        </ToolbarButton>
      </ToolbarGroup>

      {panel === "crop" && (
        <div className="block-image__tool-panel block-image__crop-panel" role="dialog" aria-label="裁剪图片">
          <strong>裁剪</strong>
          {(["top", "right", "bottom", "left"] as const).map((edge) => (
            <label key={edge}>
              <span>{{ top: "上", right: "右", bottom: "下", left: "左" }[edge]}</span>
              <input
                type="range"
                min="0"
                max="0.8"
                step="0.01"
                value={crop[edge]}
                onChange={(event) => updateTransform(clampCrop(transform, edge, Number(event.currentTarget.value)))}
                onPointerUp={commitTransform}
                onBlur={commitTransform}
                onKeyUp={(event) => {
                  if (event.key === "Enter") commitTransform();
                }}
              />
              <output>{Math.round(crop[edge] * 100)}%</output>
            </label>
          ))}
          <button
            type="button"
            onClick={() => {
              setTransformDraft(null);
              session.setImageConfig(blockId, { transform: defaultImageTransform() });
            }}
          >
            重置裁剪
          </button>
        </div>
      )}
      {panel === "flip" && (
        <div className="block-image__tool-panel block-image__flip-panel" role="dialog" aria-label="翻转图片">
          <button
            type="button"
            onClick={() => {
              const next = { ...transform, flipHorizontal: !transform.flipHorizontal };
              setTransformDraft(null);
              session.setImageConfig(blockId, { transform: next });
            }}
          >
            水平翻转
          </button>
          <button
            type="button"
            onClick={() => {
              const next = { ...transform, flipVertical: !transform.flipVertical };
              setTransformDraft(null);
              session.setImageConfig(blockId, { transform: next });
            }}
          >
            垂直翻转
          </button>
        </div>
      )}
      {panel === "caption" && (
        <div className="block-image__tool-panel block-image__caption-panel" role="dialog" aria-label="图片题注">
          <label>
            <span>题注</span>
            <input
              autoFocus
              value={captionDraft}
              maxLength={512}
              placeholder="输入图片题注"
              onChange={(event) => setCaptionDraft(event.currentTarget.value)}
              onBlur={commitCaption}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  commitCaption();
                  setPanel(null);
                }
                if (event.key === "Escape") {
                  setCaptionDraft(image.caption);
                  setPanel(null);
                }
              }}
            />
          </label>
        </div>
      )}
    </Toolbar>
  );
}
