import type { ReactElement } from "react";
import { describe, expect, it } from "vitest";

import { ToolbarItem } from "../src/ToolbarItem.js";
import type { ResolvedToolbarDescriptor } from "@open-office/toolbar-core";

describe("toolbar-react descriptor renderer", () => {
  it("emits stable descriptor identity and keyboard semantics", () => {
    const item: ResolvedToolbarDescriptor<"bold"> = {
      id: "bold",
      group: "text",
      kind: "toggle",
      action: "bold",
      label: "加粗",
      shortcut: "Mod+B",
      enabled: true,
      active: true,
    };

    const rendered = ToolbarItem({ item, onAction: () => undefined }) as ReactElement;
    expect(rendered.props["data-toolbar-id"]).toBe("bold");
    expect(rendered.props["data-toolbar-kind"]).toBe("toggle");
    expect(rendered.props["aria-keyshortcuts"]).toBe("Mod+B");
    expect(rendered.props["aria-pressed"]).toBe(true);
    expect(rendered.props.title).toBe("加粗 (Mod+B)");
  });

  it("keeps menu descriptors keyboard discoverable", () => {
    const item: ResolvedToolbarDescriptor<"insert"> = {
      id: "insert-menu",
      group: "insert",
      kind: "menu",
      action: "insert",
      label: "插入",
      shortcut: "Mod+Shift+I",
      enabled: true,
      active: false,
    };

    const rendered = ToolbarItem({ item, onAction: () => undefined }) as ReactElement;
    expect(rendered.props["aria-haspopup"]).toBe("menu");
    expect(rendered.props["aria-keyshortcuts"]).toBe("Mod+Shift+I");
    expect(rendered.props["data-toolbar-kind"]).toBe("menu");
  });
});
