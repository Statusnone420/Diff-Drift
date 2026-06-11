// Captures the 60-90 second demo MP4 without a human screen-recording session.
//
// Same six beats as docs/demo/demo-script.md (and capture-demo.mjs), but paced
// for video instead of GIF keyframes: real-time holds, a smooth fake cursor, a
// title card, and a closing card with the CLI one-liner and the GitHub Action
// snippet. Playwright records the session as WebM; system ffmpeg converts it
// to H.264 MP4 (Playwright's bundled ffmpeg only encodes WebM).
//
//   npm run demo:video
//
// Output: .eval/demo/diff-drift-demo.mp4 (gitignored — attach it to the GitHub
// release; it is not a repo file). If ffmpeg is missing the WebM is kept and
// the script tells you how to install ffmpeg (winget install -e --id Gyan.FFmpeg).

import { spawn, spawnSync } from "node:child_process";
import { mkdirSync, copyFileSync, existsSync, statSync, globSync } from "node:fs";
import { join } from "node:path";
import { chromium } from "@playwright/test";

const PORT = 51736;
const BASE_URL = `http://localhost:${PORT}`;
const VIEWPORT = { width: 1280, height: 800 };
const OUT_DIR = join(process.cwd(), ".eval", "demo");
const MP4_PATH = join(OUT_DIR, "diff-drift-demo.mp4");
const WEBM_PATH = join(OUT_DIR, "diff-drift-demo.webm");

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function waitForServer(url, timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(url);
      if (res.ok) return;
    } catch {
      // not up yet
    }
    await sleep(250);
  }
  throw new Error(`Dev server did not come up at ${url}`);
}

/**
 * In-page overlay: fake cursor (screencasts don't include the real one), a
 * lower-third caption bar, and a full-screen card layer for the title and
 * closing frames. Idempotent — safe to reinstall after React swaps the DOM.
 */
async function installOverlay(page) {
  await page.evaluate(() => {
    for (const id of ["demo-style", "demo-cursor", "demo-caption", "demo-card"]) {
      document.getElementById(id)?.remove();
    }
    const style = document.createElement("style");
    style.id = "demo-style";
    style.textContent = `
      #demo-cursor { position: fixed; z-index: 100000; width: 22px; height: 22px;
        pointer-events: none; left: 0; top: 0; transform: translate(-2px, -2px); }
      #demo-caption { position: fixed; z-index: 99999; left: 50%; bottom: 34px;
        transform: translateX(-50%) translateY(8px); pointer-events: none;
        background: rgba(10, 12, 16, 0.92); color: #f4f6f8;
        font: 600 26px/1.3 "Segoe UI", system-ui, sans-serif;
        letter-spacing: 0.01em; padding: 14px 26px; border-radius: 12px;
        border: 1px solid rgba(255, 255, 255, 0.14);
        box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
        white-space: nowrap; opacity: 0;
        transition: opacity 260ms ease, transform 260ms ease; }
      #demo-caption.show { opacity: 1; transform: translateX(-50%) translateY(0); }
      #demo-card { position: fixed; z-index: 100001; inset: 0; pointer-events: none;
        display: flex; align-items: center; justify-content: center;
        background: #0a0c10; color: #f4f6f8; opacity: 0;
        transition: opacity 450ms ease; }
      #demo-card.show { opacity: 1; }
      #demo-card .inner { max-width: 980px; text-align: center; }
      #demo-card h1 { font: 700 64px/1.1 "Segoe UI", system-ui, sans-serif; margin: 0 0 18px; }
      #demo-card p.tag { font: 400 30px/1.4 "Segoe UI", system-ui, sans-serif;
        color: #c9d1d9; margin: 0 0 10px; }
      #demo-card p.small { font: 400 22px/1.4 "Segoe UI", system-ui, sans-serif;
        color: #8b949e; margin: 0; }
      #demo-card pre { text-align: left; display: inline-block;
        background: #11151c; border: 1px solid rgba(255,255,255,0.12);
        border-radius: 10px; padding: 18px 26px; margin: 10px 0 0;
        font: 500 22px/1.5 "Cascadia Code", Consolas, monospace; color: #e6edf3; }
      #demo-card h2 { font: 600 34px/1.2 "Segoe UI", system-ui, sans-serif; margin: 0 0 14px; }
    `;
    document.head.appendChild(style);

    const cursor = document.createElement("div");
    cursor.id = "demo-cursor";
    cursor.innerHTML = `<svg viewBox="0 0 24 24" width="22" height="22">
      <path d="M5 2 L5 19 L9.5 15.2 L12.3 21.5 L15 20.3 L12.2 14.2 L18 14 Z"
        fill="#fff" stroke="#1a1d23" stroke-width="1.4" stroke-linejoin="round"/></svg>`;
    document.body.appendChild(cursor);

    const caption = document.createElement("div");
    caption.id = "demo-caption";
    document.body.appendChild(caption);

    const card = document.createElement("div");
    card.id = "demo-card";
    card.innerHTML = `<div class="inner"></div>`;
    document.body.appendChild(card);

    window.__demo = {
      cursor(x, y) {
        cursor.style.left = `${x}px`;
        cursor.style.top = `${y}px`;
      },
      caption(text) {
        caption.textContent = text;
        caption.classList.toggle("show", Boolean(text));
      },
      card(html) {
        card.querySelector(".inner").innerHTML = html ?? "";
        card.classList.toggle("show", Boolean(html));
      },
    };
  });
}

