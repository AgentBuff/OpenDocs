import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { NodeToolbar } from "./PresentationToolbars.js";

const ui = {
  inspector: { id: "presentation.group", title: "组合", fields: [] },
  toolbar: [{ id: "lock", group: "node", kind: "button", action: "node.lock", capability: "presentation.setNodeLocked", label: "锁定对象" }],
} as never;

describe("presentation node toolbar", () => {
  it("treats an omitted optional enabled predicate as enabled", () => {
    const html = renderToStaticMarkup(<NodeToolbar ui={ui} locked={false} disabled={false} downloadUrl={null} onAction={() => undefined} onOpenInspector={() => undefined} />);
    expect(html).toContain('aria-label="锁定对象"');
    expect(html).not.toContain("disabled");
  });

  it("still disables every action while a transaction is saving", () => {
    const html = renderToStaticMarkup(<NodeToolbar ui={ui} locked={false} disabled downloadUrl={null} onAction={() => undefined} onOpenInspector={() => undefined} />);
    expect(html).toContain("disabled");
  });
});
