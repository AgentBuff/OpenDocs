import { forwardRef, type TextareaHTMLAttributes } from "react";
import { controlClass } from "./shared.js";
import type { ControlStyleProps } from "./types.js";

export interface TextareaProps extends TextareaHTMLAttributes<HTMLTextAreaElement>, ControlStyleProps {}

export const Textarea = forwardRef<HTMLTextAreaElement, TextareaProps>(function Textarea(
  { className, size = "md", status = "default", ...props },
  ref,
) {
  return <textarea ref={ref} className={controlClass("oo-textarea", { size, status, className })} aria-invalid={status === "danger" || undefined} {...props} />;
});
