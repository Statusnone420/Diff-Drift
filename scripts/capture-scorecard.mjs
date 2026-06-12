// Renders a blind-agent scorecard HTML dashboard to a PNG for the README.
//
//   npm run scorecard:capture [-- <scorecard.html> <out.png>]
//
// Defaults to the published v4 benchmark scorecard and the README image path.
// Pure Playwright: load the file, screenshot the <main> dashboard at 2x. No
// screen recorder, no manual capture — the PNG is reproducible from the HTML
// that `npm run eval:score-agent` emits.

import { pathToFileURL } from "node:url";
import { resolve } from "node:path";
import { chromium } from "@playwright/test";

const htmlArg = process.argv[2] ?? "eval/benchmarks/v4/panel/panel-scorecard.html";
const outArg = process.argv[3] ?? "docs/assets/diff-drift-blind-agent-scorecard.png";

const htmlPath = resolve(process.cwd(), htmlArg);
const outPath = resolve(process.cwd(), outArg);

const browser = await chromium.launch();
try {
  const page = await browser.newPage({
    viewport: { width: 1856, height: 1200 },
    deviceScaleFactor: 2, // crisp text when the README displays it at width=920
  });
  await page.goto(pathToFileURL(htmlPath).href, { waitUntil: "networkidle" });

  // Make the page background transparent so the <main> border-radius produces
  // transparent corners rather than white-filled ones in the PNG.
  await page.addStyleTag({ content: "html, body { background: transparent !important; }" });

  // Frame the centered dashboard container rather than the full viewport so the
  // PNG has no dead margin regardless of how tall the case table grows.
  const main = page.locator("main");
  await main.waitFor({ state: "visible" });
  await main.screenshot({ path: outPath, omitBackground: true });

  console.log(`SCORECARD ${htmlArg} -> ${outArg}`);
} finally {
  await browser.close();
}