function makeDriver(page) {
  let cursorAt = { x: VIEWPORT.width / 2, y: VIEWPORT.height - 60 };

  async function caption(text) {
    await page.evaluate((t) => window.__demo.caption(t), text);
    await sleep(320); // let the fade land before the beat starts
  }

  async function card(html) {
    await page.evaluate((h) => window.__demo.card(h), html);
    await sleep(520); // opacity transition
  }

  async function setCursor(x, y) {
    cursorAt = { x, y };
    await page.evaluate(({ x, y }) => window.__demo.cursor(x, y), cursorAt);
  }

  /** Glide the fake cursor to a locator's center so clicks read as human, then click. */
  async function moveAndClick(locator) {
    const box = await locator.boundingBox();
    if (!box) throw new Error(`No bounding box for ${locator}`);
    const to = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
    const from = cursorAt;
    const steps = 28;
    for (let i = 1; i <= steps; i += 1) {
      const t = i / steps;
      const ease = t * t * (3 - 2 * t); // smoothstep
      await setCursor(from.x + (to.x - from.x) * ease, from.y + (to.y - from.y) * ease);
      await sleep(14);
    }
    await sleep(180); // settle on the target before the click registers
    await page.mouse.click(to.x, to.y);
  }

  return { caption, card, setCursor, moveAndClick };
}

/** Find a real ffmpeg (Playwright's bundled one only encodes WebM). */
function findFfmpeg() {
  const candidates = [
    "ffmpeg",
    join(process.env.LOCALAPPDATA ?? "", "Microsoft", "WinGet", "Links", "ffmpeg.exe"),
    ...globSync(
      join(process.env.LOCALAPPDATA ?? "", "Microsoft", "WinGet", "Packages", "Gyan.FFmpeg*", "**", "bin", "ffmpeg.exe"),
    ),
  ];
  for (const candidate of candidates) {
    try {
      if (spawnSync(candidate, ["-version"], { stdio: "ignore" }).status === 0) return candidate;
    } catch {
      // not this one
    }
  }
  return null;
}

