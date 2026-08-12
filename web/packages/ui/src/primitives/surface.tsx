import { type HTMLAttributes } from "react";
import { cx } from "../shared/cx.js";

export function Surface({ className, ...props }: HTMLAttributes<HTMLDivElement> & { className?: string }) {
  return <div className={cx("oo-surface", className)} {...props} />;
}
