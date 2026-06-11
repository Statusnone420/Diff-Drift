// Captures a first-pass v0.4 trailer MP4 and thumbnail.
//
// This is a release/artifact helper, not product runtime code. It drives the
// browser mock session with Playwright, overlays the v0.4 multi-language story,
// records WebM, then uses system ffmpeg to produce:
//   .eval/demo/diff-drift-v0.4-trailer.mp4
//   .eval/demo/diff-drift-v0.4-trailer-thumb.png
//
//   npm run demo:trailer

import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { chromium } from "@playwright/test";

const PORT = 51737;
const BASE_URL = `http://localhost:${PORT}`;
const VIEWPORT = { width: 1920, height: 1080 };
const OUT_DIR = join(process.cwd(), ".eval", "demo");
const WEBM_PATH = join(OUT_DIR, "diff-drift-v0.4-trailer.webm");
const MP4_PATH = join(OUT_DIR, "diff-drift-v0.4-trailer.mp4");
const THUMB_PATH = join(OUT_DIR, "diff-drift-v0.4-trailer-thumb.png");

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

async function waitForServer(url, timeoutMs = 30_000) {
  const deadline = Date.now() + timeoutMs;
  while (Date.now() < deadline) {
    try {
      const res = await fetch(url);
      if (res.ok) return;
    } catch {
      // Dev server is still starting.
    }
    await sleep(250);
  }
  throw new Error(`Dev server did not come up at ${url}`);
}

function esc(text) {
  return text
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;");
}

function codePanel(title, before, after) {
  return `
    <h3>${esc(title)}</h3>
    <div class="code-grid">
      <div><b>Before</b><pre>${esc(before)}</pre></div>
      <div><b>After</b><pre>${esc(after)}</pre></div>
    </div>`;
}

