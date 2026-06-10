import { expect, test, type Page } from "@playwright/test";
import AxeBuilder from "@axe-core/playwright";

const WCAG_TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"];

async function openMockRepo(page: Page) {
  await page.goto("/");
  await expect(page.getByRole("button", { name: /Open a repository/ })).toBeVisible();
  await page.getByRole("button", { name: /Open a repository/ }).click();
  await expect(page.getByText("Risk Flags")).toBeVisible();
  await expect(page.getByText("Loose regex pattern").first()).toBeVisible();
  // Flag cards surface the node context they jump to.
  await expect(page.getByText("validateToken › pattern")).toBeVisible();
}

async function expectNoAxeViolations(page: Page) {
  const results = await new AxeBuilder({ page }).include(".window").withTags(WCAG_TAGS).analyze();
  expect(results.violations).toEqual([]);
}

test.describe("Diff Drift browser-mode E2E", () => {
  test("onboarding, loaded, and dismissed states pass automated axe checks", async ({ page }) => {
    await page.goto("/");
    await expect(page.getByText("v0.2.1")).toBeVisible();
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

    // Analysis scope: defaults to current uncommitted work, with other compare
    // modes tucked behind an explanatory popover.
    const scope = page.getByTestId("scope-trigger");
    await expect(scope).toContainText("Current work");
    await scope.click();
    await expect(page.getByRole("dialog", { name: "Analysis scope" })).toBeVisible();
    await expect(page.getByRole("button", { name: /Since last review/ })).toBeDisabled();
    await page.getByRole("button", { name: /Entire branch/ }).click();
    await expect(scope).toContainText("Entire branch");
    await scope.click();
    await page.getByRole("button", { name: /Current work/ }).click();
    await expect(scope).toContainText("Current work");

    // Review-at-scale: toggling one node updates the drift-wide progress.
    await expect(page.getByText("0/6 reviewed")).toBeVisible();
    await page.getByRole("button", { name: /Mark reviewed: VariableDeclaration pattern/ }).click();
    await expect(page.getByText("1/6 reviewed")).toBeVisible();
    await expect(page.getByText("1/5 reviewed")).toBeVisible(); // file-level legend
    await page.getByRole("button", { name: /Mark unreviewed: VariableDeclaration pattern/ }).click();
    await expect(page.getByText("0/6 reviewed")).toBeVisible();

    await page.getByRole("button", { name: /Dismiss flag: Loose regex pattern/ }).click();
    await expect(page.getByText("Risk Flags").locator("..").getByText("2")).toBeVisible();

    await page.getByRole("button", { name: /Dismissed/ }).click();
    await page.getByRole("button", { name: /Restore flag: Loose regex pattern/ }).click();
    await expect(page.getByText("Risk Flags").locator("..").getByText("3")).toBeVisible();

    await page.getByRole("button", { name: "Dismiss all" }).click();
    await expect(page.getByRole("button", { name: "Dismiss all" })).toBeDisabled();
    await expect(page.getByText("No active risk flags")).toBeVisible();

    await page.getByRole("button", { name: "Mark reviewed", exact: true }).click();
    await expect(page.getByText(/Reviewed at/)).toBeVisible();
    // Reviewing the drift reviews every node.
    await expect(page.getByText("6/6 reviewed")).toBeVisible();

    // Mark reviewed pinned a trust point, so the last-review scope unlocks.
    await scope.click();
    const trustOption = page.getByRole("button", { name: /Since last review/ });
    await expect(trustOption).toBeEnabled();
    await expect(trustOption).toContainText("Trust point ab12cd3");
    await trustOption.click();
    await expect(scope).toContainText("Since last review");

    const downloadPromise = page.waitForEvent("download");
    await page.getByRole("button", { name: "Export report" }).click();
    const download = await downloadPromise;
    expect(download.suggestedFilename()).toBe("diff-drift-payments-api.json");
    await expect(page.getByRole("button", { name: /Exported/ })).toBeVisible();
  });
});
