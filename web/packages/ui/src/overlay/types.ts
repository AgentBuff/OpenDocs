import type { CSSProperties, MutableRefObject, ReactNode } from "react";

export type OverlayPlacement =
  | "top-start" | "top" | "top-end"
  | "right-start" | "right" | "right-end"
  | "bottom-start" | "bottom" | "bottom-end"
  | "left-start" | "left" | "left-end";

export type OverlayStrategy = "fixed" | "absolute";
export type OverlayTrigger = "click" | "hover" | "focus" | "contextMenu";
export type PortalContainer = Element | null | (() => Element | null);

export interface OverlayProviderProps {
  children: ReactNode;
  container?: PortalContainer;
}

export interface PortalProps {
  children: ReactNode;
  container?: PortalContainer;
  anchorRef?: MutableRefObject<HTMLElement | null>;
}

export interface DismissableLayerOptions {
  enabled?: boolean;
  rootRef: MutableRefObject<HTMLElement | null>;
  excludedRefs?: Array<MutableRefObject<HTMLElement | null>>;
  onDismiss: () => void;
  onPointerDownOutside?: (event: PointerEvent) => void;
  onEscapeKeyDown?: (event: KeyboardEvent) => void;
}

export interface FocusScopeProps {
  children: ReactNode;
  trapped?: boolean;
  loop?: boolean;
  returnFocus?: boolean;
  returnFocusRef?: MutableRefObject<HTMLElement | null>;
  className?: string;
}

export interface PositionOptions {
  enabled: boolean;
  placement: OverlayPlacement;
  offset: number;
  boundaryDistance: number;
  strategy: OverlayStrategy;
  matchWidth: boolean;
  boundary?: Element | null;
}

export interface PositionResult {
  style: CSSProperties;
  placement: OverlayPlacement;
  update: () => void;
}
