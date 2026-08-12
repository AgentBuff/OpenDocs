import type { ReactNode } from "react";

export function Empty({ title = "暂无内容", description, action, className }: { title?: ReactNode; description?: ReactNode; action?: ReactNode; className?: string }) {
  return <div className={["oo-empty", className].filter(Boolean).join(" ")} role="status"><div className="oo-empty__title">{title}</div>{description && <div className="oo-empty__description">{description}</div>}{action && <div className="oo-empty__action">{action}</div>}</div>;
}
