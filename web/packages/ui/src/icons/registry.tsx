import { createContext, useContext, type ReactNode } from "react";
import { builtinIcons } from "./builtin.js";
import type { IconName, IconRegistry, IconRenderer } from "./types.js";

export function createIconRegistry(overrides: Partial<Record<IconName, IconRenderer>> = {}, version = "1.0.0"): IconRegistry {
  const entries = { ...builtinIcons, ...overrides };
  return { version, resolve: (name) => entries[name] };
}

const defaultRegistry = createIconRegistry();
const IconRegistryContext = createContext<IconRegistry>(defaultRegistry);

export function IconProvider({ registry = defaultRegistry, children }: { registry?: IconRegistry; children: ReactNode }) {
  return <IconRegistryContext.Provider value={registry}>{children}</IconRegistryContext.Provider>;
}

export function useIconRegistry(): IconRegistry {
  return useContext(IconRegistryContext);
}
