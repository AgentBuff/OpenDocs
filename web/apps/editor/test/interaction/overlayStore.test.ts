import { describe, expect, it } from "vitest";

import { OverlayStore } from "../../src/interaction/overlayStore.js";

function overlay(id: string, priority: number, close: (reason: string) => void, options: Partial<{ escape: boolean; outside: boolean }> = {}) {
  return {
    id,
    kind: "popover" as const,
    priority,
    closeOnEscape: options.escape ?? true,
    closeOnOutsidePointer: options.outside ?? true,
    contains: () => false,
    close,
  };
}

describe("OverlayStore", () => {
  it("dismisses only the topmost priority layer on Escape", () => {
    const store = new OverlayStore();
    const closed: string[] = [];
    store.register(overlay("menu", 40, (reason) => closed.push(`menu:${reason}`)));
    store.register(overlay("context", 80, (reason) => closed.push(`context:${reason}`)));

    expect(store.dismissEscape()).toBe(true);
    expect(closed).toEqual(["context:escape"]);
  });

  it("uses registration order for equal-priority layers", () => {
    const store = new OverlayStore();
    const closed: string[] = [];
    store.register(overlay("first", 40, () => closed.push("first")));
    store.register(overlay("second", 40, () => closed.push("second")));

    store.dismissEscape();
    expect(closed).toEqual(["second"]);
  });

  it("keeps the topmost layer open for pointers inside it", () => {
    const store = new OverlayStore();
    const closed: string[] = [];
    const inside = {} as Node;
    store.register({ ...overlay("menu", 40, () => closed.push("menu")), contains: (target) => target === inside });

    expect(store.dismissOutsidePointer(inside)).toBe(false);
    expect(store.dismissOutsidePointer({} as Node)).toBe(true);
    expect(closed).toEqual(["menu"]);
  });
});
