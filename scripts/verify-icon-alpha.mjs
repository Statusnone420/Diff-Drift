// Verify that all regenerated icon PNGs have transparent corners.
// Run after fix-icon-alpha.mjs + tauri icon regeneration.

import { createRequire } from 'module';
import { fileURLToPath } from 'url';
import { readFileSync } from 'fs';
import { join, dirname } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const require = createRequire(import.meta.url);
const { PNG } = require('pngjs');

const ICON_DIR = join(__dirname, '..', 'src-tauri', 'icons');

// All PNG icons that should have transparent corners
// (excluding .ico/.icns and the raw master/source files)
const TARGET_PNGS = [
  '32x32.png', '64x64.png', '128x128.png', '128x128@2x.png', 'icon.png',
  'StoreLogo.png',
  'Square30x30Logo.png', 'Square44x44Logo.png', 'Square71x71Logo.png',
  'Square89x89Logo.png', 'Square107x107Logo.png', 'Square142x142Logo.png',
  'Square150x150Logo.png', 'Square284x284Logo.png', 'Square310x310Logo.png',
];

let totalPass = 0, totalFail = 0;

for (const f of TARGET_PNGS) {
  const fpath = join(ICON_DIR, f);
  let buf;
  try {
    buf = readFileSync(fpath);
  } catch (e) {
    console.log(f + ': SKIP (not found)');
    continue;
  }
  const png = PNG.sync.read(buf);
  const W = png.width, H = png.height;

  // Check a few corner pixels
  const checks = [
    [0, 0], [1, 0], [0, 1],
    [W-1, 0], [0, H-1], [W-1, H-1],
  ];
  let fileFail = false;
  const details = [];
  // Allow alpha ≤ 5 for very small icons where downsampling antialiasing
  // leaves residual sub-5 values that are visually transparent.
  const ALPHA_THRESHOLD = W <= 32 ? 5 : 0;
  for (const [x, y] of checks) {
    const i = (y * W + x) * 4;
    const a = png.data[i + 3];
    if (a > ALPHA_THRESHOLD) {
      fileFail = true;
      details.push('(' + x + ',' + y + ')=alpha:' + a);
    }
  }

  // Also check center is opaque
  const ci = (Math.floor(H/2) * W + Math.floor(W/2)) * 4;
  const ca = png.data[ci + 3];
  if (ca < 200) {
    fileFail = true;
    details.push('center=alpha:' + ca);
  }

  if (fileFail) {
    console.log(f + ' (' + W + 'x' + H + '): FAIL ' + details.join(', '));
    totalFail++;
  } else {
    console.log(f + ' (' + W + 'x' + H + '): PASS');
    totalPass++;
  }
}

console.log('\n' + totalPass + ' PASS, ' + totalFail + ' FAIL');
if (totalFail > 0) process.exit(1);
