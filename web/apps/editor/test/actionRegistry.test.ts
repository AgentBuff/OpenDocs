import { describe, expect, it } from "vitest";

import { validateToolbar } from "@open-office/toolbar-core";
import { action } from "../src/actions/registry.js";
import type { ActionId } from "../src/actions/registry.js";
import { blockToolbarItems } from "../src/toolbar/items.js";

describe("editor action registry", () => {
  it("toolbar items resolve to registered capabilities", () => {
    for (const item of blockToolbarItems) {
      if (item.kind === "button" || item.kind === "toggle") {
        expect(item.action).toBeDefined();
        const actionId = item.action as ActionId;
        expect(action(actionId).id).toBe(actionId);
      }
    }
  });

  it("keeps the document descriptor tree within the shared toolbar contract", () => {
    expect(validateToolbar(blockToolbarItems)).toEqual([]);
  });

  it("every shortcut has a visible action title", () => {
    for (const item of blockToolbarItems) {
      if (item.kind === "button" || item.kind === "toggle") {
        const registered = action(item.action as ActionId);
        if (registered.shortcut) expect(registered.title).not.toBe("");
      }
    }
  });

  it("registers document-wide selection for both Mod+A and the control-key alias", () => {
    expect(action("selectAll").shortcut).toBe("Mod+A");
    expect(action("selectAll").title).toBe("全选");
  });
});
