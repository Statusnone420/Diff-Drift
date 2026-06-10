// Captures the README demo GIF without a human screen-recording session.
//
// Drives the browser UI (mock session — the same SessionData contract the Rust
// engine emits) through the six beats in docs/demo/demo-script.md, writes the
// beat keyframes to docs/assets/demo/, and encodes docs/assets/diff-drift-demo.gif.
//
//   npm run demo:capture
//
// Pure-JS pipeline: Playwright screenshots -> pngjs decode -> 2x box-filter
// downscale -> gifenc quantize/encode. No ffmpeg, no screen recorder.

import { spawn } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { chromium } from "@playwright/test";
import { PNG } from "pngjs";
import gifenc from "gifenc";

const { GIFEncoder, quantize, applyPalette } = gifenc;

const PORT = 51735;
const BASE_URL = `http://localhost:${PORT}`;
const VIEWPORT = { width: 1280, height: 800 };
const SCALE = 2; // capture at 2x, downscale to viewport size for a crisp GIF
const OUT_DIR = join(process.cwd(), "docs", "assets");
const FRAMES_DIR = join(OUT_DIR, "demo");
const GIF_PATH = join(OUT_DIR, "diff-drift-demo.gif");

const TWEEN_STEPS = 4;
const TWEEN_DELAY_MS = 70;

/** @type {{png: Buffer, delay: number}[]} */
const frames = [];
let keyframeCount = 0;

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

