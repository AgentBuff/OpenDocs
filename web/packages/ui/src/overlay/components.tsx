import {
  cloneElement,
  useEffect,
  useId,
  useMemo,
  useRef,
  type FocusEvent,
  type HTMLAttributes,
  type MutableRefObject,
  type ReactElement,
  type ReactNode,
  type Ref,
} from "react";
import { Portal } from "./registry.js";
import { composeRefs } from "./refs.js";
import { useControllableState } from "./state.js";
import { useDismissableLayer } from "./dismiss.js";
import { useFloatingPosition } from "./position.js";
import type { OverlayPlacement, OverlayStrategy, OverlayTrigger } from "./types.js";

function mergeHandler<E extends { defaultPrevented: boolean }>(first: ((event: E) => void) | undefined, second: (event: E) => void) {
  return (event: E) => {
    first?.(event);
    if (!event.defaultPrevented) second(event);
  };
}

function useFocusReturn(open: boolean, restoreRef: MutableRefObject<HTMLElement | null>, returnFocus: boolean) {
  const previousOpen = useRef(open);
  useEffect(() => {
    if (open && !previousOpen.current && typeof document !== "undefined") restoreRef.current = document.activeElement as HTMLElement;
    if (!open && previousOpen.current && returnFocus && restoreRef.current?.isConnected) restoreRef.current.focus({ preventScroll: true });
    previousOpen.current = open;
  }, [open, restoreRef, returnFocus]);
}

export interface PopoverProps {
  children: ReactElement;
  content: ReactNode;
  open?: boolean;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  trigger?: OverlayTrigger | OverlayTrigger[];
  placement?: OverlayPlacement;
  offset?: number;
  boundaryDistance?: number;
  strategy?: OverlayStrategy;
  matchWidth?: boolean;
  returnFocus?: boolean;
  getPopupContainer?: () => Element | null;
  role?: HTMLAttributes<HTMLDivElement>["role"];
  className?: string;
  popupClassName?: string;
  childrenClassName?: string;
  openDelay?: number;
  closeDelay?: number;
}

export function Popover({ children, content, open: openProp, defaultOpen = false, onOpenChange, trigger = "click", placement = "bottom-start", offset = 8, boundaryDistance = 8, strategy = "fixed", matchWidth = false, returnFocus = true, getPopupContainer, role = "dialog", className, popupClassName, childrenClassName, openDelay = 0, closeDelay = 100 }: PopoverProps) {
  const [open, setOpen] = useControllableState({ value: openProp, defaultValue: defaultOpen, onChange: onOpenChange });
  const triggerRef = useRef<HTMLElement | null>(null);
  const popupRef = useRef<HTMLDivElement>(null);
  const restoreRef = useRef<HTMLElement | null>(null);
  const popupId = useId();
  const triggerList = Array.isArray(trigger) ? trigger : [trigger];
  const hoverCloseTimer = useRef<number | undefined>(undefined);
  const hoverOpenTimer = useRef<number | undefined>(undefined);
  const excludedRefs = useMemo(() => [triggerRef], []);
  const { style, placement: resolvedPlacement } = useFloatingPosition(triggerRef, popupRef, { enabled: open, placement, offset, boundaryDistance, strategy, matchWidth });
  useFocusReturn(open, restoreRef, returnFocus);
  useDismissableLayer({ enabled: open, rootRef: popupRef, excludedRefs, onDismiss: () => setOpen(false) });

  useEffect(() => () => {
    window.clearTimeout(hoverCloseTimer.current);
    window.clearTimeout(hoverOpenTimer.current);
  }, []);
  const toggle = () => setOpen((current) => !current);
  const openNow = () => {
    window.clearTimeout(hoverCloseTimer.current);
    window.clearTimeout(hoverOpenTimer.current);
    if (openDelay > 0) hoverOpenTimer.current = window.setTimeout(() => setOpen(true), openDelay);
    else setOpen(true);
  };
  const closeLater = () => {
    window.clearTimeout(hoverOpenTimer.current);
    hoverCloseTimer.current = window.setTimeout(() => setOpen(false), closeDelay);
  };
  const childClassName = (children.props as { className?: string }).className;
  const childProps: Record<string, unknown> = { className: [childClassName, childrenClassName].filter(Boolean).join(" ") || undefined };
  if (triggerList.includes("click")) childProps.onClick = mergeHandler((children.props as { onClick?: (event: React.MouseEvent) => void }).onClick, () => toggle());
  if (triggerList.includes("contextMenu")) childProps.onContextMenu = mergeHandler((children.props as { onContextMenu?: (event: React.MouseEvent) => void }).onContextMenu, (event) => { event.preventDefault(); openNow(); });
  if (triggerList.includes("hover")) {
    childProps.onPointerEnter = mergeHandler((children.props as { onPointerEnter?: (event: React.PointerEvent) => void }).onPointerEnter, () => openNow());
    childProps.onPointerLeave = mergeHandler((children.props as { onPointerLeave?: (event: React.PointerEvent) => void }).onPointerLeave, () => closeLater());
  }
  if (triggerList.includes("focus")) {
    childProps.onFocus = mergeHandler((children.props as { onFocus?: (event: FocusEvent) => void }).onFocus, () => openNow());
    childProps.onBlur = mergeHandler((children.props as { onBlur?: (event: FocusEvent) => void }).onBlur, (event) => { if (!popupRef.current?.contains(event.relatedTarget as Node | null)) setOpen(false); });
  }
  childProps.ref = composeRefs(triggerRef, (children as ReactElement & { ref?: Ref<HTMLElement> }).ref);
  childProps["aria-expanded"] = open;
  childProps["aria-controls"] = open ? popupId : undefined;
  childProps["data-oo-overlay-trigger"] = true;

  const triggerElement = cloneElement(children, childProps);
  const popup = open ? (
    <Portal container={getPopupContainer} anchorRef={triggerRef}>
      <div id={popupId} ref={popupRef} className={["oo-overlay", popupClassName].filter(Boolean).join(" ")} data-placement={resolvedPlacement} role={role} style={style} onPointerEnter={() => window.clearTimeout(hoverCloseTimer.current)} onPointerLeave={triggerList.includes("hover") ? closeLater : undefined}>
        {content}
      </div>
    </Portal>
  ) : null;
  return <span className={["oo-overlay-anchor", className].filter(Boolean).join(" ")}>{triggerElement}{popup}</span>;
}

export interface DropdownProps extends Omit<PopoverProps, "trigger" | "role"> {
  trigger?: OverlayTrigger | OverlayTrigger[];
}

export function Dropdown({ trigger = "click", ...props }: DropdownProps) {
  return <Popover {...props} trigger={trigger} role="menu" />;
}

export interface TooltipProps extends Omit<PopoverProps, "trigger" | "role" | "defaultOpen"> {
  delay?: number;
}

export function Tooltip({ delay = 350, ...props }: TooltipProps) {
  return <Popover {...props} trigger={["hover", "focus"]} role="tooltip" openDelay={delay} closeDelay={0} />;
}
