import { forwardRef, type ButtonHTMLAttributes } from "react";
import { cx } from "../shared/cx.js";

export type ButtonVariant = "primary" | "secondary" | "ghost" | "danger";
export type ButtonSize = "sm" | "md" | "lg";
export type ClassNameProps = { className?: string };

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement>, ClassNameProps {
  variant?: ButtonVariant;
  size?: ButtonSize;
}

export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { className, variant = "secondary", size = "md", type = "button", ...props },
  ref,
) {
  return <button ref={ref} type={type} className={cx("oo-button", `oo-button--${variant}`, `oo-button--${size}`, className)} {...props} />;
});

export const IconButton = forwardRef<HTMLButtonElement, ButtonProps>(function IconButton(
  { className, size = "md", ...props },
  ref,
) {
  return <Button ref={ref} size={size} className={cx("oo-icon-button", className)} {...props} />;
});
