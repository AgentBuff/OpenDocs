import { useRef, type PointerEvent, type RefObject } from "react";

export interface ImagePlacement {
  offsetX: number;
  offsetY: number;
}

const MOVE_THRESHOLD = 3;

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(Math.max(value, minimum), maximum);
}

/**
 * Keeps image movement as an object-local interaction. The document still
 * owns the persisted placement; pointer movement only renders a transient
 * preview and produces one semantic placement patch on pointer-up.
 */
export function useImageMove({
  frameRef,
  placement,
  onPreview,
  onCommit,
  onCancel,
}: {
  frameRef: RefObject<HTMLElement>;
  placement: ImagePlacement;
  onPreview: (placement: ImagePlacement | null) => void;
  onCommit: (placement: ImagePlacement) => void;
  onCancel: () => void;
}) {
  const dragRef = useRef<{
    startX: number;
    startY: number;
    placement: ImagePlacement;
    minOffsetX: number;
    maxOffsetX: number;
    minOffsetY: number;
    moved: boolean;
    preview: ImagePlacement;
  } | null>(null);

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || event.target instanceof Element && event.target.closest(".block-image__resize-handle")) return;
    const frame = frameRef.current;
    if (!frame) return;
    event.preventDefault();
    const rect = frame.getBoundingClientRect();
    const containingRow = frame.closest(".block-row__body")?.getBoundingClientRect();
    const page = frame.closest(".block-editor__page")?.getBoundingClientRect();
    dragRef.current = {
      startX: event.clientX,
      startY: event.clientY,
      placement,
      minOffsetX: containingRow ? placement.offsetX + containingRow.left - rect.left : -8192,
      maxOffsetX: containingRow ? placement.offsetX + containingRow.right - rect.right : 8192,
      minOffsetY: page ? placement.offsetY + page.top - rect.top : -8192,
      moved: false,
      preview: placement,
    };
    event.currentTarget.setPointerCapture(event.pointerId);
  };

  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const drag = dragRef.current;
    if (!drag) return;
    const deltaX = event.clientX - drag.startX;
    const deltaY = event.clientY - drag.startY;
    if (!drag.moved && Math.hypot(deltaX, deltaY) < MOVE_THRESHOLD) return;
    drag.moved = true;
    drag.preview = {
      offsetX: Math.round(clamp(drag.placement.offsetX + deltaX, drag.minOffsetX, drag.maxOffsetX)),
      offsetY: Math.round(Math.max(drag.minOffsetY, drag.placement.offsetY + deltaY)),
    };
    onPreview(drag.preview);
  };

  const finish = (event: PointerEvent<HTMLDivElement>, commit: boolean) => {
    const drag = dragRef.current;
    if (!drag) return;
    dragRef.current = null;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
    if (commit && drag.moved) onCommit(drag.preview);
    else onCancel();
  };

  return {
    onPointerDown,
    onPointerMove,
    onPointerUp: (event: PointerEvent<HTMLDivElement>) => finish(event, true),
    onPointerCancel: (event: PointerEvent<HTMLDivElement>) => finish(event, false),
  };
}
