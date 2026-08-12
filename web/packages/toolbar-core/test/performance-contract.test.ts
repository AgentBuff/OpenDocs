import { describe, expect, it } from "vitest";
import { resolveToolbar, type ToolbarDescriptor } from "../src/index.js";

interface Context {
  canInsert: boolean;
}

describe("toolbar-core performance contract", () => {
  it("resolves a large stable descriptor tree within the adapter budget", () => {
    const descriptors: ToolbarDescriptor<"insert", Context>[] = Array.from({ length: 1000 }, (_, index) => ({
      id: `item-${index}`,
      group: `group-${index % 10}`,
      kind: "button",
      action: "insert",
      label: `项目 ${index}`,
      enabled: (context) => context.canInsert,
      active: index % 7 === 0,
    }));

    // This is a smoke budget, not a machine benchmark: it catches accidental full-model
    // cloning or quadratic descriptor walks while leaving room for CI variance.
    const started = performance.now();
    for (let iteration = 0; iteration < 50; iteration += 1) {
      const resolved = resolveToolbar(descriptors, { canInsert: true });
      expect(resolved).toHaveLength(descriptors.length);
    }
    const elapsed = performance.now() - started;

    expect(elapsed).toBeLessThan(250);
  });
});
