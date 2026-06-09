import { expect, test, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

const WCAG_TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"];

async function openMockRepo(page: Page) {
  await page.goto("/");
  await expect(page.getByRole("button", { name: /Open a repository/ })).toBeVisible();
  await page.getByRole("button", { name: /Open a repository/ }).click();
  await expect(page.getByText("Risk Flags")).toBeVisible();
  await expect(page.getByText("Loose regex pattern").first()).toBeVisible();
}

async function expectNoAxeViolations(page: Page) {
  const results = await new AxeBuilder({ page }).include(".window").withTags(WCAG_TAGS).analyze();
  expect(results.violations).toEqual([]);
}

test.describe("Diff Drift browser-mode E2E", () => {
  test("onboarding, loaded, and dismissed states pass automated axe checks", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByText("v0.1.1")).toBeVisible();
    await expect(page.getByRole("button", { name: /Open a repository/ })).toBeVisible();
    await expectNoAxeViolations(page);

    await page.getByRole("button", { name: /Open a repository/ }).click();
    await expect(page.getByText("Risk Flags")).toBeVisible();
    await expectNoAxeViolations(page);

    await page.getByRole("button", { name: "Dismiss all" }).click();
    await expect(page.getByText("No active risk flags")).toBeVisible();
    await expect(page.getByText(/No flags in 3 changed files/)).toBeVisible();
    await expectNoAxeViolations(page);
  });

  test("triage, approval, and browser export feedback are interactive", async ({ page }) => {
    await openMockRepo(page);

    await page.getByRole("button", { name: /Dismiss flag: Loose regex pattern/ }).click();
    await expect(page.getByText("Risk Flags").locator("..").getByText("2")).toBeVisible();

    await page.getByRole("button", { name: /Dismissed/ }).click();
    await page.getByRole("button", { name: /Restore flag: Loose regex pattern/ }).click();
    await expect(page.getByText("Risk Flags").locator("..").getByText("3")).toBeVisible();

    await page.getByRole("button", { name: "Dismiss all" }).click();
    await expect(page.getByRole("button", { name: "Dismiss all" })).toBeDisabled();
    await expect(page.getByText("No active risk flags")).toBeVisible();

    await page.getByRole("button", { name: "Mark reviewed" }).click();
    await expect(page.getByText(/Reviewed at/)).toBeVisible();

    const downloadPromise = page.waitForEvent("download");
    await page.getByRole("button", { name: "Export report" }).click();
    const download = await downloadPromise;
    expect(download.suggestedFilename()).toBe("diff-drift-payments-api.json");
    await expect(page.getByRole("button", { name: /Exported/ })).toBeVisible();
  });
});