async function installOverlay(page) {
  await page.evaluate(() => {
    for (const id of ["trailer-style", "trailer-cursor", "trailer-caption", "trailer-card", "trailer-panel", "trailer-rail"]) {
      document.getElementById(id)?.remove();
    }

    const style = document.createElement("style");
    style.id = "trailer-style";
    style.textContent = `
      #trailer-cursor { position: fixed; z-index: 100000; width: 28px; height: 28px;
        pointer-events: none; left: 0; top: 0; transform: translate(-2px, -2px); }
      #trailer-caption { position: fixed; z-index: 99998; left: 50%; bottom: 38px;
        transform: translateX(-50%) translateY(10px); pointer-events: none;
        background: rgba(8, 10, 14, 0.93); color: #f7f8fb;
        font: 650 36px/1.25 "Segoe UI", system-ui, sans-serif;
        padding: 18px 34px; border-radius: 14px;
        border: 1px solid rgba(255, 255, 255, 0.14);
        box-shadow: 0 16px 44px rgba(0, 0, 0, 0.45);
        white-space: nowrap; opacity: 0;
        transition: opacity 220ms ease, transform 220ms ease; }
      #trailer-caption.show { opacity: 1; transform: translateX(-50%) translateY(0); }
      #trailer-card { position: fixed; z-index: 100001; inset: 0; pointer-events: none;
        display: flex; align-items: center; justify-content: center;
        background: radial-gradient(circle at 50% 30%, #1b2527 0, #0a0d12 48%, #07090c 100%);
        color: #f7f8fb; opacity: 0; transition: opacity 420ms ease; }
      #trailer-card.show { opacity: 1; }
      #trailer-card .inner { max-width: 1240px; text-align: center; padding: 0 70px; }
      #trailer-card h1 { font: 760 92px/1.02 "Segoe UI", system-ui, sans-serif; margin: 0 0 24px; }
      #trailer-card h2 { font: 720 52px/1.12 "Segoe UI", system-ui, sans-serif; margin: 0 0 18px; }
      #trailer-card p { font: 420 36px/1.34 "Segoe UI", system-ui, sans-serif; color: #d9e2e6; margin: 0; }
      #trailer-card .small { margin-top: 24px; color: #9ba8af; font-size: 26px; }
      #trailer-card pre { display: inline-block; text-align: left; margin: 24px 0 0; padding: 22px 30px;
        border-radius: 12px; border: 1px solid rgba(255, 255, 255, 0.14);
        background: #10151b; color: #eff4f7; font: 560 28px/1.45 "Cascadia Code", Consolas, monospace; }
      #trailer-panel { position: fixed; z-index: 99997; top: 96px; right: 48px;
        width: 760px; pointer-events: none; opacity: 0; transform: translateY(12px);
        transition: opacity 240ms ease, transform 240ms ease;
        background: rgba(7, 10, 14, 0.94); color: #eef5f7;
        border: 1px solid rgba(255, 255, 255, 0.14); border-radius: 14px;
        box-shadow: 0 20px 56px rgba(0, 0, 0, 0.48); padding: 22px; }
      #trailer-panel.show { opacity: 1; transform: translateY(0); }
      #trailer-panel h3 { margin: 0 0 16px; font: 700 26px/1.2 "Segoe UI", system-ui, sans-serif; }
      #trailer-panel .code-grid { display: grid; grid-template-columns: 1fr 1fr; gap: 14px; }
      #trailer-panel b { display: block; margin: 0 0 8px; color: #9fb2bb;
        font: 700 16px/1 "Segoe UI", system-ui, sans-serif; text-transform: uppercase; }
      #trailer-panel pre { margin: 0; min-height: 150px; padding: 14px;
        background: #111820; border: 1px solid rgba(255, 255, 255, 0.10);
        border-radius: 10px; white-space: pre-wrap;
        font: 520 18px/1.42 "Cascadia Code", Consolas, monospace; color: #e7edf0; }
      #trailer-rail { position: fixed; z-index: 99997; top: 28px; left: 50%;
        transform: translateX(-50%) translateY(-8px); pointer-events: none;
        display: flex; gap: 10px; opacity: 0; transition: opacity 220ms ease, transform 220ms ease; }
      #trailer-rail.show { opacity: 1; transform: translateX(-50%) translateY(0); }
      #trailer-rail span { padding: 9px 14px; border-radius: 999px;
        background: rgba(13, 18, 24, 0.92); border: 1px solid rgba(255, 255, 255, 0.13);
        color: #e7edf0; font: 750 19px/1 "Cascadia Code", Consolas, monospace; }
    `;
    document.head.appendChild(style);

    const cursor = document.createElement("div");
    cursor.id = "trailer-cursor";
    cursor.innerHTML = `<svg viewBox="0 0 24 24" width="28" height="28">
      <path d="M5 2 L5 19 L9.5 15.2 L12.3 21.5 L15 20.3 L12.2 14.2 L18 14 Z"
        fill="#fff" stroke="#15191f" stroke-width="1.4" stroke-linejoin="round"/></svg>`;
    document.body.appendChild(cursor);

    const caption = document.createElement("div");
    caption.id = "trailer-caption";
    document.body.appendChild(caption);

    const card = document.createElement("div");
    card.id = "trailer-card";
    card.innerHTML = `<div class="inner"></div>`;
    document.body.appendChild(card);

    const panel = document.createElement("div");
    panel.id = "trailer-panel";
    document.body.appendChild(panel);

    const rail = document.createElement("div");
    rail.id = "trailer-rail";
    document.body.appendChild(rail);

    window.__trailer = {
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
      panel(html) {
        panel.innerHTML = html ?? "";
        panel.classList.toggle("show", Boolean(html));
      },
      rail(labels) {
        rail.innerHTML = labels.map((label) => `<span>${label}</span>`).join("");
        rail.classList.toggle("show", labels.length > 0);
      },
    };
  });
}

