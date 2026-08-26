import { defineConfig } from "vitest/config";

/**
 * Unit tests and browser scenarios have deliberately separate runners. The
 * default Vitest discovery pattern also matches Playwright's `*.spec.ts`
 * files, which makes `pnpm test` import a second test runtime and fail before
 * any unit assertion executes.
 */
export default defineConfig({
  test: {
    exclude: [
      "apps/editor/e2e/**",
      "test-results/**",
      "node_modules/**",
    ],
  },
});
