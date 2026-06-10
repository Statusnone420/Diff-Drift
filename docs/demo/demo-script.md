# Demo Script

One GIF, 15–20 seconds, no sound. The viewer should leave understanding the loop: an agent changed code, Diff Drift shows the drift since a trusted point, a human reviews it, evidence comes out.

The scenario is the bundled `payments-api` mock session: an agent branch (`agent/refactor-token-validation`) that widened a token regex to `/.*/`, neutered the guard clause, removed `sanitizeInput`, swapped signature verification for a bare decode, and emptied the logger's redaction list. Three flags, six changed nodes, three files.

## Storyboard

| # | Hold | On screen | Caption |
| --- | --- | --- | --- |
| 1 | ~3 s | Loaded session: drift list, branch name, Risk Flags panel with 3 flags | An AI agent changed payments code. |
| 2 | ~2.5 s | Scope picker open: Current work / Entire branch / Since last review | Compare against a baseline you trust. |
| 3 | ~3 s | The high flag selected: `pattern` node, before `/^[A-Za-z0-9_\-]{32,}$/` → after `/.*/` | Risky drift, node by node. |
| 4 | ~2.5 s | One node marked reviewed; progress ticks to 1/6 | Review what matters. Dismiss what doesn't. |
| 5 | ~2.5 s | Drift marked reviewed; trust point pinned; "Since last review" unlocks in the scope picker | Your review becomes the next baseline. |
| 6 | ~3 s | Export report clicked; "Exported" confirmation | Export evidence for the PR. |

Caption style: one short line per beat, lower-third bar, single typeface, no emoji. Crop to the app window — no desktop, no browser chrome.

## Recording it the automated way

```bash
npm install
npm run demo:capture
```

This drives the browser UI (mock session — the same `SessionData` contract the Rust engine emits) through the storyboard with Playwright, writes keyframes to `docs/assets/demo/`, and encodes `docs/assets/diff-drift-demo.gif`. No screen recording, no ffmpeg.

## Recording it by hand (if you want the native app instead)

About ten minutes:

1. `npm run seed:demo` — recreates `demo/payments-api` with the before/after drift.
2. `npm run tauri dev` and open `demo/payments-api` (or launch with no path for the default session).
3. Walk the six beats above. Pause two beats per scene; don't rush the regex flag — it's the money shot.
4. Record with any capture tool, crop to the window, export at ~920 px wide, under 20 seconds.
5. Save as `docs/assets/diff-drift-demo.gif`. The README already points there.