function makeDriver(page) {
  let cursorAt = { x: VIEWPORT.width / 2, y: VIEWPORT.height - 68 };

  async function caption(text) {
    await page.evaluate((value) => window.__trailer.caption(value), text);
    await sleep(260);
  }

  async function card(html) {
    await page.evaluate((value) => window.__trailer.card(value), html);
    await sleep(460);
  }

  async function panel(html) {
    await page.evaluate((value) => window.__trailer.panel(value), html);
    await sleep(260);
  }

  async function rail(labels) {
    await page.evaluate((value) => window.__trailer.rail(value), labels);
    await sleep(220);
  }

  async function setCursor(x, y) {
    cursorAt = { x, y };
    await page.evaluate(({ x: nextX, y: nextY }) => window.__trailer.cursor(nextX, nextY), cursorAt);
  }

  async function moveAndClick(locator) {
    const box = await locator.boundingBox();
    if (!box) throw new Error(`No bounding box for ${locator}`);
    const to = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
    const from = cursorAt;
    for (let i = 1; i <= 30; i += 1) {
      const t = i / 30;
      const ease = t * t * (3 - 2 * t);
      await setCursor(from.x + (to.x - from.x) * ease, from.y + (to.y - from.y) * ease);
      await sleep(12);
    }
    await sleep(140);
    await page.mouse.click(to.x, to.y);
  }

  return { caption, card, panel, rail, setCursor, moveAndClick };
}

function findFfmpegUnder(dir, depth = 0) {
  if (depth > 4) return null;
  let entries;
  try {
    entries = readdirSync(dir, { withFileTypes: true });
  } catch {
    return null;
  }
  for (const entry of entries) {
    const candidate = join(dir, entry.name);
    if (entry.isFile() && entry.name.toLowerCase() === "ffmpeg.exe") return candidate;
    if (entry.isDirectory()) {
      const hit = findFfmpegUnder(candidate, depth + 1);
      if (hit) return hit;
    }
  }
  return null;
}

function findFfmpeg() {
  const winget = join(process.env.LOCALAPPDATA ?? "", "Microsoft", "WinGet");
  let gyanDirs = [];
  try {
    gyanDirs = readdirSync(join(winget, "Packages"))
      .filter((name) => name.startsWith("Gyan.FFmpeg"))
      .map((name) => join(winget, "Packages", name));
  } catch {
    // No winget install directory.
  }
  const candidates = [
    "ffmpeg",
    join(winget, "Links", "ffmpeg.exe"),
    ...gyanDirs.map((dir) => findFfmpegUnder(dir)).filter(Boolean),
  ];
  for (const candidate of candidates) {
    try {
      if (spawnSync(candidate, ["-version"], { stdio: "ignore" }).status === 0) return candidate;
    } catch {
      // Try the next candidate.
    }
  }
  return null;
}

