import { defineConfig } from "@playwright/test";

export default defineConfig({
  testDir: "tests/e2e-tauri",
  timeout: 60_000,
  globalTimeout: 8 * 60_000,
  workers: 1,
  reporter: [
    ["list"],
    ["html", { open: "never", outputFolder: "playwright-report/tauri" }],
    ["json", { outputFile: ".test-results/playwright-tauri.json" }],
  ],
  use: {
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
    video: "retain-on-failure",
  },
});
