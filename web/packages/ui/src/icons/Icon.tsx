import { useMemo } from "react";
import { useIconRegistry } from "./registry.js";
import type { IconName, IconProps, IconRenderer, IconRegistry } from "./types.js";

export function Icon({ name, ...props }: IconProps & { name: IconName }) {
  const renderer = useIconRegistry().resolve(name);
  return renderer ? <>{renderer(props)}</> : null;
}

export function useIconRegistryWithOverrides(overrides: Partial<Record<IconName, IconRenderer>>, version?: string): IconRegistry {
  return useMemo(() => {
    const entries = { ...overrides };
    return {
      version: version ?? "1.0.0",
      resolve: (name) => entries[name],
    };
  }, [overrides, version]);
}
