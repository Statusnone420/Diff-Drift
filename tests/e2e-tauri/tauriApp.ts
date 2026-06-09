import {
  chromium,
  expect,
  test as base,
  type Browser,
  type BrowserContext,
  type Page,
} from "@playwright/test";
import { spawn, type ChildProcess } from "node:child_process";
import { mkdir, mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import path from "node:path";
import { createServer, type AddressInfo } from "node:net";

const STARTUP_TIMEOUT_MS = 30_000;
const CLEANUP_TIMEOUT_MS = 10_000;
const PROCESS_LOG_LIMIT = 8_000;

export interface DiffDriftLaunchOptions {
  repoRoot: string;
  exportPath: string;
  statePath: string;
}

export interface DiffDriftAppInstance {
  page: Page;
  close: () => Promise<void>;
  output: () => string;
  tempRoot: string;
}

export const test = base;
export { expect };

export async function launchDiffDriftApp(
  frontendRoot: string,
  options: DiffDriftLaunchOptions,
): Promise<DiffDriftAppInstance> {
  const executablePath =
    process.env.DIFF_DRIFT_E2E_BIN ??
    path.join(
      frontendRoot,
      "src-tauri",
      "target",
      "debug",
      process.platform === "win32" ? "diff-drift.exe" : "diff-drift",
    );
  const cdpPort = await findFreePort();
  const tempRoot = await mkdtemp(path.join(tmpdir(), "diff-drift-e2e-"));
  const userDataFolder = path.join(tempRoot, "WebView2");
  await mkdir(userDataFolder, { recursive: true });

  let stdout = "";
  let stderr = "";
  let browser: Browser | undefined;

  const app = spawn(executablePath, [], {
    cwd: frontendRoot,
    env: {
      ...process.env,
      WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS: `--remote-debugging-port=${cdpPort}`,
      WEBVIEW2_USER_DATA_FOLDER: userDataFolder,
      DIFF_DRIFT_E2E_REPO: options.repoRoot,
      DIFF_DRIFT_E2E_EXPORT_PATH: options.exportPath,
      DIFF_DRIFT_E2E_STATE_FILE: options.statePath,
    },
    stdio: ["ignore", "pipe", "pipe"],
    windowsHide: true,
  });

  app.stdout?.on("data", (chunk: Buffer) => {
    stdout = appendBounded(stdout, chunk);
  });
  app.stderr?.on("data", (chunk: Buffer) => {
    stderr = appendBounded(stderr, chunk);
  });
  app.once("error", (error) => {
    stderr = appendBounded(stderr, Buffer.from(`spawn error: ${error.message}\n`));
  });

  async function close() {
    await browser?.close().catch(() => undefined);
    await killProcessTree(app);
    await rm(tempRoot, {
      force: true,
      maxRetries: 3,
      recursive: true,
      retryDelay: 250,
    });
  }

  const processOutput = () => `stdout:\n${stdout || "(empty)"}\n\nstderr:\n${stderr || "(empty)"}`;

  try {
    await waitForCdp(cdpPort, app, processOutput);
    browser = await chromium.connectOverCDP(`http://127.0.0.1:${cdpPort}`);
    const context = browser.contexts()[0];
    if (!context) {
      throw new Error("WebView2 CDP connection did not expose a browser context.");
    }

    const page = await firstPage(context);
    await page.bringToFront().catch(() => undefined);
    await page.waitForLoadState("domcontentloaded");

    return { page, close, output: processOutput, tempRoot };
  } catch (error) {
    await close();
    throw error;
  }
}

async function findFreePort(): Promise<number> {
  const server = createServer();
  await new Promise<void>((resolve, reject) => {
    server.once("error", reject);
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address() as AddressInfo;
  await new Promise<void>((resolve, reject) => {
    server.close((error) => (error ? reject(error) : resolve()));
  });
  return address.port;
}

async function waitForCdp(
  port: number,
  app: ChildProcess,
  processOutput: () => string,
): Promise<void> {
  const deadline = Date.now() + STARTUP_TIMEOUT_MS;
  let lastError = "";

  while (Date.now() < deadline) {
    if (app.exitCode !== null || app.signalCode !== null) {
      throw new Error(
        `Diff Drift exited before CDP was ready. exit=${app.exitCode} signal=${app.signalCode}\n${processOutput()}`,
      );
    }

    try {
      const response = await fetch(`http://127.0.0.1:${port}/json/version`);
      if (response.ok) return;
      lastError = `HTTP ${response.status}`;
    } catch (error) {
      lastError = error instanceof Error ? error.message : String(error);
    }

    await delay(250);
  }

  throw new Error(
    `Timed out waiting for WebView2 CDP on port ${port}. Last error: ${lastError}\n${processOutput()}`,
  );
}

async function firstPage(context: BrowserContext): Promise<Page> {
  return context.pages()[0] ?? context.waitForEvent("page", { timeout: STARTUP_TIMEOUT_MS });
}

async function killProcessTree(app: ChildProcess): Promise<void> {
  if (app.exitCode !== null || app.signalCode !== null || app.pid === undefined) return;

  if (process.platform === "win32") {
    await new Promise<void>((resolve) => {
      const killer = spawn("taskkill", ["/PID", String(app.pid), "/T", "/F"], {
        stdio: "ignore",
        windowsHide: true,
      });
      const timer = setTimeout(resolve, CLEANUP_TIMEOUT_MS);
      killer.once("error", () => {
        clearTimeout(timer);
        resolve();
      });
      killer.once("exit", () => {
        clearTimeout(timer);
        resolve();
      });
    });
    return;
  }

  app.kill("SIGTERM");
  await Promise.race([waitForExit(app), delay(CLEANUP_TIMEOUT_MS)]);
  if (app.exitCode === null && app.signalCode === null) app.kill("SIGKILL");
}

function waitForExit(app: ChildProcess): Promise<void> {
  return new Promise((resolve) => {
    if (app.exitCode !== null || app.signalCode !== null) {
      resolve();
      return;
    }
    app.once("exit", () => resolve());
  });
}

function appendBounded(current: string, chunk: Buffer): string {
  const next = current + chunk.toString("utf8");
  return next.length > PROCESS_LOG_LIMIT ? next.slice(-PROCESS_LOG_LIMIT) : next;
}

function delay(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}
