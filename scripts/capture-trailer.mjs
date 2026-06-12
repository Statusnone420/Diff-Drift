// Captures the v0.4 product trailer MP4 and social thumbnail.
//
// This is a release/artifact helper, not product runtime code. It drives the
// browser mock session with Playwright, but instead of a static screen capture
// with lower-third captions it stages a real trailer: full-bleed kinetic
// typography cards, animated counters, language chips that pop in on the beat,
// and camera-style push-ins (CSS transforms) over the live app. Playwright
// records one continuous WebM at deviceScaleFactor 2 for a crisp UI; system
// ffmpeg encodes H.264 yuv420p MP4 + a 1280x640 social thumbnail.
//
//   npm run demo:trailer
//
// Output (both gitignored, attach to the release / embed in README):
//   .eval/demo/diff-drift-v0.4-trailer.mp4
//   .eval/demo/diff-drift-v0.4-trailer-thumb.png
//
// Pacing: everything lands on a 2s grid (~120 BPM) so the owner can lay
// royalty-free music over it in Clipchamp and the cuts sit on the beat. The
// trailer carries energy silent — hard cuts between cards, no audio track.

import { spawn, spawnSync } from "node:child_process";
import { copyFileSync, mkdirSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { chromium } from "@playwright/test";

const PORT = 51738;
const BASE_URL = `http://localhost:${PORT}`;
const VIEWPORT = { width: 1920, height: 1080 };
const SCALE = 2; // deviceScaleFactor: render the UI at 2x for sharp text, record at viewport size
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

// ---------------------------------------------------------------------------
// In-page trailer stage: a full-screen card layer (kinetic type / counters /
// chips / terminal / end card) plus a "camera" that transforms the live app
// (#root) for push-ins and Ken Burns. Idempotent so it survives React DOM swaps.
// ---------------------------------------------------------------------------
async function installStage(page) {
  await page.evaluate(() => {
    for (const id of ["dd-stage-style", "dd-stage", "dd-vignette"]) {
      document.getElementById(id)?.remove();
    }

    const style = document.createElement("style");
    style.id = "dd-stage-style";
    style.textContent = `
      :root { --dd-amber: #e7a83e; --dd-green: #4ec46a; --dd-red: #f2604c; --dd-blue: #6f8bc4; }

      /* Camera: wraps the whole app so we can push in / pan over the live UI. */
      #root { will-change: transform, filter; transform-origin: 50% 50%;
        transition: transform 1100ms cubic-bezier(.16,.84,.27,1), filter 700ms ease; }
      #root.dd-dim { filter: brightness(0.32) saturate(0.85); }

      /* Vignette + faint grain for depth over the app shots. */
      #dd-vignette { position: fixed; inset: 0; z-index: 99990; pointer-events: none; opacity: 0;
        transition: opacity 500ms ease;
        background: radial-gradient(ellipse 120% 90% at 50% 45%, transparent 52%, rgba(0,0,0,0.55) 100%); }
      #dd-vignette.show { opacity: 1; }

      /* Full-bleed card layer. */
      #dd-stage { position: fixed; inset: 0; z-index: 99999; pointer-events: none;
        display: flex; align-items: center; justify-content: center;
        font-family: "Segoe UI Variable Text","Segoe UI",system-ui,sans-serif;
        opacity: 0; }
      #dd-stage.show { opacity: 1; }
      #dd-stage .bg { position: absolute; inset: 0; }
      #dd-stage .wrap { position: relative; width: 1640px; max-width: 92vw; padding: 0 40px;
        text-align: center; }

      /* Kinetic word: each word slides+rises in, slight blur-to-sharp. */
      .dd-kw { display: inline-block; opacity: 0; transform: translateY(38px) skewY(4deg);
        filter: blur(6px); }
      .dd-kw.in { animation: ddWordIn 460ms cubic-bezier(.18,.9,.28,1) forwards; }
      @keyframes ddWordIn {
        from { opacity: 0; transform: translateY(38px) skewY(4deg); filter: blur(6px); }
        to   { opacity: 1; transform: translateY(0) skewY(0); filter: blur(0); } }

      .dd-head { font-weight: 800; line-height: 1.02; letter-spacing: -0.01em;
        color: #f4f6fa; margin: 0; text-wrap: balance; }
      .dd-h-xl { font-size: 116px; }
      .dd-h-lg { font-size: 88px; }
      .dd-h-md { font-size: 68px; }
      .dd-sub { font-weight: 400; font-size: 40px; line-height: 1.32; color: #c7cdd8;
        margin: 30px auto 0; max-width: 1180px; text-wrap: balance; }
      .dd-eyebrow { font-weight: 700; font-size: 26px; letter-spacing: 0.26em;
        text-transform: uppercase; color: var(--dd-amber); margin: 0 0 28px; }
      .dd-amber { color: var(--dd-amber); }
      .dd-green { color: var(--dd-green); }
      .dd-red { color: var(--dd-red); }

      /* Animated counter row. */
      .dd-stats { display: flex; gap: 96px; justify-content: center; margin: 18px 0 0; }
      .dd-stat { text-align: center; }
      .dd-stat .num { font-weight: 800; font-size: 132px; line-height: 1; letter-spacing: -0.02em;
        font-variant-numeric: tabular-nums; }
      .dd-stat .lbl { font-weight: 600; font-size: 30px; color: #aeb6c2; margin-top: 14px;
        letter-spacing: 0.02em; }

      /* Language chips. */
      .dd-langs { display: flex; flex-wrap: wrap; gap: 22px; justify-content: center;
        margin: 44px auto 0; max-width: 1500px; }
      .dd-chip { opacity: 0; transform: translateY(26px) scale(.82);
        font-family: "Cascadia Code","Cascadia Mono",ui-monospace,Consolas,monospace;
        font-weight: 700; font-size: 52px; color: #eef2f8;
        padding: 18px 34px; border-radius: 16px; background: rgba(20,23,30,0.9);
        border: 1px solid rgba(255,255,255,0.10);
        box-shadow: 0 18px 50px -22px rgba(0,0,0,0.8); }
      .dd-chip.in { animation: ddChipIn 380ms cubic-bezier(.2,1.3,.4,1) forwards; }
      .dd-chip .dot { color: var(--dd-amber); }
      @keyframes ddChipIn {
        from { opacity: 0; transform: translateY(26px) scale(.82); }
        to   { opacity: 1; transform: translateY(0) scale(1); } }

      /* Pulsing severity badge for the problem beat. */
      .dd-badge { display: inline-flex; align-items: center; gap: 16px; margin: 0 0 30px;
        font-weight: 700; font-size: 30px; letter-spacing: 0.04em; padding: 14px 28px;
        border-radius: 999px; color: var(--dd-red); background: rgba(242,96,76,0.12);
        border: 1px solid rgba(242,96,76,0.4); }
      .dd-badge .pip { width: 16px; height: 16px; border-radius: 50%; background: var(--dd-red);
        animation: ddPulse 1.1s ease-in-out infinite; }
      @keyframes ddPulse {
        0%,100% { box-shadow: 0 0 0 0 rgba(242,96,76,0.55); }
        50%     { box-shadow: 0 0 0 14px rgba(242,96,76,0); } }

      /* Terminal card. */
      .dd-term { margin: 0 auto; width: 1200px; max-width: 90vw; text-align: left;
        border-radius: 18px; overflow: hidden; border: 1px solid rgba(255,255,255,0.10);
        box-shadow: 0 40px 120px -40px rgba(0,0,0,0.9); background: #0c0e13; }
      .dd-term .bar { display: flex; align-items: center; gap: 10px; padding: 18px 24px;
        background: #14171e; border-bottom: 1px solid rgba(255,255,255,0.07); }
      .dd-term .bar i { width: 14px; height: 14px; border-radius: 50%; display: inline-block; }
      .dd-term .bar .t { margin-left: 14px; font-size: 22px; color: #8b93a2;
        font-family: "Cascadia Code",ui-monospace,Consolas,monospace; }
      .dd-term .body { padding: 36px 40px; display: flex; flex-direction: column; gap: 20px;
        font-family: "Cascadia Code","Cascadia Mono",ui-monospace,Consolas,monospace;
        font-size: 30px; line-height: 1.2; color: #e7edf4; }
      .dd-term .body .ln { display: block; white-space: nowrap; }
      .dd-term .body .pr { color: var(--dd-green); margin-right: 16px; }
      .dd-term .body .fl { color: #8b93a2; }
      .dd-term .body .ok { color: var(--dd-green); margin-right: 8px; }
      .dd-term .body .new { color: var(--dd-red); margin-right: 8px; }

      /* End card brand lockup. */
      .dd-brand { display: flex; align-items: center; justify-content: center; gap: 26px; }
      .dd-brand svg { width: 92px; height: 92px; }
      .dd-brand .name { font-weight: 800; font-size: 96px; letter-spacing: -0.01em; color: #f4f6fa; }
      .dd-url { font-family: "Cascadia Code",ui-monospace,Consolas,monospace; font-weight: 600;
        font-size: 34px; color: #c7cdd8; margin-top: 40px; }
      .dd-url .at { color: var(--dd-amber); }
    `;
    document.head.appendChild(style);

    const vignette = document.createElement("div");
    vignette.id = "dd-vignette";
    document.body.appendChild(vignette);

    const stage = document.createElement("div");
    stage.id = "dd-stage";
    stage.innerHTML = `<div class="bg"></div><div class="wrap"></div>`;
    document.body.appendChild(stage);

    const root = document.getElementById("root");

    // Severity-tinted radial backgrounds for cards.
    const BG = {
      dark: "radial-gradient(ellipse 90% 80% at 50% 35%, #15171d 0%, #0a0b0f 55%, #07080b 100%)",
      red: "radial-gradient(ellipse 90% 80% at 50% 38%, #1f1413 0%, #0c0a0c 55%, #08070a 100%)",
      amber: "radial-gradient(ellipse 90% 80% at 50% 38%, #1d1810 0%, #0c0a0a 55%, #08070a 100%)",
      green: "radial-gradient(ellipse 90% 80% at 50% 38%, #101a13 0%, #090c0a 55%, #07090a 100%)",
    };

    window.__dd = {
      // Show a card with raw inner HTML. tint picks the radial background.
      card(html, tint = "dark") {
        const wrap = stage.querySelector(".wrap");
        stage.querySelector(".bg").style.background = BG[tint] || BG.dark;
        wrap.innerHTML = html || "";
        stage.classList.toggle("show", Boolean(html));
      },
      hideCard() {
        stage.classList.remove("show");
      },
      // Animate kinetic words in sequence (per-word slide-in).
      playWords(stepMs = 150) {
        const words = stage.querySelectorAll(".dd-kw");
        words.forEach((w, i) => setTimeout(() => w.classList.add("in"), i * stepMs));
      },
      // Pop language chips in one at a time.
      playChips(stepMs = 150) {
        const chips = stage.querySelectorAll(".dd-chip");
        chips.forEach((c, i) => setTimeout(() => c.classList.add("in"), i * stepMs));
      },
      // Count a number up over durMs into the element matching sel.
      countUp(sel, to, durMs = 1100) {
        const el = stage.querySelector(sel);
        if (!el) return;
        const start = performance.now();
        const tick = (now) => {
          const t = Math.min(1, (now - start) / durMs);
          const eased = 1 - Math.pow(1 - t, 3);
          el.textContent = String(Math.round(to * eased));
          if (t < 1) requestAnimationFrame(tick);
        };
        requestAnimationFrame(tick);
      },
      // Camera transform over the live app.
      camera(transform, ms = 1100) {
        root.style.transitionDuration = `${ms}ms, 700ms`;
        root.style.transform = transform || "none";
      },
      cameraReset() {
        root.style.transform = "none";
      },
      dim(on) {
        root.classList.toggle("dd-dim", Boolean(on));
      },
      vignette(on) {
        vignette.classList.toggle("show", Boolean(on));
      },
    };
  });
}

// Build kinetic-type markup: wraps each word so playWords can stagger them.
function kinetic(text) {
  return text
    .split(/(\s+)/)
    .map((tok) => (tok.trim() ? `<span class="dd-kw">${tok}</span>` : tok))
    .join("");
}

function makeDriver(page) {
  const card = (html, tint) => page.evaluate(({ html, tint }) => window.__dd.card(html, tint), { html, tint });
  const hideCard = () => page.evaluate(() => window.__dd.hideCard());
  const playWords = (ms) => page.evaluate((ms) => window.__dd.playWords(ms), ms);
  const playChips = (ms) => page.evaluate((ms) => window.__dd.playChips(ms), ms);
  const countUp = (sel, to, ms) => page.evaluate(({ sel, to, ms }) => window.__dd.countUp(sel, to, ms), { sel, to, ms });
  const camera = (t, ms) => page.evaluate(({ t, ms }) => window.__dd.camera(t, ms), { t, ms });
  const cameraReset = () => page.evaluate(() => window.__dd.cameraReset());
  const dim = (on) => page.evaluate((on) => window.__dd.dim(on), on);
  const vignette = (on) => page.evaluate((on) => window.__dd.vignette(on), on);
  return { card, hideCard, playWords, playChips, countUp, camera, cameraReset, dim, vignette };
}

const BRAND_SVG = `<svg viewBox="0 0 16 16" fill="none">
  <path d="M2 11.5C4 11.5 4 4.5 8 4.5s4 7 6 7" stroke="#e7a83e" stroke-width="1.5" stroke-linecap="round" fill="none"/>
  <circle cx="8" cy="4.5" r="1.5" fill="#e7a83e"/></svg>`;

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

// ---------------------------------------------------------------------------
// Storyboard. Everything is timed on a 2s grid (~120 BPM). beat(n) waits n*2s.
// ---------------------------------------------------------------------------
async function storyboard(page) {
  const d = makeDriver(page);
  const BEAT = 2000;
  const beat = (n = 1) => sleep(BEAT * n);

  // Load the mock session behind a dark cold-open card so there is never a
  // white flash. recordVideo starts at context creation; we open on the card.
  await d.card(`<h1 class="dd-head dd-h-lg">&nbsp;</h1>`, "dark"); // dark hold
  await page.getByRole("button", { name: /Open a repository/ }).click();
  await page.getByText("Risk Flags").waitFor();
  await sleep(700);
  // Clear the auto-selected flag so later flag clicks visibly light a node.
  await page.getByRole("button", { name: /auth\/validateToken\.ts, 2 flags/ }).click();
  await sleep(250);
  await installStage(page); // React swapped the DOM on load — reinstall the stage
  await sleep(600); // let fonts settle under the card

  // ---- 0–6s COLD OPEN: kinetic line, hard cut to the stakes. ----
  await d.card(
    `<p class="dd-eyebrow">Diff Drift</p>
     <h1 class="dd-head dd-h-xl">${kinetic("An AI agent just rewrote your repo.")}</h1>`,
    "dark",
  );
  await d.playWords(150);
  await beat(2); // 4s

  await d.card(
    `<h1 class="dd-head dd-h-lg">${kinetic("git diff shows you")} <span class="dd-amber">${kinetic("everything.")}</span></h1>
     <p class="dd-sub">${kinetic("Every line. Every file. All at once.")}</p>`,
    "dark",
  );
  await d.playWords(120);
  await beat(1); // 6s

  // ---- 6–14s PUSH-IN ON THE LIVE DRIFT. Hide card, camera over the app. ----
  await d.hideCard();
  await d.vignette(true);
  // Push in on the center diff column — keep the before/after lines in frame.
  await d.camera("scale(1.45) translate(6%, -3%)", 1200);
  await beat(1.5); // ~9s — watch the before/after diff fill the frame

  // Slow drift down the diff (Ken Burns) while staying readable.
  await d.camera("scale(1.5) translate(5%, -14%)", 1500);
  await beat(1.5); // ~12s

  // Pull back to a gentle hero of the full shell for one breath.
  await d.camera("scale(1.08)", 1100);
  await beat(1); // ~14s

  // ---- 14–20s THE PROBLEM, stated plainly. ----
  await d.dim(true);
  await d.card(
    `<div class="dd-badge"><span class="pip"></span>High severity</div>
     <h1 class="dd-head dd-h-md">${kinetic("You don't need everything.")}</h1>
     <p class="dd-sub">${kinetic("You need what changed since you last trusted it.")}</p>`,
    "red",
  );
  await d.playWords(130);
  await beat(3); // 20s
  await d.dim(false);
  await d.cameraReset();
  await d.vignette(false);

  // ---- 20–34s v0.4 HERO: counters, then language chips slam in. ----
  await d.card(
    `<p class="dd-eyebrow">v0.4</p>
     <h1 class="dd-head dd-h-lg">${kinetic("Structural drift review,")}<br>${kinetic("now across the stack.")}</h1>
     <div class="dd-stats">
       <div class="dd-stat"><div class="num dd-amber" id="dd-c1">0</div><div class="lbl">languages</div></div>
       <div class="dd-stat"><div class="num dd-green" id="dd-c2">0</div><div class="lbl">model calls</div></div>
     </div>`,
    "dark",
  );
  await d.playWords(110);
  await sleep(400);
  await d.countUp("#dd-c1", 11, 1300);
  await d.countUp("#dd-c2", 0, 200); // stays 0 — the honest stat
  await beat(2); // 24s

  await d.card(
    `<h1 class="dd-head dd-h-md">${kinetic("Beyond TypeScript.")}</h1>
     <div class="dd-langs">
       <span class="dd-chip"><span class="dot">.</span>ts</span>
       <span class="dd-chip"><span class="dot">.</span>js</span>
       <span class="dd-chip"><span class="dot">.</span>rs</span>
       <span class="dd-chip"><span class="dot">.</span>go</span>
       <span class="dd-chip"><span class="dot">.</span>py</span>
       <span class="dd-chip"><span class="dot">.</span>java</span>
       <span class="dd-chip"><span class="dot">.</span>cs</span>
       <span class="dd-chip"><span class="dot">.</span>kt</span>
       <span class="dd-chip"><span class="dot">.</span>swift</span>
     </div>`,
    "amber",
  );
  await d.playWords(110);
  await sleep(250);
  await d.playChips(150); // 9 chips * 150ms ≈ 1.35s — lands inside the beat
  await beat(2.5); // 29s

  await d.card(
    `<h1 class="dd-head dd-h-lg dd-amber" style="font-family:'Cascadia Code',ui-monospace,Consolas,monospace;font-size:74px;letter-spacing:0">
       Rust &middot; Go &middot; Python &middot; Java<br>C# &middot; Kotlin &middot; Swift
     </h1>
     <p class="dd-sub">${kinetic("Same structural drift review. Same local engine.")}</p>`,
    "dark",
  );
  await page.evaluate(() => window.__dd.playWords(120));
  await beat(2.5); // 34s

  // ---- 34–46s THE PRODUCT'S SOUL: trust points. ----
  await d.card(
    `<h1 class="dd-head dd-h-lg">${kinetic("It remembers what")}<br>${kinetic("you already reviewed.")}</h1>`,
    "dark",
  );
  await d.playWords(120);
  await beat(2); // 38s

  // Show the live triage: mark a node reviewed under a push-in.
  await d.hideCard();
  await d.vignette(true);
  await d.camera("scale(1.4) translate(8%, 4%)", 1100);
  await sleep(900);
  await page
    .getByRole("button", { name: /Mark reviewed: VariableDeclaration pattern/ })
    .click({ timeout: 5000 })
    .catch(() => {});
  await beat(1); // ~41s
  await page
    .getByRole("button", { name: "Dismiss flag: Permissive logging config" })
    .click({ timeout: 5000 })
    .catch(() => {});
  await beat(1); // ~43s
  await d.cameraReset();
  await d.vignette(false);

  await d.card(
    `<h1 class="dd-head dd-h-md">${kinetic("Pin the trust point.")}</h1>
     <p class="dd-sub">${kinetic("Drift only gets loud where it's")} <span class="dd-amber">${kinetic("new.")}</span></p>`,
    "green",
  );
  await d.playWords(130);
  await beat(2); // ~47s

  // ---- 47–58s CLI + ACTION terminal beat. ----
  await d.card(
    `<p class="dd-eyebrow">Run it after the agent stops</p>
     <div class="dd-term">
       <div class="bar"><i style="background:#f2604c"></i><i style="background:#e7a83e"></i><i style="background:#4ec46a"></i>
         <span class="t">diff-drift</span></div>
       <div class="body"><span class="ln"><span class="pr">$</span>diff-drift-cli check . --baseline merge-base --md</span><span class="ln"><span class="ok">&check;</span>reviewed drift unchanged</span><span class="ln"><span class="new">&#9650;</span>1 new high-severity flag &mdash; exit 1</span><span class="ln fl">// fails the check and prints the report</span></div>
     </div>`,
    "dark",
  );
  await beat(3); // ~53s

  await d.card(
    `<h1 class="dd-head dd-h-md">${kinetic("Local. Deterministic.")}</h1>
     <p class="dd-sub">${kinetic("No model calls. Your code never leaves the machine.")}</p>`,
    "dark",
  );
  await d.playWords(130);
  await beat(2); // ~57s

  // ---- 57–68s END CARD. ----
  await d.card(
    `<div class="dd-brand">${BRAND_SVG}<span class="name">Diff Drift</span></div>
     <p class="dd-sub" style="margin-top:34px">Review what changed since you last trusted it.</p>
     <p class="dd-url"><span class="at">github.com/</span>Statusnone420/Diff-Drift</p>`,
    "dark",
  );
  await beat(3); // ~63s
  await sleep(800); // brief hold; ffmpeg trims the recording's pre-roll head
}

async function run() {
  mkdirSync(OUT_DIR, { recursive: true });
  const ffmpeg = findFfmpeg();
  if (!ffmpeg) {
    throw new Error(
      "System ffmpeg is required for the MP4 trailer and thumbnail. Install with: winget install -e --id Gyan.FFmpeg",
    );
  }

  console.log("Starting vite dev server...");
  const vite = spawn(
    process.execPath,
    ["node_modules/vite/bin/vite.js", "--port", String(PORT), "--strictPort"],
    { stdio: "ignore" },
  );
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
      deviceScaleFactor: SCALE, // crisp UI; Playwright still records at viewport size (1920x1080)
      recordVideo: { dir: OUT_DIR, size: VIEWPORT },
      acceptDownloads: true,
    });
    const page = await context.newPage();
    page.on("download", (download) => void download.cancel().catch(() => {}));
    const video = page.video();

    await page.goto(BASE_URL);
    await page.getByRole("button", { name: /Open a repository/ }).waitFor();
    await installStage(page);

    await storyboard(page);

    await context.close(); // flushes the recording
    await browser.close();
    videoSource = await video.path();
  } finally {
    stopVite();
  }

  copyFileSync(videoSource, WEBM_PATH);
  console.log(`Recorded ${WEBM_PATH}`);

  console.log(`Converting to MP4 with ${ffmpeg}...`);
  // Trim the recording's pre-roll: recordVideo starts at context creation, so
  // there is ~1.3s of black/load before the cold-open card. -ss before -i seeks
  // there so the MP4 opens directly on the first kinetic line.
  const convert = spawnSync(
    ffmpeg,
    [
      "-y",
      "-ss", "1.3",
      "-i", WEBM_PATH,
      "-c:v", "libx264",
      "-pix_fmt", "yuv420p",
      "-crf", "20",
      "-preset", "medium",
      "-r", "30",
      "-an", // no audio track — owner adds music in Clipchamp
      "-movflags", "+faststart",
      MP4_PATH,
    ],
    { stdio: ["ignore", "ignore", "inherit"] },
  );
  if (convert.status !== 0) throw new Error(`ffmpeg MP4 conversion exited ${convert.status}`);

  // Social thumbnail: pull a frame from the v0.4 counter hero and crop 1280x640.
  const thumb = spawnSync(
    ffmpeg,
    [
      "-y",
      "-ss", "00:00:23",
      "-i", MP4_PATH,
      "-vf", "scale=1280:720,crop=1280:640:0:40",
      "-frames:v", "1",
      "-update", "1",
      THUMB_PATH,
    ],
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
