import { cx } from "../shared/cx.js";
import type { ControlStyleProps } from "./types.js";

export { cx };

export function controlClass(base: string, { size = "md", status = "default", className }: ControlStyleProps): string {
  return cx(base, `oo-control--${size}`, status !== "default" && `oo-control--${status}`, className);
}
