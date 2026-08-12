import {
  forwardRef,
  type ButtonHTMLAttributes,
  type HTMLAttributes,
  type KeyboardEvent as ReactKeyboardEvent,
  type MouseEvent as ReactMouseEvent,
  type ReactNode,
  useId,
  useRef,
  useState,
  useEffect,
} from "react";
import { cx } from "../shared/cx.js";
import { Popover } from "../overlay/components.js";
import { Icon } from "../icons/index.js";

export const ToolbarMenuButton = forwardRef<HTMLButtonElement, ButtonHTMLAttributes<HTMLButtonElement>>(function ToolbarMenuButton({ className, type = "button", ...props }, ref) {
  return <button ref={ref} type={type} className={cx("oo-toolbar-menu-button", className)} {...props} />;
});

export interface ToolbarButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  /** Whether the action is currently selected/toggled. */
  active?: boolean;
  /** Visual intent for destructive or accent actions. */
  tone?: "default" | "accent" | "danger";
}

/**
 * Toolbar action primitive. Product toolbars should use this instead of
 * styling a raw button so state, focus and theme behavior stay consistent.
 */
export const ToolbarButton = forwardRef<HTMLButtonElement, ToolbarButtonProps>(function ToolbarButton(
  { active, tone = "default", className, type = "button", onMouseDown, ...props },
  ref,
) {
  return (
    <button
      {...props}
      ref={ref}
      type={type}
      className={cx("oo-toolbar__item", `oo-toolbar__item--${tone}`, active && "is-active", className)}
      aria-pressed={active === undefined ? props["aria-pressed"] : active}
      onMouseDown={(event: ReactMouseEvent<HTMLButtonElement>) => {
        onMouseDown?.(event);
        if (!event.defaultPrevented && event.button === 0) event.preventDefault();
      }}
    />
  );
});

export interface ToolbarSelectOption {
  value: string;
  label: ReactNode;
  disabled?: boolean;
}

export interface ToolbarSelectProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "value" | "defaultValue" | "onChange"> {
  value?: string;
  defaultValue?: string;
  options: readonly ToolbarSelectOption[];
  onValueChange?: (value: string) => void;
  placeholder?: ReactNode;
  compact?: boolean;
  popupClassName?: string;
}

/**
 * Arco-inspired select primitive: a real combobox trigger and a themed listbox
 * popup. Native select menus cannot share the UI theme or keyboard contract.
 */
