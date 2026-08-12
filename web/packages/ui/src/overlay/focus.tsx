import { useEffect, useRef, type KeyboardEvent as ReactKeyboardEvent } from "react";
import type { FocusScopeProps } from "./types.js";

const focusableSelector = [
  "a[href]", "button:not([disabled])", "input:not([disabled])", "select:not([disabled])",
  "textarea:not([disabled])", "[tabindex]:not([tabindex='-1'])", "[contenteditable='true']",
].join(",");

export function FocusScope({ children, trapped = false, loop = true, returnFocus = true, returnFocusRef, className }: FocusScopeProps) {
  const rootRef = useRef<HTMLDivElement>(null);
  const restoreRef = useRef<HTMLElement | null>(null);
  useEffect(() => {
    if (returnFocus && typeof document !== "undefined") restoreRef.current = document.activeElement as HTMLElement;
    return () => {
      const target = returnFocusRef?.current ?? restoreRef.current;
      if (returnFocus && target?.isConnected) target.focus({ preventScroll: true });
    };
  }, [returnFocus, returnFocusRef]);

  const onKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key !== "Tab" || !trapped) return;
    const nodes = Array.from(rootRef.current?.querySelectorAll<HTMLElement>(focusableSelector) ?? []).filter((node) => {
      if (!node.isConnected || node.getClientRects().length === 0) return false;
      const style = window.getComputedStyle(node);
      return style.visibility !== "hidden" && style.display !== "none";
    });
    if (nodes.length === 0) return;
    const first = nodes[0];
    const last = nodes[nodes.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      if (loop) { event.preventDefault(); last.focus(); }
    } else if (!event.shiftKey && document.activeElement === last) {
      if (loop) { event.preventDefault(); first.focus(); }
    }
  };
  return <div ref={rootRef} className={["oo-focus-scope", className].filter(Boolean).join(" ")} onKeyDown={onKeyDown}>{children}</div>;
}