async function run() {
  mkdirSync(OUT_DIR, { recursive: true });

  console.log("Starting vite dev server...");
  const vite = spawn(process.execPath, ["node_modules/vite/bin/vite.js", "--port", String(PORT), "--strictPort"], {
    stdio: "ignore",
  });
  const stopVite = () => {
    if (!vite.killed) vite.kill();
  };
  process.on("exit", stopVite);

  let videoSource;
  try {
    await waitForServer(BASE_URL);

    const browser = await chromium.launch();
    const context = await browser.newContext({
      viewport: VIEWPORT,
      recordVideo: { dir: OUT_DIR, size: VIEWPORT },
      acceptDownloads: true,
    });
    const page = await context.newPage();
    page.on("download", (d) => void d.cancel().catch(() => {}));
    const video = page.video();

    await page.goto(BASE_URL);
    await page.getByRole("button", { name: /Open a repository/ }).waitFor();
    await installOverlay(page);
    const d = makeDriver(page);

    // Title card up first; load the session behind it.
    await d.card(`
      <h1>Diff Drift</h1>
      <p class="tag">Review what an AI coding agent changed<br>since the last state you trusted.</p>
      <p class="small">Local and deterministic &middot; v0.3.2</p>
    `);
    await page.getByRole("button", { name: /Open a repository/ }).click();
    await page.getByText("Risk Flags").waitFor();
    await sleep(1000); // initial flag selection scrolls smoothly; let it settle
    // Clear the auto-selected flag so beat 3's flag click visibly lights the node up.
    await page.getByRole("button", { name: /auth\/validateToken\.ts, 2 flags/ }).click();
    await sleep(300);
    await installOverlay(page); // React replaced the DOM on state change; reinstall
    await d.card(`
      <h1>Diff Drift</h1>
      <p class="tag">Review what an AI coding agent changed<br>since the last state you trusted.</p>
      <p class="small">Local and deterministic &middot; v0.3.2</p>
    `);
    await sleep(3600);
    await d.card(null);
    await d.setCursor(VIEWPORT.width / 2, VIEWPORT.height - 60);
    await sleep(700);

    // Beat 1 — the loaded drift.
    await d.caption("An AI agent changed payments code.");
    await sleep(5200);

    // Beat 2 — baseline picker.
    await d.caption("Compare against a baseline you trust.");
    await sleep(900);
    await d.moveAndClick(page.getByTestId("scope-trigger"));
    await page.getByRole("dialog", { name: "Analysis scope" }).waitFor();
    await sleep(5600);
    await page.keyboard.press("Escape");
    await sleep(600);

    // Beat 3 — the risky node, before/after.
    await d.caption("Risky drift, node by node.");
    await sleep(700);
    await d.moveAndClick(page.getByRole("button", { name: /High severity: Loose regex pattern/ }));
    await sleep(1100); // smooth scroll + pulse
    await sleep(7400); // hold on the before/after regex — the money shot

    // Beat 4 — triage: review one node, dismiss one flag.
    await d.caption("Review what matters. Dismiss what doesn't.");
    await sleep(700);
    await d.moveAndClick(page.getByRole("button", { name: /Mark reviewed: VariableDeclaration pattern/ }));
    await sleep(2800);
    await d.moveAndClick(page.getByRole("button", { name: "Dismiss flag: Permissive logging config" }));
    await sleep(3200);

    // Beat 5 — mark the drift reviewed; the trust point unlocks the next baseline.
    await d.caption("Your review becomes the next baseline.");
    await sleep(700);
    await d.moveAndClick(page.getByRole("button", { name: "Mark reviewed", exact: true }));
    await page.getByText(/Reviewed at/).waitFor();
    await sleep(3000);
    await d.moveAndClick(page.getByTestId("scope-trigger"));
    await page.getByRole("dialog", { name: "Analysis scope" }).waitFor();
    await sleep(4400);
    await d.moveAndClick(page.getByRole("button", { name: /Since last review/ }));
    await sleep(2200);

    // Beat 6 — export evidence.
    await d.caption("Export evidence for the PR.");
    await sleep(700);
    await d.moveAndClick(page.getByRole("button", { name: "Export report" }));
    await page.getByRole("button", { name: /Exported/ }).waitFor();
    await sleep(3600);
    await d.caption("");

    // Closing card — how to try it.
    await d.card(`
      <h2>Try it</h2>
      <pre>diff-drift check . --baseline merge-base --md</pre>
      <pre>- uses: actions/checkout@v4
  with: { fetch-depth: 0 }
- uses: Statusnone420/Diff-Drift@v0.3.2
  with: { baseline: merge-base, fail-on: medium }</pre>
      <p class="small" style="margin-top:18px">github.com/Statusnone420/Diff-Drift &middot; runs locally, no model calls</p>
    `);
    await sleep(9000);

    await context.close(); // flushes the recording
    await browser.close();
    videoSource = await video.path();
  } finally {
    stopVite();
  }

  copyFileSync(videoSource, WEBM_PATH);
  console.log(`Recorded ${WEBM_PATH}`);

  const ffmpeg = findFfmpeg();
  if (!ffmpeg) {
    console.log("No system ffmpeg found — kept the WebM. For the MP4 install ffmpeg");
    console.log("(winget install -e --id Gyan.FFmpeg, then a new terminal) and rerun.");
    return;
  }
  console.log(`Converting to MP4 with ${ffmpeg}...`);
  const convert = spawnSync(
    ffmpeg,
    ["-y", "-i", WEBM_PATH, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "19", "-preset", "slow", "-r", "30", "-movflags", "+faststart", MP4_PATH],
    { stdio: ["ignore", "ignore", "inherit"] },
  );
  if (convert.status !== 0) throw new Error(`ffmpeg exited ${convert.status}`);
  const mb = (statSync(MP4_PATH).size / 1024 / 1024).toFixed(1);
  console.log(`Wrote ${MP4_PATH} (${mb} MB). Not a repo file — attach it to the GitHub release.`);
  if (existsSync(MP4_PATH)) {
    console.log("Keep or delete the intermediate WebM as you like:", WEBM_PATH);
  }
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