/** In-page overlay: a fake cursor (screenshots don't include the real one) and a lower-third caption bar. */
async function installOverlay(page) {
  await page.evaluate(() => {
    const style = document.createElement("style");
    style.textContent = `
      #demo-cursor { position: fixed; z-index: 100000; width: 22px; height: 22px;
        pointer-events: none; left: 0; top: 0; transform: translate(-2px, -2px); }
      #demo-caption { position: fixed; z-index: 99999; left: 50%; bottom: 22px;
        transform: translateX(-50%); pointer-events: none;
        background: rgba(10, 12, 16, 0.84); color: #f4f6f8;
        font: 500 16px/1.35 "Segoe UI", system-ui, sans-serif;
        letter-spacing: 0.01em; padding: 10px 18px; border-radius: 8px;
        white-space: nowrap; opacity: 0; transition: opacity 160ms ease; }
      #demo-caption.show { opacity: 1; }
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

    window.__demo = {
      cursor(x, y) {
        cursor.style.left = `${x}px`;
        cursor.style.top = `${y}px`;
      },
      caption(text) {
        caption.textContent = text;
        caption.classList.toggle("show", Boolean(text));
      },
    };
  });
}

function makeDriver(page) {
  let cursorAt = { x: VIEWPORT.width / 2, y: VIEWPORT.height - 60 };

  async function snap(delay) {
    const png = await page.screenshot({ type: "png" });
    frames.push({ png, delay });
    return png;
  }

  /** Hold a beat: capture one frame with a long display delay, and keep it as a keyframe. */
  async function hold(delay, name) {
    const png = await snap(delay);
    if (name) {
      keyframeCount += 1;
      const file = `${String(keyframeCount).padStart(2, "0")}-${name}.png`;
      writeFileSync(join(FRAMES_DIR, file), png);
    }
  }

  async function caption(text) {
    await page.evaluate((t) => window.__demo.caption(t), text);
    await sleep(220); // let the fade finish so frames don't catch it mid-transition
  }

  async function setCursor(x, y) {
    cursorAt = { x, y };
    await page.evaluate(({ x, y }) => window.__demo.cursor(x, y), cursorAt);
  }

  /** Tween the fake cursor to a locator's center so clicks read as human, then click. */
  async function moveAndClick(locator) {
    const box = await locator.boundingBox();
    if (!box) throw new Error(`No bounding box for ${locator}`);
    const to = { x: box.x + box.width / 2, y: box.y + box.height / 2 };
    const from = cursorAt;
    for (let i = 1; i <= TWEEN_STEPS; i += 1) {
      const t = i / TWEEN_STEPS;
      const ease = t * t * (3 - 2 * t); // smoothstep
      await setCursor(from.x + (to.x - from.x) * ease, from.y + (to.y - from.y) * ease);
      await snap(TWEEN_DELAY_MS);
    }
    await page.mouse.click(to.x, to.y);
  }

  return { snap, hold, caption, setCursor, moveAndClick };
}

/** Box-filter downscale of RGBA pixels by an integer factor. */
function downscale(data, width, height, factor) {
  const w = Math.floor(width / factor);
  const h = Math.floor(height / factor);
  const out = new Uint8ClampedArray(w * h * 4);
  const area = factor * factor;
  for (let y = 0; y < h; y += 1) {
    for (let x = 0; x < w; x += 1) {
      let r = 0, g = 0, b = 0, a = 0;
      for (let dy = 0; dy < factor; dy += 1) {
        for (let dx = 0; dx < factor; dx += 1) {
          const i = ((y * factor + dy) * width + (x * factor + dx)) * 4;
          r += data[i];
          g += data[i + 1];
          b += data[i + 2];
          a += data[i + 3];
        }
      }
      const o = (y * w + x) * 4;
      out[o] = r / area;
      out[o + 1] = g / area;
      out[o + 2] = b / area;
      out[o + 3] = a / area;
    }
  }
  return { data: out, width: w, height: h };
}

function encodeGif() {
  const gif = GIFEncoder();
  let size = null;
  for (const { png, delay } of frames) {
    const decoded = PNG.sync.read(png);
    const scaled = downscale(decoded.data, decoded.width, decoded.height, SCALE);
    size = { width: scaled.width, height: scaled.height };
    const palette = quantize(scaled.data, 256);
    const index = applyPalette(scaled.data, palette);
    gif.writeFrame(index, scaled.width, scaled.height, { palette, delay });
  }
  gif.finish();
  writeFileSync(GIF_PATH, gif.bytes());
  return size;
}

async function run() {
  mkdirSync(FRAMES_DIR, { recursive: true });

  console.log("Starting vite dev server...");
  const vite = spawn(process.execPath, ["node_modules/vite/bin/vite.js", "--port", String(PORT), "--strictPort"], {
    stdio: "ignore",
  });
  const stopVite = () => {
    if (!vite.killed) vite.kill();
  };
  process.on("exit", stopVite);

  try {
    await waitForServer(BASE_URL);

    const browser = await chromium.launch();
    const context = await browser.newContext({
      viewport: VIEWPORT,
      deviceScaleFactor: SCALE,
      acceptDownloads: true,
    });
    const page = await context.newPage();
    page.on("download", (d) => void d.cancel().catch(() => {}));

    await page.goto(BASE_URL);
    await page.getByRole("button", { name: /Open a repository/ }).waitFor();
    await installOverlay(page);
    const d = makeDriver(page);

    // Load the session (not part of the GIF; the story starts on the loaded view).
    await page.getByRole("button", { name: /Open a repository/ }).click();
    await page.getByText("Risk Flags").waitFor();
    await sleep(900); // initial flag selection scrolls smoothly; let it settle
    // Clear the auto-selected flag so beat 3's flag click visibly lights the node up.
    await page.getByRole("button", { name: /auth\/validateToken\.ts, 2 flags/ }).click();
    await sleep(200);
    await installOverlay(page); // React replaced the DOM on state change; reinstall
    await d.setCursor(VIEWPORT.width / 2, VIEWPORT.height - 60);

    // Beat 1 — the loaded drift.
    await d.caption("An AI agent changed payments code.");
    await d.hold(2600, "loaded-session");

    // Beat 2 — baseline picker.
    await d.caption("Compare against a baseline you trust.");
    await d.moveAndClick(page.getByTestId("scope-trigger"));
    await page.getByRole("dialog", { name: "Analysis scope" }).waitFor();
    await sleep(150);
    await d.hold(2400, "scope-picker");
    await page.keyboard.press("Escape");

    // Beat 3 — the risky node, before/after.
    await d.caption("Risky drift, node by node.");
    await d.moveAndClick(page.getByRole("button", { name: /High severity: Loose regex pattern/ }));
    await sleep(950); // smooth scroll + pulse
    await d.hold(2800, "risky-node");

    // Beat 4 — triage: review one node, dismiss one flag.
    await d.caption("Review what matters. Dismiss what doesn't.");
    await d.moveAndClick(page.getByRole("button", { name: /Mark reviewed: VariableDeclaration pattern/ }));
    await sleep(350);
    await d.hold(1300, "node-reviewed");
    await d.moveAndClick(page.getByRole("button", { name: "Dismiss flag: Permissive logging config" }));
    await sleep(400);
    await d.hold(1500, "flag-dismissed");

    // Beat 5 — mark the drift reviewed; the trust point unlocks the next baseline.
    await d.caption("Your review becomes the next baseline.");
    await d.moveAndClick(page.getByRole("button", { name: "Mark reviewed", exact: true }));
    await page.getByText(/Reviewed at/).waitFor();
    await sleep(250);
    await d.hold(1300, "drift-reviewed");
    await d.moveAndClick(page.getByTestId("scope-trigger"));
    await page.getByRole("dialog", { name: "Analysis scope" }).waitFor();
    await sleep(150);
    await d.hold(2000, "trust-point-unlocked");
    await d.moveAndClick(page.getByRole("button", { name: /Since last review/ }));
    await sleep(400);

    // Beat 6 — export evidence.
    await d.caption("Export evidence for the PR.");
    await d.moveAndClick(page.getByRole("button", { name: "Export report" }));
    await page.getByRole("button", { name: /Exported/ }).waitFor();
    await sleep(200);
    await d.hold(2800, "exported");

    await browser.close();
  } finally {
    stopVite();
  }

  console.log(`Captured ${frames.length} frames (${keyframeCount} keyframes in docs/assets/demo/).`);
  console.log("Encoding GIF...");
  const size = encodeGif();
  const totalMs = frames.reduce((ms, f) => ms + f.delay, 0);
  const { statSync } = await import("node:fs");
  const mb = (statSync(GIF_PATH).size / 1024 / 1024).toFixed(1);
  console.log(`Wrote ${GIF_PATH} — ${size.width}x${size.height}, ${(totalMs / 1000).toFixed(1)}s, ${mb} MB.`);
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
