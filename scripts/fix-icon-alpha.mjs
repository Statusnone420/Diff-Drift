// Fix opaque corners on the Diff Drift icon set.
//
// The icon-source.png has a fully-opaque background. This script:
//   1. Reads icon-source.png
//   2. Applies a smooth rounded-rect alpha mask (squircle, r ≈ 22.5% of size)
//   3. Writes the masked version to src-tauri/icons/icon-source.png in-place
//      and a copy to the Tauri icon input path (icon-master-transparent.png)
//
// After this script, run:
//   npm run tauri -- icon src-tauri/icons/icon-master-transparent.png --output src-tauri/icons
//
// Then verify with scripts/verify-icon-alpha.mjs.

import { createRequire } from 'module';
import { fileURLToPath } from 'url';
import { readFileSync, writeFileSync } from 'fs';
import { join, dirname } from 'path';

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);
const require = createRequire(import.meta.url);
const { PNG } = require('pngjs');

const ICON_DIR = join(__dirname, '..', 'src-tauri', 'icons');

/**
 * Apply a rounded-rect alpha mask to a PNG.
 * Pixels outside the rounded rect get alpha=0.
 * Pixels inside keep their original color + alpha.
 * Pixels on the antialiased boundary get proportional alpha.
 *
 * @param {Buffer} inputBuf - raw PNG buffer (must have alpha channel)
 * @param {number} radiusFraction - corner radius as fraction of min(width, height)
 * @returns {Buffer} output PNG buffer with transparent corners
 */
function applyRoundedCornerMask(inputBuf, radiusFraction = 0.225) {
  const src = PNG.sync.read(inputBuf);
  const W = src.width;
  const H = src.height;
  const r = Math.floor(Math.min(W, H) * radiusFraction);

  const dst = new PNG({ width: W, height: H, colorType: 6, inputColorType: 6 });
  // Copy all data first
  src.data.copy(dst.data);

  for (let y = 0; y < H; y++) {
    for (let x = 0; x < W; x++) {
      // Distance from each corner center to this pixel
      // Corner centers are at (r, r), (W-1-r, r), (r, H-1-r), (W-1-r, H-1-r)
      let mask = 1.0;

      if (x < r && y < r) {
        // TL corner
        const dx = r - x, dy = r - y;
        mask = roundedCornerAlpha(dx, dy, r);
      } else if (x > W - 1 - r && y < r) {
        // TR corner
        const dx = x - (W - 1 - r), dy = r - y;
        mask = roundedCornerAlpha(dx, dy, r);
      } else if (x < r && y > H - 1 - r) {
        // BL corner
        const dx = r - x, dy = y - (H - 1 - r);
        mask = roundedCornerAlpha(dx, dy, r);
      } else if (x > W - 1 - r && y > H - 1 - r) {
        // BR corner
        const dx = x - (W - 1 - r), dy = y - (H - 1 - r);
        mask = roundedCornerAlpha(dx, dy, r);
      }

      if (mask < 1.0) {
        const idx = (y * W + x) * 4;
        dst.data[idx + 3] = Math.round(dst.data[idx + 3] * mask);
      }
    }
  }

  return PNG.sync.write(dst);
}

/**
 * Compute smooth alpha for a pixel at (dx, dy) from a corner arc center.
 * dx, dy > 0 means "into the corner" (outside the safe zone).
 * Uses antialiased boundary: 1.0 inside, 0.0 outside, linear transition across 1.5px.
 */
function roundedCornerAlpha(dx, dy, r) {
  // Distance from corner arc center: (dx, dy) = distance from arc center
  const dist = Math.sqrt(dx * dx + dy * dy);
  // The arc boundary is at distance r from the corner center
  // dist < r => inside => alpha 1.0
  // dist > r+1.5 => outside => alpha 0.0
  // linear in between
  if (dist <= r - 0.5) return 1.0;
  if (dist >= r + 1.0) return 0.0;
  return (r + 1.0 - dist) / 1.5;
}

// Process icon-source.png
const srcPath = join(ICON_DIR, 'icon-source.png');
const srcBuf = readFileSync(srcPath);

console.log('Reading icon-source.png...');
const check = PNG.sync.read(srcBuf);
console.log('  Size: ' + check.width + 'x' + check.height);

// Apply rounded-rect mask
console.log('Applying rounded-rect alpha mask (r=22.5%)...');
const maskedBuf = applyRoundedCornerMask(srcBuf, 0.225);

// Overwrite icon-source.png with the transparent-corner version
writeFileSync(srcPath, maskedBuf);
console.log('Wrote: ' + srcPath);

// Verify corners on the output
const verify = PNG.sync.read(maskedBuf);
const W = verify.width, H = verify.height;
const corners = [
  ['TL(0,0)', 0, 0], ['TL(1,0)', 1, 0], ['TL(0,1)', 0, 1], ['TL(2,2)', 2, 2],
  ['TR(W-1,0)', W-1, 0], ['BL(0,H-1)', 0, H-1], ['BR(W-1,H-1)', W-1, H-1],
];
console.log('\nCorner alpha check on output:');
let allPass = true;
for (const [name, cx, cy] of corners) {
  const i = (cy * W + cx) * 4;
  const a = verify.data[i + 3];
  const pass = a === 0;
  if (!pass) allPass = false;
  console.log('  ' + name + ': alpha=' + a + (pass ? ' PASS' : ' FAIL'));
}
// Also verify center pixel is still opaque
const centerIdx = (Math.floor(H/2) * W + Math.floor(W/2)) * 4;
const centerAlpha = verify.data[centerIdx + 3];
console.log('  Center: alpha=' + centerAlpha + (centerAlpha > 200 ? ' PASS' : ' FAIL'));

if (allPass && centerAlpha > 200) {
  console.log('\nAll checks passed. Now run:');
  console.log('  npm run tauri -- icon src-tauri/icons/icon-source.png --output src-tauri/icons');
} else {
  console.error('\nSome checks FAILED.');
  process.exit(1);
}