async function run() {
  mkdirSync(OUT_DIR, { recursive: true });
  const ffmpeg = findFfmpeg();
  if (!ffmpeg) {
    throw new Error("System ffmpeg is required for the MP4 trailer and thumbnail. Install with: winget install -e --id Gyan.FFmpeg");
  }

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
    page.on("download", (download) => void download.cancel().catch(() => {}));
    const video = page.video();

    await page.goto(BASE_URL);
    await page.getByRole("button", { name: /Open a repository/ }).waitFor();
    await installOverlay(page);
    const d = makeDriver(page);

    await d.card(`
      <h1>Diff Drift v0.4</h1>
      <p>Structural drift review for the AI coding loop.</p>
      <p class="small">Local, deterministic, no model calls.</p>
    `);

    await page.getByRole("button", { name: /Open a repository/ }).click();
    await page.getByText("Risk Flags").waitFor();
    await sleep(900);
    await page.getByRole("button", { name: /auth\/validateToken\.ts, 2 flags/ }).click();
    await sleep(250);
    await installOverlay(page);
    await d.card(`
      <h1>Diff Drift v0.4</h1>
      <p>Structural drift review for the AI coding loop.</p>
      <p class="small">Local, deterministic, no model calls.</p>
    `);
    await sleep(3600);
    await d.card(null);
    await d.setCursor(VIEWPORT.width / 2, VIEWPORT.height - 68);

    await d.rail([".ts", ".tsx", ".js", ".rs", ".go", ".py", ".java", ".cs", ".kt", ".swift"]);
    await d.caption("v0.4 widens the drift map beyond TypeScript.");
    await sleep(4200);

    await d.caption("JS and TS security prompts still get the sharpest rules.");
    await d.moveAndClick(page.getByRole("button", { name: /High severity: Loose regex pattern/ }));
    await sleep(1200);
    await sleep(5200);

    await d.caption("New language families get structural review progress.");
    const panels = [
      codePanel("Rust handler drift", "input.trim().to_string()", "input.trim().to_ascii_lowercase()"),
      codePanel("Python secret drift", "def handler(payload):\n    return payload.strip()", "API_KEY = \"AKIA...\"\n\ndef handler(payload):\n    return payload.strip().lower()"),
      codePanel("Go service drift", "return strings.TrimSpace(input)", "return strings.ToLower(strings.TrimSpace(input))"),
      codePanel("C# / Kotlin / Swift", "Validate(token)\nreturn user", "Decode(token)\nreturn user"),
    ];
    for (const html of panels) {
      await d.panel(html);
      await sleep(2600);
    }
    await d.panel(null);

    await d.caption("Unsupported files are still counted instead of disappearing.");
    await sleep(4400);

    await d.caption("Pick the baseline that matches your trust point.");
    await d.moveAndClick(page.getByTestId("scope-trigger"));
    await page.getByRole("dialog", { name: "Analysis scope" }).waitFor();
    await sleep(5600);
    await page.keyboard.press("Escape");
    await sleep(500);

    await d.caption("Mark reviewed nodes as you actually inspect them.");
    await d.moveAndClick(page.getByRole("button", { name: /Mark reviewed: VariableDeclaration pattern/ }));
    await sleep(3200);

    await d.caption("Dismiss noise. Keep the audit trail.");
    await d.moveAndClick(page.getByRole("button", { name: "Dismiss flag: Permissive logging config" }));
    await sleep(3600);

    await d.caption("Pin the review when the drift is trusted.");
    await d.moveAndClick(page.getByRole("button", { name: "Mark reviewed", exact: true }));
    await page.getByText(/Reviewed at/).waitFor();
    await sleep(3400);

    await d.caption("Export the evidence for a PR, hook, or handoff.");
    await d.moveAndClick(page.getByRole("button", { name: "Export report" }));
    await page.getByRole("button", { name: /Exported/ }).waitFor();
    await sleep(3800);
    await d.caption("");
    await d.rail([]);

    await d.card(`
      <h2>Run it after the agent stops</h2>
      <pre>diff-drift-cli check . --baseline merge-base --md</pre>
      <p class="small">v0.4: TS/JS plus Rust, Go, Python, Java, C#, Kotlin, and Swift structural drift.</p>
    `);
    await sleep(7600);

    await context.close();
    await browser.close();
    videoSource = await video.path();
  } finally {
    stopVite();
  }

  copyFileSync(videoSource, WEBM_PATH);
  console.log(`Recorded ${WEBM_PATH}`);

  console.log(`Converting to MP4 with ${ffmpeg}...`);
  const convert = spawnSync(
    ffmpeg,
    ["-y", "-i", WEBM_PATH, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18", "-preset", "medium", "-r", "30", "-movflags", "+faststart", MP4_PATH],
    { stdio: ["ignore", "ignore", "inherit"] },
  );
  if (convert.status !== 0) throw new Error(`ffmpeg MP4 conversion exited ${convert.status}`);

  const thumb = spawnSync(
    ffmpeg,
    ["-y", "-ss", "00:00:08", "-i", MP4_PATH, "-vf", "scale=1280:720,crop=1280:640:0:40", "-frames:v", "1", "-update", "1", THUMB_PATH],
    { stdio: ["ignore", "ignore", "inherit"] },
  );
  if (thumb.status !== 0) throw new Error(`ffmpeg thumbnail extraction exited ${thumb.status}`);

  const mb = (statSync(MP4_PATH).size / 1024 / 1024).toFixed(1);
  console.log(`Wrote ${MP4_PATH} (${mb} MB)`);
  console.log(`Wrote ${THUMB_PATH}`);
}

run().catch((error) => {
  console.error(error);
  process.exit(1);
});
