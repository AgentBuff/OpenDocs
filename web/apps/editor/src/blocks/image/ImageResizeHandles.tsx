import { useRef, type PointerEvent, type RefObject } from "react";

type Handle = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w";

export interface ImageResizeGeometry {
  width: number;
  height: number;
  offsetX: number;
  offsetY: number;
}

const MINIMUM_IMAGE_SIZE = 48;
const MAXIMUM_IMAGE_SIZE = 8192;

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(Math.max(value, minimum), maximum);
}

/**
 * DOM-first equivalent of a scene-graph transformer. Pointer movement only
 * produces transient geometry; its single completed geometry is persisted by
 * the image block after pointer-up. West/north anchors include a flow-relative
 * offset, which keeps the opposite visual edge fixed instead of merely
 * resizing from the top-left origin.
 */
export function ImageResizeHandles({
  frameRef,
  placement,
  lockAspectRatio,
  onPreview,
  onCommit,
  onCancel,
}: {
  frameRef: RefObject<HTMLElement>;
  placement: { offsetX: number; offsetY: number };
  lockAspectRatio: boolean;
  onPreview: (geometry: ImageResizeGeometry) => void;
  onCommit: (geometry: ImageResizeGeometry) => void;
  onCancel: () => void;
}) {
  const dragRef = useRef<{
    handle: Handle;
    startX: number;
    startY: number;
    width: number;
    height: number;
    placement: { offsetX: number; offsetY: number };
    maxWidth: number;
    maxHeight: number;
    preview: ImageResizeGeometry;
  } | null>(null);

  const resize = (event: PointerEvent<HTMLButtonElement>) => {
    const drag = dragRef.current;
    if (!drag) return;
    const deltaX = event.clientX - drag.startX;
    const deltaY = event.clientY - drag.startY;
    const hasWest = drag.handle.includes("w");
    const hasEast = drag.handle.includes("e");
    const hasNorth = drag.handle.includes("n");
    const hasSouth = drag.handle.includes("s");
    const changesWidth = hasWest || hasEast;
    const changesHeight = hasNorth || hasSouth;
    const widthDelta = hasWest ? -deltaX : deltaX;
    const heightDelta = hasNorth ? -deltaY : deltaY;
    const cornerResize = changesWidth && changesHeight;

    let width = drag.width + (changesWidth ? widthDelta : 0);
    let height = drag.height + (changesHeight ? heightDelta : 0);
    if (lockAspectRatio && cornerResize) {
      // Univer keeps image corners proportional by default. Choose the axis
      // that moved farther in normalized space, then derive the other axis.
      const widthScale = width / drag.width;
      const heightScale = height / drag.height;
      const scale = Math.abs(widthScale - 1) >= Math.abs(heightScale - 1) ? widthScale : heightScale;
      const boundedScale = clamp(
        scale,
        Math.max(MINIMUM_IMAGE_SIZE / drag.width, MINIMUM_IMAGE_SIZE / drag.height),
        Math.min(drag.maxWidth / drag.width, drag.maxHeight / drag.height),
      );
      width = drag.width * boundedScale;
      height = drag.height * boundedScale;
    } else {
      width = clamp(width, MINIMUM_IMAGE_SIZE, drag.maxWidth);
      height = clamp(height, MINIMUM_IMAGE_SIZE, drag.maxHeight);
    }

    const preview = {
      width: Math.round(width),
      height: Math.round(height),
      offsetX: Math.round(drag.placement.offsetX + (hasWest ? drag.width - width : 0)),
      offsetY: Math.round(drag.placement.offsetY + (hasNorth ? drag.height - height : 0)),
    };
    drag.preview = preview;
    onPreview(preview);
  };

  const start = (handle: Handle, event: PointerEvent<HTMLButtonElement>) => {
    const frame = frameRef.current;
    if (!frame) return;
    event.preventDefault();
    event.stopPropagation();
    const rect = frame.getBoundingClientRect();
    const containingRow = frame.closest(".block-row__body")?.getBoundingClientRect();
    const page = frame.closest(".block-editor__page")?.getBoundingClientRect();
    const hasWest = handle.includes("w");
    const hasNorth = handle.includes("n");
    const maxWidth = containingRow
      ? Math.max(MINIMUM_IMAGE_SIZE, hasWest ? rect.right - containingRow.left : containingRow.right - rect.left)
      : MAXIMUM_IMAGE_SIZE;
    // A block row grows with the image, so it is not a vertical constraint.
    // Only a northward resize has a stable page boundary; southward growth
    // expands normal document flow.
    const maxHeight = hasNorth && page
      ? Math.max(MINIMUM_IMAGE_SIZE, rect.bottom - page.top)
      : MAXIMUM_IMAGE_SIZE;
    const preview = {
      width: Math.round(rect.width),
      height: Math.round(rect.height),
      offsetX: placement.offsetX,
      offsetY: placement.offsetY,
    };
    dragRef.current = {
      handle,
      startX: event.clientX,
      startY: event.clientY,
      width: preview.width,
      height: preview.height,
      placement,
      maxWidth: Math.min(MAXIMUM_IMAGE_SIZE, maxWidth),
      maxHeight: Math.min(MAXIMUM_IMAGE_SIZE, maxHeight),
      preview,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const finish = (event: PointerEvent<HTMLButtonElement>, commit: boolean) => {
    const drag = dragRef.current;
    if (!drag) return;
    dragRef.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    if (commit) onCommit(drag.preview);
    else onCancel();
  };

  return (
    <div className="block-image__resize-handles" aria-label="调整图片尺寸">
      {(["nw", "n", "ne", "e", "se", "s", "sw", "w"] as const).map((handle) => (
        <button
          key={handle}
          className={`block-image__resize-handle block-image__resize-handle--${handle}`}
          type="button"
          aria-label={`拖动调整图片${handle}`}
          onPointerDown={(event) => start(handle, event)}
          onPointerMove={resize}
          onPointerUp={(event) => finish(event, true)}
          onPointerCancel={(event) => finish(event, false)}
        />
      ))}
    </div>
  );
}
