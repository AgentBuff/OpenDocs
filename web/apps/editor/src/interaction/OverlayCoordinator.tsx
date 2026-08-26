import { createContext, useContext, useEffect, useMemo, useRef, type PropsWithChildren, type RefObject } from "react";

import type { InteractionStore } from "./interactionStore.js";
import { OverlayStore, type OverlayDismissReason } from "./overlayStore.js";
import type { OverlayKind } from "./types.js";

interface OverlayContextValue {
  store: OverlayStore;
}

const OverlayCoordinatorContext = createContext<OverlayContextValue | null>(null);

export function OverlayCoordinator({ children, interaction }: PropsWithChildren<{ interaction: InteractionStore }>) {
  const store = useMemo(() => new OverlayStore(), []);
  useEffect(() => {
    const onPointerDown = (event: PointerEvent) => {
      const target = event.target;
      if (target instanceof Node) store.dismissOutsidePointer(target);
    };
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      const overlayHandled = store.dismissEscape();
      interaction.dispatch({ type: "escape", overlayHandled });
      if (overlayHandled) event.preventDefault();
    };
    document.addEventListener("pointerdown", onPointerDown, true);
    document.addEventListener("keydown", onKeyDown, true);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown, true);
      document.removeEventListener("keydown", onKeyDown, true);
    };
  }, [interaction, store]);
  return <OverlayCoordinatorContext.Provider value={{ store }}>{children}</OverlayCoordinatorContext.Provider>;
}

export function useManagedOverlay({
  id,
  kind,
  priority,
  rootRef,
  excludedRefs = [],
  closeOnEscape = true,
  closeOnOutsidePointer = true,
  enabled = true,
  onDismiss,
}: {
  id: string;
  kind: OverlayKind;
  priority: number;
  rootRef: RefObject<HTMLElement | null>;
  excludedRefs?: readonly RefObject<HTMLElement | null>[];
  closeOnEscape?: boolean;
  closeOnOutsidePointer?: boolean;
  enabled?: boolean;
  onDismiss: (reason: OverlayDismissReason) => void;
}): void {
  const context = useContext(OverlayCoordinatorContext);
  const callbackRef = useRef(onDismiss);
  callbackRef.current = onDismiss;
  useEffect(() => {
    if (!context || !enabled) return undefined;
    return context.store.register({
      id,
      kind,
      priority,
      closeOnEscape,
      closeOnOutsidePointer,
      contains(target) {
        return rootRef.current?.contains(target) === true
          || excludedRefs.some((ref) => ref.current?.contains(target) === true);
      },
      close(reason) {
        callbackRef.current(reason);
      },
    });
  }, [closeOnEscape, closeOnOutsidePointer, context, enabled, excludedRefs, id, kind, priority, rootRef]);
}
