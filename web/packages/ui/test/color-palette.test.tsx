import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ColorPalette } from "../src/index.js";

describe("ColorPalette", () => {
  it("uses semantic menu controls and keeps the default color selectable", () => {
    const markup = renderToStaticMarkup(
      <ColorPalette role="text" value={null} recentColors={["#165dff"]} onValueChange={() => {}} />,
    );
    expect(markup).toContain('role="menu"');
    expect(markup).toContain('aria-label="字体颜色选择"');
    expect(markup).toContain('aria-checked="true"');
    expect(markup).toContain("最近使用");
    expect(markup).toContain("#165dff");
  });
});
