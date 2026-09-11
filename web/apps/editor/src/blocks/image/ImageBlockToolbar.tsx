import { useCallback, useRef, useState } from "react";

import { Icon, Toolbar, ToolbarButton, ToolbarGroup, ToolbarSeparator } from "@open-office/ui";
import { defaultImageTransform, type ImageBlock, type ImageTransform, type RichText } from "@open-office/schema/artifact";

import type { BlockSessionApi } from "../../hooks/useBlockSession.js";
import { useManagedOverlay } from "../../interaction/OverlayCoordinator.js";

type ImagePanel = "crop" | "flip" | "caption" | "alt" | null;
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
  cropDraft,
  onCropDraftChange,
  onCropEditingChange,
  onCropCommit,
}: {
  blockId: string;
  image: ImageBlock;
  assetUrl: string;
  session: BlockSessionApi;
  cropDraft: ImageTransform | null;
  onCropDraftChange: (next: ImageTransform | null) => void;
  onCropEditingChange: (editing: boolean) => void;
  onCropCommit: (transform: ImageTransform) => void;
}) {
  const rootRef = useRef<HTMLDivElement>(null);
  const [panel, setPanel] = useState<ImagePanel>(null);
  const [busy, setBusy] = useState(false);
  const [transformDraft, setTransformDraft] = useState<ImageTransform | null>(null);
  const [captionDraft, setCaptionDraft] = useState(image.caption);
  const [altDraft, setAltDraft] = useState(image.alt);

  const transform = panel === "crop" ? cropDraft ?? image.transform : transformDraft ?? image.transform;
  const commitTransform = () => {
    const draft = panel === "crop" ? cropDraft : transformDraft;
    if (!draft) return;
    setTransformDraft(null);
    onCropDraftChange(null);
    if (panel === "crop") onCropCommit(draft);
    else session.setImageConfig(blockId, { transform: draft });
  };
  const commitCaption = () => {
    const next = captionDraft.trim();
    if (next !== image.caption) session.setImageConfig(blockId, { caption: next });
  };
  const commitAlt = () => {
    const next = altDraft.trim();
    if (next !== image.alt) session.setImageConfig(blockId, { alt: next });
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
      if (next === "alt") commitAlt();
      setPanel(null);
      if (next === "crop") onCropEditingChange(false);
      return;
    }
    if (panel === "crop") {
      commitTransform();
      onCropEditingChange(false);
    }
    if (panel === "caption") commitCaption();
    if (panel === "alt") commitAlt();
    if (next === "crop") {
      onCropDraftChange(image.transform);
      onCropEditingChange(true);
    }
    if (next === "flip") setTransformDraft(image.transform);
    if (next === "caption") setCaptionDraft(image.caption);
    if (next === "alt") setAltDraft(image.alt);
    setPanel(next);
  };
  const dismissPanel = useCallback(() => {
    // An outside click/Escape cancels an unfinished object edit. Committed
    // toolbar actions still produce their semantic image command immediately.
    if (panel === "crop") {
      onCropDraftChange(null);
      onCropEditingChange(false);
    }
    if (panel === "caption") setCaptionDraft(image.caption);
    if (panel === "alt") setAltDraft(image.alt);
    setTransformDraft(null);
    setPanel(null);
  }, [image.alt, image.caption, onCropDraftChange, onCropEditingChange, panel]);
  useManagedOverlay({
    id: `image-tool-panel:${blockId}`,
    kind: "dialog",
    priority: 70,
    rootRef,
    enabled: panel !== null,
    onDismiss: dismissPanel,
  });
  return (
    <div ref={rootRef}>
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
        <ToolbarButton active={panel === "alt"} aria-label="图片替代文本" title="图片替代文本" onClick={() => openPanel("alt")}>
          <Icon name="text" />
        </ToolbarButton>
      </ToolbarGroup>
      <ToolbarSeparator />
      <ToolbarGroup aria-label="图片危险操作">
        <ToolbarButton tone="danger" aria-label="删除图片" title="删除图片" onClick={() => session.deleteBlock(blockId)}>
          <Icon name="delete" />
        </ToolbarButton>
      </ToolbarGroup>

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
      {panel === "alt" && (
        <div className="block-image__tool-panel block-image__caption-panel" role="dialog" aria-label="图片替代文本">
          <label>
            <span>替代文本</span>
            <input
              autoFocus
              value={altDraft}
              maxLength={2048}
              placeholder="描述图片内容；装饰性图片可留空"
              onChange={(event) => setAltDraft(event.currentTarget.value)}
              onBlur={commitAlt}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  commitAlt();
                  setPanel(null);
                }
                if (event.key === "Escape") {
                  setAltDraft(image.alt);
                  setPanel(null);
                }
              }}
            />
          </label>
        </div>
      )}
    </Toolbar>
    </div>
  );
}
