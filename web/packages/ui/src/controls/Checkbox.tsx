import { forwardRef, type InputHTMLAttributes, type ReactNode } from "react";
import { cx } from "./shared.js";

export interface CheckboxProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "type"> {
  children?: ReactNode;
}

export const Checkbox = forwardRef<HTMLInputElement, CheckboxProps>(function Checkbox({ className, children, ...props }, ref) {
  return <label className={cx("oo-checkbox", className)}><input ref={ref} type="checkbox" {...props} />{children && <span className="oo-checkbox__label">{children}</span>}</label>;
});
