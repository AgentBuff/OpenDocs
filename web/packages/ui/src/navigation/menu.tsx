import { forwardRef, type ButtonHTMLAttributes, type HTMLAttributes, type KeyboardEvent as ReactKeyboardEvent, type ReactNode } from "react";
import { cx } from "../shared/cx.js";

export const MenuPanel = forwardRef<HTMLDivElement, HTMLAttributes<HTMLDivElement> & { className?: string }>(function MenuPanel(
  { className, role = "menu", onKeyDown, ...props },
  ref,
) {
  const handleKeyDown = (event: ReactKeyboardEvent<HTMLDivElement>) => {
    onKeyDown?.(event);
    if (event.defaultPrevented || !["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
    const items = Array.from(event.currentTarget.querySelectorAll<HTMLButtonElement>("[role='menuitem']:not(:disabled)"));
    if (items.length === 0) return;
    const activeIndex = items.indexOf(document.activeElement as HTMLButtonElement);
    const nextIndex = event.key === "Home"
      ? 0
      : event.key === "End"
        ? items.length - 1
        : (activeIndex + (event.key === "ArrowDown" ? 1 : -1) + items.length) % items.length;
    event.preventDefault();
    items[nextIndex]?.focus();
  };
  return <div ref={ref} className={cx("oo-menu-panel", className)} role={role} onKeyDown={handleKeyDown} {...props} />;
});

export function MenuSectionTitle({ className, children, ...props }: HTMLAttributes<HTMLDivElement> & { className?: string }) {
  return <div className={cx("oo-menu-section-title", className)} {...props}>{children}</div>;
}

export function MenuSeparator({ className, ...props }: HTMLAttributes<HTMLDivElement> & { className?: string }) {
  return <div role="separator" className={cx("oo-menu-separator", className)} {...props} />;
}

export interface MenuItemProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  icon?: ReactNode;
  trailing?: ReactNode;
  danger?: boolean;
  selected?: boolean;
}

export const MenuItem = forwardRef<HTMLButtonElement, MenuItemProps>(function MenuItem(
  { className, icon, trailing, danger = false, selected = false, type = "button", children, ...props },
  ref,
) {
  return (
    <button ref={ref} type={type} role="menuitem" aria-selected={selected || undefined} className={cx("oo-menu-item", danger && "oo-menu-item--danger", className)} {...props}>
      {icon && <span className="oo-menu-item__icon" aria-hidden="true">{icon}</span>}
      <span className="oo-menu-item__label">{children}</span>
      {trailing && <span className="oo-menu-item__trailing" aria-hidden="true">{trailing}</span>}
    </button>
  );
});
