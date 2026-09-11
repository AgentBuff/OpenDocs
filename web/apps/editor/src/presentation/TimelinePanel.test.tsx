import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";

import { appendTimelineOrderKey, TimelinePanel } from "./TimelinePanel.js";

const slide = {
  slideId: "slide-1",
  orderKey: "a",
  name: "第一页",
  layoutId: null,
  background: { type: "none" },
  nodes: [{ id: "node-1", name: "标题", kind: { type: "text" } }],
  timeline: { entries: [] },
} as never;

describe("presentation timeline panel", () => {
  it("offers a capability-gated animation creator for projected nodes", () => {
    const html = renderToStaticMarkup(<TimelinePanel
      slide={slide}
      selectedNodeId="node-1"
      disabled={false}
      availableCapabilities={new Set(["presentation.upsertAnimation"])}
      onTransitionChange={() => undefined}
      onAnimationUpsert={() => undefined}
      onAnimationDelete={() => undefined}
      onAnimationMove={() => undefined}
    />);
    expect(html).toContain('aria-label="添加对象动画"');
    expect(html).toContain('aria-label="动画对象"');
    expect(html).toContain("标题");
  });

  it("creates a unique key that sorts after the current last entry", () => {
    const entries = [
      { id: "b", orderKey: "z" },
      { id: "a", orderKey: "a" },
    ] as never;
    const next = appendTimelineOrderKey(entries);
    expect(next.localeCompare("z")).toBeGreaterThan(0);
    expect(appendTimelineOrderKey([])).not.toBe("");
  });
});
