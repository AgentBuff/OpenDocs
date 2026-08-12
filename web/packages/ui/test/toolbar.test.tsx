import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ToolbarButton, ToolbarField, ToolbarSelect, ToolbarSeparator, ToolbarSplitGroup } from "../src/navigation/toolbar.js";

describe("toolbar controls", () => {
  it("renders a themed action contract with active state and accessible name", () => {
    const markup = renderToStaticMarkup(
      <ToolbarButton active aria-label="加粗" title="加粗">
        B
      </ToolbarButton>,
    );

    expect(markup).toContain('class="oo-toolbar__item oo-toolbar__item--default is-active"');
    expect(markup).toContain('aria-label="加粗"');
    expect(markup).toContain('aria-pressed="true"');
  });

  it("keeps combobox, field and separator semantics in the UI package", () => {
    const markup = renderToStaticMarkup(
      <>
        <ToolbarField aria-label="行距"><ToolbarSelect aria-label="行距" compact options={[{ value: "1.5", label: "1.5" }]} /></ToolbarField>
        <ToolbarSeparator />
      </>,
    );

    expect(markup).toContain('class="oo-toolbar__field"');
    expect(markup).toContain('class="oo-toolbar__select oo-toolbar__select--compact"');
    expect(markup).toContain('role="combobox"');
    expect(markup).toContain('aria-haspopup="listbox"');
    expect(markup).not.toContain("<select");
    expect(markup).toContain('role="separator"');
  });

  it("keeps split actions as a semantic group instead of merging the arrow into the label", () => {
    const markup = renderToStaticMarkup(
      <ToolbarSplitGroup aria-label="字体颜色">
        <ToolbarButton aria-label="字体颜色">A</ToolbarButton>
        <ToolbarButton aria-label="字体颜色更多选项">⌄</ToolbarButton>
      </ToolbarSplitGroup>,
    );

    expect(markup).toContain('class="oo-toolbar__split"');
    expect(markup).toContain('role="group"');
    expect(markup).toContain('aria-label="字体颜色"');
    expect(markup.match(/class="oo-toolbar__item/g)?.length).toBe(2);
  });
});
