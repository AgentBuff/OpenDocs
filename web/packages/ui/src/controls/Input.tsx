import { forwardRef, type InputHTMLAttributes, type ReactNode } from "react";
import { controlClass, cx } from "./shared.js";
import type { ControlStyleProps } from "./types.js";

export interface InputProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "size" | "prefix">, ControlStyleProps {
  prefix?: ReactNode;
  suffix?: ReactNode;
}

export const Input = forwardRef<HTMLInputElement, InputProps>(function Input(
  { className, size = "md", status = "default", prefix, suffix, ...props },
  ref,
) {
  const input = <input ref={ref} className={controlClass("oo-input", { size, status, className })} aria-invalid={status === "danger" || undefined} {...props} />;
  if (!prefix && !suffix) return input;
  return (
    <span className={cx("oo-input-group", `oo-control--${size}`, status !== "default" && `oo-control--${status}`, className)}>
      {prefix && <span className="oo-input__affix" aria-hidden="true">{prefix}</span>}
      {input}
      {suffix && <span className="oo-input__affix" aria-hidden="true">{suffix}</span>}
    </span>
  );
});
