import type { ControlSize } from "../controls/types.js";

export function Spinner({ size = "md", label = "加载中", className }: { size?: ControlSize; label?: string; className?: string }) {
  return <span className={["oo-spinner", `oo-control--${size}`, className].filter(Boolean).join(" ")} role="status" aria-label={label} />;
}
