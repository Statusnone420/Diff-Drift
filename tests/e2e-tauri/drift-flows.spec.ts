import { execFile } from "node:child_process";
import { mkdir, mkdtemp, rm, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import { expect, launchDiffDriftApp, test, type DiffDriftAppInstance } from "./tauriApp";

const execFileAsync = promisify(execFile);

test("native watcher reacts to regex insertion, signature mutation, export, and dismissal", async ({}, testInfo) => {
  const frontendRoot = testInfo.config.configFile
    ? path.dirname(testInfo.config.configFile)
    : process.cwd();
  const repoRoot = await createFixtureRepo();
  const e2eRoot = await mkdtemp(path.join(tmpdir(), "diff-drift-e2e-state-"));
  const exportPath = path.join(e2eRoot, "diff-drift-report.md");
  const statePath = path.join(e2eRoot, "repo-state.json");
  let app: DiffDriftAppInstance | undefined;

  try {
    app = await launchDiffDriftApp(frontendRoot, { repoRoot, exportPath, statePath });
    const page = app.page;

    await expect(page.locator(".window")).toBeVisible();
    await expect(page.locator(".center-clean-title")).toHaveText(/No drift detected/);

    await writeFile(path.join(repoRoot, "auth.ts"), "const parser = /.*/;\n", "utf8");
    await expect(page.locator(".flag.high .flag-type")).toHaveText("Loose regex pattern", {
      timeout: 15_000,
    });
    await expect(page.locator(".summary-pill")).toContainText(/1 risks? across 1 files?/);

    await writeFile(path.join(repoRoot, "src", "api.ts"), signatureMutationApi(), "utf8");
    await page.getByRole("button", { name: /src\/api\.ts, 0 risks/ }).click();
    const modifiedFunction = page.locator(".node-card.state-modified").filter({
      hasText: "parseToken",
    });
    await expect(modifiedFunction).toBeVisible({ timeout: 15_000 });
    await expect(modifiedFunction.locator(".node-sig")).toHaveText(
      "(token: string, param: any): boolean",
    );

    await page.getByRole("button", { name: "Export report" }).click();
    await expect.poll(() => existsSync(exportPath), { timeout: 10_000 }).toBe(true);
    const report = await import("node:fs/promises").then((fs) => fs.readFile(exportPath, "utf8"));
    expect(report).toMatch(/^# Diff Drift report/m);
    expect(report).toContain("Loose regex pattern");
    expect(report).toContain("auth.ts");

    await page.getByRole("button", { name: "Dismiss all" }).click();
    await expect(page.locator(".rp-empty-title")).toHaveText("No active risk flags");
    await expect(page.locator(".rp-title .tcount")).toHaveText("0");
    await expect(page.locator(".summary-pill")).toContainText(/No risks/);
  } finally {
    await app?.close();
    await removeTestRoot(repoRoot);
    await removeTestRoot(e2eRoot);
  }
});

async function createFixtureRepo(): Promise<string> {
  const repoRoot = await mkdtemp(path.join(tmpdir(), "diff-drift-e2e-repo-"));
  await mkdir(path.join(repoRoot, "src"), { recursive: true });
  await writeFile(
    path.join(repoRoot, "package.json"),
    JSON.stringify({ name: "diff-drift-e2e-fixture", version: "0.0.0" }, null, 2),
    "utf8",
  );
  await writeFile(path.join(repoRoot, "src", "api.ts"), baselineApi(), "utf8");
  await execFileAsync("git", ["init"], { cwd: repoRoot });
  await execFileAsync("git", ["add", "-A"], { cwd: repoRoot });
  await execFileAsync(
    "git",
    [
      "-c",
      "user.email=e2e@diffdrift.local",
      "-c",
      "user.name=Diff Drift E2E",
      "commit",
      "-m",
      "baseline",
    ],
    { cwd: repoRoot },
  );
  return repoRoot;
}

function baselineApi() {
  return `function parseToken(token: string): boolean {
  return token.length > 0;
}

export { parseToken };
`;
}

function signatureMutationApi() {
  return `function parseToken(token: string, param: any): boolean {
  return token.length > 0;
}

export { parseToken };
`;
}

async function removeTestRoot(root: string) {
  await rm(root, {
    force: true,
    maxRetries: 3,
    recursive: true,
    retryDelay: 250,
  });
}
