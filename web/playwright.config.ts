import { defineConfig, devices } from "@playwright/test";

const apiPort = 8788;
const webPort = 5175;
const apiOrigin = `http://127.0.0.1:${apiPort}`;
const webOrigin = `http://127.0.0.1:${webPort}`;

export default defineConfig({
  testDir: "./apps/editor/e2e",
  outputDir: "./test-results",
  snapshotPathTemplate: "{testDir}/{testFilePath}-snapshots/{arg}{ext}",
  fullyParallel: false,
  forbidOnly: Boolean(process.env.CI),
  retries: process.env.CI ? 1 : 0,
  workers: 1,
  reporter: process.env.CI ? [["github"], ["html", { open: "never" }]] : [["list"], ["html", { open: "never" }]],
  use: {
    baseURL: webOrigin,
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "retain-on-failure",
    viewport: { width: 1440, height: 960 },
    deviceScaleFactor: 1,
    colorScheme: "light",
    locale: "zh-CN",
  },
  expect: { timeout: 10_000 },
  webServer: [
    {
      command: `OO_BIND=127.0.0.1:${apiPort} OO_DATA_DIR=.e2e-data cargo run -p oo-server`,
      cwd: "..",
      url: `${apiOrigin}/api/health`,
      timeout: 120_000,
      reuseExistingServer: !process.env.CI,
    },
    {
      // `pnpm run dev -- --port` is forwarded to the package script as a
      // literal `--` by the recursive runner in this workspace, so Vite keeps
      // its normal 5174 config port while Playwright waits for 5175. Invoke
      // the Vite binary directly to make the test server port deterministic.
      command: `OO_API=${apiOrigin} pnpm --filter @open-office/editor exec vite --host 127.0.0.1 --port ${webPort}`,
      cwd: ".",
      url: webOrigin,
      timeout: 120_000,
      reuseExistingServer: !process.env.CI,
    },
  ],
  projects: [
    { name: "chromium", use: { ...devices["Desktop Chrome"] } },
  ],
});
