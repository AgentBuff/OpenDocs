import { describe, expect, it } from "vitest";
import { groupToolbar, resolveToolbar, validateToolbar, type ToolbarDescriptor } from "../src/index.js";

interface Context {
  canInsert: boolean;
}

describe("toolbar-core", () => {
  const items: ToolbarDescriptor<"insert" | "undo", Context>[] = [
    { id: "undo", group: "history", kind: "button", action: "undo", label: "撤销", enabled: false },
    { id: "insert", group: "insert", kind: "button", action: "insert", label: "插入", visible: (context) => context.canInsert },
  ];

  it("resolves visibility and state without invoking a renderer", () => {
    expect(resolveToolbar(items, { canInsert: false })).toEqual([
      expect.objectContaining({ id: "undo", enabled: false, active: false }),
    ]);
    expect(resolveToolbar(items, { canInsert: true })).toHaveLength(2);
  });

  it("resolves visibility recursively for nested menu capabilities", () => {
    const menu: ToolbarDescriptor<"insert", Context>[] = [
      {
        id: "insert-menu",
        group: "insert",
        kind: "menu",
        label: "插入",
        children: [
          { id: "insert-image", group: "insert", kind: "button", label: "图片", action: "insert", visible: (context) => context.canInsert },
          { id: "insert-link", group: "insert", kind: "button", label: "链接", action: "insert", visible: false },
        ],
      },
    ];

    const resolved = resolveToolbar(menu, { canInsert: true });
    expect(resolved[0]?.children).toHaveLength(1);
    expect(resolved[0]?.children?.[0]?.id).toBe("insert-image");
    expect(resolveToolbar(menu, { canInsert: false })[0]?.children).toHaveLength(0);
  });

  it("groups descriptors in declaration order", () => {
    expect(groupToolbar(items).map((group) => group.id)).toEqual(["history", "insert"]);
  });

  it("rejects duplicate and unlabeled capabilities", () => {
    expect(validateToolbar([
      { id: "x", group: "g", kind: "button", action: "x" },
      { id: "x", group: "g", kind: "button", action: "x", label: "重复" },
    ])).toEqual(expect.arrayContaining([
      "toolbar item requires label or ariaLabel: x",
      "duplicate toolbar item id: x",
    ]));
  });

  it("validates nested ids, menu ownership and priority values", () => {
    expect(validateToolbar([
      {
        id: "menu",
        group: "insert",
        kind: "menu",
        label: "插入",
        children: [
          { id: "child", group: "insert", kind: "button", action: "x", label: "子项" },
          { id: "child", group: "insert", kind: "button", action: "x", label: "重复" },
        ],
      },
      { id: "bad-separator", group: "insert", kind: "separator", action: "x" },
      { id: "bad-priority", group: "insert", kind: "button", action: "x", label: "操作", priority: -1 },
      {
        id: "bad-children",
        group: "insert",
        kind: "button",
        action: "x",
        label: "操作",
        children: [{ id: "nested", group: "insert", kind: "button", action: "x", label: "嵌套" }],
      },
    ])).toEqual(expect.arrayContaining([
      "duplicate toolbar item id: child",
      "separator cannot define action or children: bad-separator",
      "toolbar priority must be a non-negative finite number: bad-priority",
      "toolbar children require menu kind: bad-children",
    ]));
  });

  it("groups large descriptor sets without mutating the source", () => {
    const source = Array.from({ length: 1000 }, (_, index) => ({
      id: `item-${index}`,
      group: `group-${index % 10}`,
      kind: "button" as const,
      action: "insert" as const,
      label: `项目 ${index}`,
    }));
    const snapshot = source.map((item) => ({ ...item }));
    const grouped = groupToolbar(source);

    expect(grouped).toHaveLength(10);
    expect(grouped.reduce((count, group) => count + group.items.length, 0)).toBe(source.length);
    expect(source).toEqual(snapshot);
  });
});
