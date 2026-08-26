import { useRef, type PointerEvent } from "react";

import type { ImageTransform } from "@open-office/schema/artifact";

type Crop = ImageTransform["crop"];
type CropHandle = "nw" | "n" | "ne" | "e" | "se" | "s" | "sw" | "w" | "move";

const MIN_VISIBLE_PORTION = 0.08;

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(Math.max(value, minimum), maximum);
}

function updateCrop(start: Crop, handle: CropHandle, deltaX: number, deltaY: number): Crop {
  if (handle === "move") {
    const shiftX = clamp(deltaX, -start.left, start.right);
    const shiftY = clamp(deltaY, -start.top, start.bottom);
    return {
      top: start.top + shiftY,
      right: start.right - shiftX,
      bottom: start.bottom - shiftY,
      left: start.left + shiftX,
    };
  }

  const next = { ...start };
  if (handle.includes("n")) next.top = clamp(start.top + deltaY, 0, 1 - start.bottom - MIN_VISIBLE_PORTION);
  if (handle.includes("s")) next.bottom = clamp(start.bottom - deltaY, 0, 1 - start.top - MIN_VISIBLE_PORTION);
  if (handle.includes("w")) next.left = clamp(start.left + deltaX, 0, 1 - start.right - MIN_VISIBLE_PORTION);
  if (handle.includes("e")) next.right = clamp(start.right - deltaX, 0, 1 - start.left - MIN_VISIBLE_PORTION);
  return next;
}

/**
 * Direct-manipulation crop surface. The image stays renderer-owned; this
 * component emits only normalized crop fractions for the ImageBlock command.
 */
export function ImageCropOverlay({ crop, onChange }: { crop: Crop; onChange: (next: Crop) => void }) {
  const dragRef = useRef<{
    handle: CropHandle;
    startX: number;
    startY: number;
    crop: Crop;
    width: number;
    height: number;
  } | null>(null);

  const begin = (handle: CropHandle, event: PointerEvent<HTMLElement>) => {
    const frame = event.currentTarget.closest(".block-image__frame");
    if (!frame) return;
    event.preventDefault();
    event.stopPropagation();
    const rect = frame.getBoundingClientRect();
    dragRef.current = {
      handle,
      startX: event.clientX,
      startY: event.clientY,
      crop,
      width: Math.max(1, rect.width),
      height: Math.max(1, rect.height),
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const move = (event: PointerEvent<HTMLElement>) => {
    const drag = dragRef.current;
    if (!drag) return;
    event.preventDefault();
    onChange(updateCrop(
      drag.crop,
      drag.handle,
      (event.clientX - drag.startX) / drag.width,
      (event.clientY - drag.startY) / drag.height,
    ));
  };

  const end = (event: PointerEvent<HTMLElement>) => {
    dragRef.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  };

  const selectionStyle = {
    left: `${crop.left * 100}%`,
    top: `${crop.top * 100}%`,
    width: `${(1 - crop.left - crop.right) * 100}%`,
    height: `${(1 - crop.top - crop.bottom) * 100}%`,
  };

  return (
    <div className="block-image__crop-overlay" aria-label="图片裁剪区域">
      <div
        className="block-image__crop-selection"
        style={selectionStyle}
        onPointerDown={(event) => begin("move", event)}
        onPointerMove={move}
        onPointerUp={end}
        onPointerCancel={end}
        role="presentation"
      >
        <div className="block-image__crop-grid" aria-hidden="true" />
        {(["nw", "n", "ne", "e", "se", "s", "sw", "w"] as const).map((handle) => (
          <button
            key={handle}
            type="button"
            className={`block-image__crop-handle block-image__crop-handle--${handle}`}
            aria-label={`调整裁剪区域 ${handle}`}
            onPointerDown={(event) => begin(handle, event)}
            onPointerMove={move}
            onPointerUp={end}
            onPointerCancel={end}
          />
        ))}
      </div>
    </div>
  );
}