export const ToolbarSelect = forwardRef<HTMLButtonElement, ToolbarSelectProps>(function ToolbarSelect(
  { compact = false, className, popupClassName, value, defaultValue = "", options, onValueChange, placeholder, disabled, onKeyDown, onMouseDown, ...props },
  ref,
) {
  const generatedId = useId();
  const listboxId = `oo-toolbar-select-${generatedId.replace(/:/g, "")}`;
  const optionItems = options;
  const [internalValue, setInternalValue] = useState(defaultValue);
  const [open, setOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(() => Math.max(0, optionItems.findIndex((option) => option.value === (value ?? defaultValue))));
  const optionRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const selectedValue = value ?? internalValue;
  const selectedOption = optionItems.find((option) => option.value === selectedValue);
  const displayValue = selectedOption?.label ?? placeholder ?? "";
  const enabledIndexes = optionItems.map((option, index) => option.disabled ? -1 : index).filter((index) => index >= 0);

  useEffect(() => {
    const selectedIndex = optionItems.findIndex((option) => option.value === selectedValue);
    if (selectedIndex >= 0 && !optionItems[selectedIndex]?.disabled) setActiveIndex(selectedIndex);
  }, [selectedValue]);

  const focusOption = (index: number) => {
    const nextIndex = enabledIndexes.includes(index) ? index : enabledIndexes[0];
    if (nextIndex === undefined) return;
    setActiveIndex(nextIndex);
    requestAnimationFrame(() => optionRefs.current[nextIndex]?.focus());
  };

  const selectValue = (nextValue: string, index: number) => {
    const option = optionItems[index];
    if (!option || option.disabled) return;
    if (value === undefined) setInternalValue(nextValue);
    setActiveIndex(index);
    onValueChange?.(nextValue);
    setOpen(false);
  };

  const handleTriggerKeyDown = (event: ReactKeyboardEvent<HTMLButtonElement>) => {
    onKeyDown?.(event);
    if (event.defaultPrevented || disabled) return;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      setOpen(true);
      focusOption(activeIndex);
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      setOpen((current) => !current);
    }
  };

  const handleListKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      setOpen(false);
      return;
    }
    const currentEnabledIndex = enabledIndexes.indexOf(activeIndex);
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (enabledIndexes.length === 0) return;
      const delta = event.key === "ArrowDown" ? 1 : -1;
      const next = (currentEnabledIndex + delta + enabledIndexes.length) % enabledIndexes.length;
      focusOption(enabledIndexes[next]);
    } else if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      if (activeIndex >= 0) selectValue(optionItems[activeIndex]?.value ?? "", activeIndex);
    }
  };

  return (
    <Popover
      open={open}
      onOpenChange={setOpen}
      placement="bottom-start"
      offset={4}
      role="presentation"
      popupClassName={cx("oo-toolbar-select__popover", popupClassName)}
      content={(
        <div id={listboxId} className="oo-toolbar-select__listbox" role="listbox" aria-label={props["aria-label"]} onKeyDown={handleListKeyDown}>
          {optionItems.map((option, index) => (
            <button
              key={`${option.value}-${index}`}
              ref={(element) => { optionRefs.current[index] = element; }}
              type="button"
              role="option"
              aria-selected={option.value === selectedValue}
              aria-disabled={option.disabled || undefined}
              tabIndex={index === activeIndex ? 0 : -1}
              className={cx("oo-toolbar-select__option", option.value === selectedValue && "is-selected")}
              disabled={option.disabled}
              onMouseDown={(event) => event.preventDefault()}
              onClick={() => selectValue(option.value, index)}
            >
              <span>{option.label}</span>
              {option.value === selectedValue && <span className="oo-toolbar-select__check" aria-hidden="true">✓</span>}
            </button>
          ))}
        </div>
      )}
    >
      <button
        {...props}
        ref={ref}
        type="button"
        role="combobox"
        aria-haspopup="listbox"
        aria-controls={listboxId}
        aria-label={props["aria-label"]}
        disabled={disabled}
        className={cx("oo-toolbar__select", compact && "oo-toolbar__select--compact", className)}
        onKeyDown={handleTriggerKeyDown}
        onMouseDown={(event: ReactMouseEvent<HTMLButtonElement>) => {
          onMouseDown?.(event);
          if (!event.defaultPrevented && event.button === 0) event.preventDefault();
        }}
      >
        <span className="oo-toolbar-select__value">{displayValue}</span>
        <Icon name="arrow-down" className="oo-toolbar-select__arrow" />
      </button>
    </Popover>
  );
});

export interface ToolbarFieldProps extends HTMLAttributes<HTMLSpanElement> {
  children?: ReactNode;
}

/** Groups a toolbar control with an icon/affordance without leaking layout CSS to products. */
export function ToolbarField({ className, ...props }: ToolbarFieldProps) {
  return <span className={cx("oo-toolbar__field", className)} {...props} />;
}

export function ToolbarSeparator({ className, ...props }: HTMLAttributes<HTMLSpanElement> & { className?: string }) {
  return <span className={cx("oo-toolbar__separator", className)} role="separator" aria-orientation="vertical" {...props} />;
}

export function Toolbar({ className, density, children, role = "toolbar", ...props }: HTMLAttributes<HTMLDivElement> & { className?: string; density?: "compact" | "default" | "comfortable" }) {
  return <div className={cx("oo-toolbar", density && `oo-toolbar--${density}`, className)} role={role} {...props}><div className="oo-toolbar__row">{children}</div></div>;
}

export function ToolbarGroup({ className, ...props }: HTMLAttributes<HTMLSpanElement> & { className?: string }) {
  return <span className={cx("oo-toolbar__group", className)} {...props} />;
}

/**
 * Layout primitive for a primary action paired with a separate disclosure
 * action. The arrow remains a real button (usually wrapped by Popover/Dropdown)
 * so keyboard and pointer semantics are not conflated with the primary action.
 */
export function ToolbarSplitGroup({ className, ...props }: HTMLAttributes<HTMLSpanElement> & { className?: string }) {
  return <span className={cx("oo-toolbar__split", className)} role="group" {...props} />;
}
