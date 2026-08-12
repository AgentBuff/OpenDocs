import { useCallback, useEffect, useLayoutEffect, useState, type CSSProperties, type MutableRefObject } from "react";
import type { OverlayPlacement, PositionOptions, PositionResult } from "./types.js";

const useIsomorphicLayoutEffect = typeof window === "undefined" ? useEffect : useLayoutEffect;

/** Position a portal using viewport coordinates, with flipping and clamping. */
export function useFloatingPosition(anchorRef: MutableRefObject<HTMLElement | null>, popupRef: MutableRefObject<HTMLElement | null>, options: Partial<PositionOptions> = {}): PositionResult {
  const { enabled = true, placement = "bottom-start", offset = 8, boundaryDistance = 8, strategy = "fixed", matchWidth = false, boundary = null } = options;
  const [style, setStyle] = useState<CSSProperties>({ position: strategy, visibility: "hidden" });
  const [resolvedPlacement, setResolvedPlacement] = useState<OverlayPlacement>(placement);

  const update = useCallback(() => {
    const anchor = anchorRef.current;
    const popup = popupRef.current;
    if (!anchor || !popup || typeof window === "undefined") return;
    const anchorRect = anchor.getBoundingClientRect();
    const popupRect = popup.getBoundingClientRect();
    const boundaryRect = boundary?.getBoundingClientRect() ?? { left: 0, top: 0, right: window.innerWidth, bottom: window.innerHeight, width: window.innerWidth, height: window.innerHeight };
    const [side, align = "center"] = placement.split("-") as ["top" | "right" | "bottom" | "left", "start" | "end" | "center" | undefined];
    let x = anchorRect.left;
    let y = anchorRect.bottom + offset;
    if (side === "top") y = anchorRect.top - popupRect.height - offset;
    if (side === "left") { x = anchorRect.left - popupRect.width - offset; y = anchorRect.top; }
    if (side === "right") { x = anchorRect.right + offset; y = anchorRect.top; }
    if (side === "bottom") y = anchorRect.bottom + offset;
    if (side === "top" || side === "bottom") {
      if (align === "center") x = anchorRect.left + (anchorRect.width - popupRect.width) / 2;
      if (align === "end") x = anchorRect.right - popupRect.width;
    } else {
      if (align === "center") y = anchorRect.top + (anchorRect.height - popupRect.height) / 2;
      if (align === "end") y = anchorRect.bottom - popupRect.height;
    }
    const fits = {
      top: anchorRect.top - popupRect.height - offset >= boundaryRect.top + boundaryDistance,
      bottom: anchorRect.bottom + popupRect.height + offset <= boundaryRect.bottom - boundaryDistance,
      left: anchorRect.left - popupRect.width - offset >= boundaryRect.left + boundaryDistance,
      right: anchorRect.right + popupRect.width + offset <= boundaryRect.right - boundaryDistance,
    };
    let finalSide = side;
    if (side === "bottom" && !fits.bottom && fits.top) finalSide = "top";
    if (side === "top" && !fits.top && fits.bottom) finalSide = "bottom";
    if (side === "left" && !fits.left && fits.right) finalSide = "right";
    if (side === "right" && !fits.right && fits.left) finalSide = "left";
    if (finalSide !== side) {
      if (finalSide === "top") y = anchorRect.top - popupRect.height - offset;
      if (finalSide === "bottom") y = anchorRect.bottom + offset;
      if (finalSide === "left") x = anchorRect.left - popupRect.width - offset;
      if (finalSide === "right") x = anchorRect.right + offset;
    }
    const finalPlacement = (align === "center" ? finalSide : `${finalSide}-${align}`) as OverlayPlacement;
    const minX = boundaryRect.left + boundaryDistance;
    const maxX = boundaryRect.right - popupRect.width - boundaryDistance;
    const minY = boundaryRect.top + boundaryDistance;
    const maxY = boundaryRect.bottom - popupRect.height - boundaryDistance;
    x = Math.min(Math.max(x, minX), Math.max(minX, maxX));
    y = Math.min(Math.max(y, minY), Math.max(minY, maxY));
    const scrollX = strategy === "absolute" ? window.scrollX : 0;
    const scrollY = strategy === "absolute" ? window.scrollY : 0;
    setResolvedPlacement(finalPlacement);
    setStyle({ position: strategy, left: Math.round(x + scrollX), top: Math.round(y + scrollY), visibility: "visible", ...(matchWidth ? { minWidth: Math.round(anchorRect.width) } : {}) });
  }, [anchorRef, boundary, boundaryDistance, matchWidth, offset, placement, popupRef, strategy]);

  useIsomorphicLayoutEffect(() => {
    if (!enabled) {
      setResolvedPlacement(placement);
      setStyle({ position: strategy, visibility: "hidden" });
      return undefined;
    }
    update();
    if (typeof window === "undefined") return undefined;
    const onChange = () => update();
    window.addEventListener("resize", onChange);
    window.addEventListener("scroll", onChange, true);
    const observer = typeof ResizeObserver !== "undefined" ? new ResizeObserver(onChange) : null;
    if (anchorRef.current) observer?.observe(anchorRef.current);
    if (popupRef.current) observer?.observe(popupRef.current);
    return () => {
      window.removeEventListener("resize", onChange);
      window.removeEventListener("scroll", onChange, true);
      observer?.disconnect();
    };
  }, [anchorRef, enabled, placement, popupRef, strategy, update]);

  return { style, placement: resolvedPlacement, update };
}
