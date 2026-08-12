import type { ReactNode } from "react";

export function Badge({ children, tone = "neutral", className }: { children: ReactNode; tone?: "neutral" | "accent" | "success" | "warning" | "danger"; className?: string }) {
  return <span className={["oo-badge", `oo-badge--${tone}`, className].filter(Boolean).join(" ")}>{children}</span>;
}
