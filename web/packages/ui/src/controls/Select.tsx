import { forwardRef, type SelectHTMLAttributes } from "react";
import { controlClass } from "./shared.js";
import type { ControlStyleProps } from "./types.js";

export interface SelectProps extends Omit<SelectHTMLAttributes<HTMLSelectElement>, "size">, ControlStyleProps {}

export const Select = forwardRef<HTMLSelectElement, SelectProps>(function Select(
  { className, size = "md", status = "default", ...props },
  ref,
) {
  return <select ref={ref} className={controlClass("oo-select", { size, status, className })} aria-invalid={status === "danger" || undefined} {...props} />;
});
