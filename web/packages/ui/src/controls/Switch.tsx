import { forwardRef, useCallback, useState, type ChangeEvent, type InputHTMLAttributes } from "react";
import { cx } from "./shared.js";
import type { ControlSize } from "./types.js";

export interface SwitchProps extends Omit<InputHTMLAttributes<HTMLInputElement>, "type" | "size"> {
  checked?: boolean;
  defaultChecked?: boolean;
  onCheckedChange?: (checked: boolean) => void;
  size?: ControlSize;
}

export const Switch = forwardRef<HTMLInputElement, SwitchProps>(function Switch(
  { className, checked: checkedProp, defaultChecked = false, onChange, onCheckedChange, disabled, size = "md", ...props },
  ref,
) {
  const [uncontrolled, setUncontrolled] = useState(defaultChecked);
  const checked = checkedProp ?? uncontrolled;
  const handleChange = useCallback((event: ChangeEvent<HTMLInputElement>) => {
    if (checkedProp === undefined) setUncontrolled(event.target.checked);
    onCheckedChange?.(event.target.checked);
    onChange?.(event);
  }, [checkedProp, onChange, onCheckedChange]);
  return <label className={cx("oo-switch", `oo-control--${size}`, disabled && "is-disabled", className)}><input ref={ref} type="checkbox" role="switch" checked={checked} disabled={disabled} onChange={handleChange} {...props} /><span className="oo-switch__track" aria-hidden="true"><span className="oo-switch__thumb" /></span></label>;
});
