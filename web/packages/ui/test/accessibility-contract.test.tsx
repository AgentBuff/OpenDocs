import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  Icon,
  MenuItem,
  MenuPanel,
  Toolbar,
  ToolbarButton,
  ToolbarMenuButton,
  ToolbarSeparator,
} from "../src/index.js";

describe("public UI accessibility contract", () => {
  it("keeps toolbar controls keyboard-addressable and named", () => {
    const markup = renderToStaticMarkup(
      <Toolbar aria-label="文档工具栏">
        <ToolbarButton aria-label="加粗" active>
          <Icon name="bold" />
        </ToolbarButton>
        <ToolbarSeparator />
        <ToolbarMenuButton aria-label="插入内容" aria-haspopup="menu">
          <Icon name="insert" />
        </ToolbarMenuButton>
      </Toolbar>,
    );

    expect(markup).toContain('role="toolbar"');
    expect(markup).toContain('aria-label="文档工具栏"');
    expect(markup).toContain('type="button"');
    expect(markup).toContain('aria-label="加粗"');
    expect(markup).toContain('aria-pressed="true"');
    expect(markup).toContain('role="separator"');
    expect(markup).toContain('aria-orientation="vertical"');
    expect(markup).toContain('aria-label="插入内容"');
    expect(markup).toContain('aria-haspopup="menu"');
    expect(markup).toContain('aria-hidden="true"');
  });

  it("keeps menu semantics explicit and icon-only affordances decorative", () => {
    const markup = renderToStaticMarkup(
      <MenuPanel aria-label="插入内容">
        <MenuItem icon={<Icon name="table" />} trailing={<Icon name="arrow-right" />}>
          表格
        </MenuItem>
      </MenuPanel>,
    );

    expect(markup).toContain('role="menu"');
    expect(markup).toContain('aria-label="插入内容"');
    expect(markup).toContain('role="menuitem"');
    expect(markup).toContain('type="button"');
    expect(markup).toContain('aria-hidden="true"');
  });

  it("resolves catalog-backed toolbar glyphs through the shared registry", () => {
    const markup = renderToStaticMarkup(
      <>
        <Icon name="brush" />
        <Icon name="eraser" />
        <Icon name="font-colors" />
        <Icon name="bg-colors" />
        <Icon name="merge-cells" />
        <Icon name="vertical-align" />
      </>,
    );

    expect(markup.match(/<svg/g)?.length).toBe(6);
    expect(markup).toContain('viewBox="0 0 48 48"');
    expect(markup).toContain('stroke="currentColor"');
  });
});
