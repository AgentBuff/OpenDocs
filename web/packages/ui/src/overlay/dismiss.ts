import { useContext, useEffect, useId, useRef } from "react";
import { OverlayRegistryContext, getOverlayRegistry, type OverlayLayer } from "./registry.js";
import type { DismissableLayerOptions } from "./types.js";

export function useDismissableLayer({
  enabled = true,
  rootRef,
  excludedRefs = [],
  onDismiss,
  onPointerDownOutside,
  onEscapeKeyDown,
}: DismissableLayerOptions): void {
  const contextRegistry = useContext(OverlayRegistryContext);
  const registry = contextRegistry ?? getOverlayRegistry();
  const id = useId();
  const callbackRefs = useRef({ onDismiss, onPointerDownOutside, onEscapeKeyDown, excludedRefs });
  callbackRefs.current = { onDismiss, onPointerDownOutside, onEscapeKeyDown, excludedRefs };

  useEffect(() => {
    if (!enabled || typeof document === "undefined") return undefined;
    const layer: OverlayLayer = { id, rootRef, excludedRefs };
    return registry.register(layer);
  }, [enabled, excludedRefs, id, registry, rootRef]);

  useEffect(() => {
    if (!enabled || typeof document === "undefined") return undefined;
    const isInside = (event: Event) => {
      const target = event.target as Node | null;
      if (!target) return false;
      if (rootRef.current?.contains(target)) return true;
      return callbackRefs.current.excludedRefs.some((ref) => ref.current?.contains(target));
    };
    const onPointerDown = (event: PointerEvent) => {
      if (!registry.isTop(id) || isInside(event)) return;
      callbackRefs.current.onPointerDownOutside?.(event);
      if (!event.defaultPrevented) callbackRefs.current.onDismiss();
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || !registry.isTop(id)) return;
      callbackRefs.current.onEscapeKeyDown?.(event);
      if (!event.defaultPrevented) {
        event.preventDefault();
        callbackRefs.current.onDismiss();
      }
    };
    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("keydown", onKeyDown, true);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("keydown", onKeyDown, true);
    };
  }, [enabled, id, registry, rootRef]);
}
