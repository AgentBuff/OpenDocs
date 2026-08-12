export function Divider({ orientation = "horizontal", className }: { orientation?: "horizontal" | "vertical"; className?: string }) {
  return <div className={["oo-divider", `oo-divider--${orientation}`, className].filter(Boolean).join(" ")} role="separator" aria-orientation={orientation} />;
}
