import { expect, test, type Page } from "@playwright/test";

async function openMockRepo(page: Page) {
  await page.goto("/");
  await page.getByRole("button", { name: /Open a repository/ }).click();
  await expect(page.getByText("Risk Flags")).toBeVisible();
  await expect(page.getByText("Loose regex pattern").first()).toBeVisible();
}

function screenshotOptions(page: Page) {
  return {
    mask: [page.locator(".tb-version")],
  };
}

test.describe("Diff Drift visual baselines", () => {
  test("onboarding", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByRole("button", { name: /Open a repository/ })).toBeVisible();
    await expect(page.locator(".window")).toHaveScreenshot(
      "onboarding.png",
      screenshotOptions(page),
    );
  });

  test("loaded session", async ({ page }) => {
    await openMockRepo(page);
    await expect(page.locator(".window")).toHaveScreenshot(
      "loaded-session.png",
      screenshotOptions(page),
    );
  });

  test("dismissed state", async ({ page }) => {
    await openMockRepo(page);
    await page.getByRole("button", { name: "Dismiss all" }).click();
    await expect(page.getByText("No active risk flags")).toBeVisible();
    await expect(page.locator(".window")).toHaveScreenshot(
      "dismissed-state.png",
      screenshotOptions(page),
    );
  });
});
