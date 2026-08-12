import { createContext, useContext, useMemo, type MutableRefObject } from "react";
import { createPortal } from "react-dom";
import type { OverlayProviderProps, PortalContainer, PortalProps } from "./types.js";

export interface OverlayLayer {
  id: string;
  rootRef: MutableRefObject<HTMLElement | null>;
  excludedRefs: Array<MutableRefObject<HTMLElement | null>>;
}

export interface OverlayRegistry {
  register(layer: OverlayLayer): () => void;
  isTop(id: string): boolean;
}

function createOverlayRegistry(): OverlayRegistry {
  const layers: OverlayLayer[] = [];
  return {
    register(layer) {
      layers.push(layer);
      return () => {
        const index = layers.indexOf(layer);
        if (index >= 0) layers.splice(index, 1);
      };
    },
    isTop(id) {
      return layers.length > 0 && layers[layers.length - 1].id === id;
    },
  };
}

const fallbackRegistry = createOverlayRegistry();
export const OverlayRegistryContext = createContext<OverlayRegistry | null>(null);
export const OverlayContainerContext = createContext<PortalContainer | undefined>(undefined);

export function getOverlayRegistry(): OverlayRegistry {
  return fallbackRegistry;
}

export function OverlayProvider({ children, container }: OverlayProviderProps) {
  const registry = useMemo(createOverlayRegistry, []);
  const inheritedContainer = useContext(OverlayContainerContext);
  const portalContainer = container ?? inheritedContainer;
  return <OverlayContainerContext.Provider value={portalContainer}><OverlayRegistryContext.Provider value={registry}>{children}</OverlayRegistryContext.Provider></OverlayContainerContext.Provider>;
}

export function Portal({ children, container, anchorRef }: PortalProps) {
  const scopedContainer = useContext(OverlayContainerContext);
  if (typeof document === "undefined") return null;
  const nearestThemeRoot = anchorRef?.current?.closest(".oo-theme-root");
  const defaultContainer = () => document.querySelector(".oo-theme-root") ?? document.body;
  const targetSpec = container ?? scopedContainer ?? nearestThemeRoot ?? defaultContainer;
  const target = typeof targetSpec === "function" ? targetSpec() : targetSpec;
  return target ? createPortal(children, target) : null;
}
