import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { ThemeProvider, ThemeRuntime, useTheme, useThemeRuntime } from "../src/foundation/theme.js";

function ThemeProbe() {
  const theme = useTheme();
  return <output>{`${theme.theme}:${theme.mode}:${theme.density}`}</output>;
}

function RuntimeProbe() {
  const theme = useThemeRuntime();
  return <output>{`${theme.theme}:${theme.mode}:${theme.density}`}</output>;
}

describe("theme provider contract", () => {
  it("exposes the canonical dark skin and independent density", () => {
    const markup = renderToStaticMarkup(
      <ThemeProvider theme="office-dark" density="compact">
        <ThemeProbe />
      </ThemeProvider>,
    );

    expect(markup).toContain('class="oo-theme-root oo-theme--office-dark oo-theme--dark"');
    expect(markup).toContain('data-theme="dark"');
    expect(markup).toContain('data-density="compact"');
    expect(markup).toContain("office-dark:dark:compact");
  });

  it("provides a product-level switch boundary without coupling to document state", () => {
    const markup = renderToStaticMarkup(
      <ThemeRuntime initialTheme="office-dark" storageKey={null} density="compact">
        <RuntimeProbe />
      </ThemeRuntime>,
    );

    expect(markup).toContain('class="oo-theme-root oo-theme--office-dark oo-theme--dark"');
    expect(markup).toContain('data-theme="dark"');
    expect(markup).toContain("office-dark:dark:compact");
  });
});
