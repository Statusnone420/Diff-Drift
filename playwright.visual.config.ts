import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "tests/e2e-visual",
  timeout: 30_000,
  expect: {
    timeout: 5_000,
    toHaveScreenshot: {
      maxDiffPixelRatio: 0.02,
    },
  },
  reporter: [
    ["list"],
    ["html", { open: "never", outputFolder: "playwright-report/visual" }],
    ["json", { outputFile: ".test-results/playwright-visual.json" }],
  ],
  use: {
    baseURL: "http://127.0.0.1:1438",
    viewport: { width: 1440, height: 900 },
    trace: "on-first-retry",
  },
  webServer: {
    command: "npm run dev -- --host 127.0.0.1 --port 1438",
    url: "http://127.0.0.1:1438",
    reuseExistingServer: true,
    timeout: 120_000,
    stdout: "pipe",
    stderr: "pipe",
  },
});
