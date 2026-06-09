import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "tests/e2e-web",
  timeout: 30_000,
  expect: {
    timeout: 5_000,
  },
  reporter: [
    ["list"],
    ["html", { open: "never", outputFolder: "playwright-report/web" }],
    ["json", { outputFile: ".test-results/playwright-web.json" }],
  ],
  use: {
    baseURL: "http://127.0.0.1:1437",
    viewport: { width: 1440, height: 900 },
    trace: "on-first-retry",
  },
  webServer: {
    command: "npm run dev -- --host 127.0.0.1 --port 1437",
    url: "http://127.0.0.1:1437",
    reuseExistingServer: false,
    timeout: 120_000,
    stdout: "pipe",
    stderr: "pipe",
  },
});
