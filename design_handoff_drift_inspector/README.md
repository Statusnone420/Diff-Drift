# Handoff: Drift Inspector — AST-level security inspector for AI coding agents

## Overview
Drift Inspector is a desktop developer tool. After an AI coding agent edits files in a
session, it parses those files and shows an **AST-level diff** (structured node tree, not a
raw text diff) that highlights security risks the agent may have introduced. This package
covers the **primary inspection screen** styled as a **Windows 11 (Fluent) desktop app**.

## About the Design Files
The files in this bundle are **design references created in HTML/CSS/React** — a working
prototype showing the intended look and behavior. They are **not** production code to ship
as-is. The task is to **recreate this design natively** inside the target app's environment.

Recommended target (matches the brief — "native one" for Windows 11):
- **Tauri 2.0** shell (Rust core + WebView2 on Windows) with a **React + TypeScript + Vite**
  frontend. The HTML prototype maps almost 1:1 onto a Tauri frontend.
- Use Tauri's window APIs for the real title bar / caption buttons instead of the mocked
  ones (see "Window chrome" below).
- For real AST parsing, the Rust side (or a Node sidecar) should use a parser such as
  **tree-sitter** or **@typescript-eslint/typescript-estree** / **Babel** and emit the node
  model described under "Data model".

If the team prefers a different stack, keep the layout, tokens, and interactions identical
and re-skin with their component library.

## Fidelity
**High-fidelity.** Final colors, typography, spacing, and interactions. Recreate
pixel-for-pixel using the codebase's libraries. Exact values are in "Design Tokens".

---

## Window chrome (Windows 11 / Tauri specifics)
The prototype *mocks* the OS window. In the real Tauri app:
- Set `decorations: false` and `transparent: true` in `tauri.conf.json`, draw a custom
  title bar, and wire buttons to `appWindow.minimize()`, `toggleMaximize()`, `close()`.
- Title bar height **38px**, command toolbar **46px**.
- Caption buttons sit flush top-right, each **46px wide × full title-bar height**.
  - Minimize: 10px horizontal line. Maximize: 10px square (1px stroke, 0.5 radius).
    Close: 10px "X" (1px stroke).
  - Hover: minimize/maximize → `rgba(255,255,255,0.07)`; **close → `#c42b1c` bg, white icon**.
- Title bar is the drag region (`data-tauri-drag-region`); caption buttons are no-drag.
- Window: `border-radius: 9px`, `1px solid rgba(255,255,255,0.085)` border, large soft
  drop shadow. Title bar uses a **Mica** look: `linear-gradient(180deg, rgba(40,43,51,.72),
  rgba(22,24,30,.72))` with `backdrop-filter: blur(30px) saturate(1.3)`.
- The desktop backdrop behind the window (layered radial blue/violet glows on `#07080a`) is
  only for the prototype; the real app's window floats on the user's actual desktop.

---

## Screen: Inspection View
Single screen, design width **1440px** (min usable ~1200px). Three-column body under the
title bar + toolbar.

### Top: Command toolbar (46px, below title bar)
- **Left — breadcrumb:** project name `payments-api` (weight 600) · branch
  `agent/refactor-token-validation` (monospace, with a small branch glyph).
- **Right — summary pill + actions:**
  - Summary pill: red-tinted (`--sev-high-bg`), 1px red-ish border, 6px radius, a 6px glowing
    red dot, text "**3 risks** across **2 files**" (numbers bold/white).
  - Buttons: "Dismiss all" (subtle) and "Approve session" (**amber accent** primary:
    bg `rgba(231,168,62,.14)`, text `#f0c069`, border `rgba(231,168,62,.42)`).

### Left column — Session sidebar (width 256px)
- `bg: --bg-panel-2`, 1px right border.
- **Header:** uppercase label "SESSION" + a live pill "● Active" (green, the dot has a 2s
  pulsing ring animation).
- **Meta grid (2 cols):** cells "Edits = 14", "Elapsed = 6m 41s" (mono values), and a
  full-width "Agent = Claude Sonnet" cell with a small amber gradient avatar (sparkle icon).
- **"FILES TOUCHED" list** (count badge "3"): one row per file:
  - File glyph + filename (mono, e.g. `validateToken.ts`) + dir line (`auth/`, mute).
  - Right-aligned **risk-count badge**: 2 risks → red badge, 1 → amber badge, 0 → muted gray.
  - **Selected row:** `bg: --bg-row-sel`, 1px `--border-strong`, plus a **Fluent selection
    indicator** — a 3px × 18px vertically-centered amber rounded bar on the left edge. The
    file glyph turns amber. Selected by default: `validateToken.ts`.
- **Footer:** "● Watching working tree" (green dot), top border.

### Center column — AST node comparison (the hero, flex: 1fr)
- **Header:** file path `auth/`**`validateToken.ts`** (dir is faint, name bold; mono).
  Sub-line: language `TypeScript` · summary `1 added · 3 modified · 1 removed`.
  Right side: a **legend** — `+N added` (green sw), `~N modified` (amber sw), `−N removed`
  (red sw); counts computed from the node tree.
- **Tree:** vertical stack of node cards. Children are nested under a parent with a **13px
  left margin + 17px left padding + 1px left guide line** (`--border-strong`).
- **Node card** (`--bg-panel`, 9px radius, 1px border tinted by state):
  - **Header row:** a 22px square **kind glyph** (mono abbrev: `im` import, `fn` function,
    `let` var, `if`, `()` expression, `ret` return, `ex` export — glyph tinted by state),
    then a two-line title: **name** (mono, weight 600) + optional **signature** (faint,
    truncates with ellipsis) on row 1, **node kind** (faint mono, e.g. `ImportDeclaration`)
    on row 2.
  - Right side of header: optional **flag chip** (severity-colored, warning icon + "High/
    Medium/Low"), a **state badge** ("ADDED" / "MODIFIED" / "REMOVED", state-colored), and a
    chevron (rotates 90° when open).
  - **Changed nodes are expanded** by default to show an inline **diff body** below the
    header (`--bg-panel-2`, mono 12px): removed lines prefixed `−` on a faint red wash
    (`--removed-line`), added lines prefixed `+` on a faint green wash (`--added-line`), a
    1px separator between the before and after groups. Added node → only `+` lines; removed
    node → only `−` lines; modified → both.
  - **Unchanged container nodes** (e.g. the `validateToken` function) render as a calm card
    with no diff body, and hold changed children.
- **Active/highlighted node:** colored ring — `box-shadow: 0 0 0 1px <sev>, 0 0 22px -4px
  <sev>` + border in the same color, where `<sev>` is the mapped flag's severity color
  (high=red, med=amber, low=blue). A 0.72s `nodePulse` animation fires on activation.
- If a file has no structural changes (e.g. `session.ts`): show a calm green-tinted note
  "No security-relevant structural changes in this file." above the (dimmed) tree.

### Right column — Risk Flags (width 372px)
- `bg: --bg-panel-2`, 1px left border.
- **Header:** flag icon + "Risk Flags" + a mono count chip "3"; right side "severity ↓".
- **Flag cards** (10px radius), one per risk, sorted by severity:
  - A 3px top **accent bar** in the severity color.
  - Header: **severity badge** (warning icon + "High/Medium/Low", severity-tinted) + risk
    **type** label (weight 600).
  - One-line plain-English **description**.
  - Footer (top border): mapped **file path** (mono) on the left, "→ node" jump affordance on
    the right (turns amber on hover/active).
  - **Active flag:** elevated bg + a severity-colored glow ring matching its node.
- **Footer:** "Export report" (subtle) + "Resolve all" (amber primary), top border.

---

## Interactions & Behavior
- **Select a file** (left list): switches the center tree to that file; clears the active
  node/flag highlight.
- **Click a risk flag** (right): (1) switches to the flag's file if needed, (2) smooth-scrolls
  the center tree to the mapped node, (3) highlights + pulses that node in the flag's severity
  color, (4) marks the flag card active. Scroll offset ≈ node top − 96px; never use
  `scrollIntoView` — compute against the scroll container's rect.
- **Click a node's flag chip** (center): the reverse tie — activates the corresponding flag in
  the right panel and re-pulses the node.
- **Click a changed node header:** toggles its diff body open/closed (chevron rotates).
- **Live "Active" pill:** 2s infinite pulsing ring on the green dot.
- Defaults on load: file `validateToken.ts` selected, node `pattern` active, flag `f1` (High)
  active and scrolled into view.

## State Management
- `selectedFileId` — which file's tree is shown (default `"auth"`).
- `activeNodeId` — currently highlighted node (default `"n_pattern"`).
- `activeFlagId` — currently selected flag (default `"f1"`).
- `pulseId` — transient; set to a node id to fire the pulse, cleared after ~720ms.
- Per-node `open` state for the expand/collapse chevron (changed nodes default open).
- Derived: legend counts (walk the node tree counting added/removed/modified); risk badge
  color from `file.risks`.

## Data model
A session has `session` meta, a flat `flags[]` list, and `files[]`, each with a recursive
`nodes[]` tree. Each flag maps to a node via `fileId` + `nodeId`. Node shape:
```ts
type NodeState = "added" | "removed" | "modified" | "unchanged";
interface AstNode {
  id: string;
  kind: string;            // "ImportDeclaration", "FunctionDeclaration", ...
  name: string;            // display name
  signature?: string;      // dim trailing text
  state: NodeState;
  flagId?: string;         // ties to a risk flag
  before?: string[];       // removed/old lines (for removed + modified)
  after?: string[];        // added/new lines (for added + modified)
  children?: AstNode[];
}
interface Flag {
  id: string; severity: "high" | "medium" | "low";
  type: string; desc: string;
  fileId: string; filePath: string; nodePath: string; nodeId: string;
}
```
The full realistic mock dataset is in **`data.jsx`** (3 files, a JS/TS AST, 3 flags). Use it
as the contract for the parser's output. The narrative: the agent added an unvetted
`jwt-tiny-decode` import, widened a validation regex to `/.*/`, neutered an `if` guard,
removed `sanitizeInput`, swapped `verify(...)` for `decode(...)`, and emptied the logger's
redaction list.

---

## Design Tokens

### Colors
```
Surfaces
--bg              #0b0c0f      app background
--bg-panel        #101116      cards
--bg-panel-2      #0d0e12      side panels / diff body
--bg-elevated     #16181f      buttons, chips
--bg-hover        #181a22
--bg-row-sel      #1b1e27      selected file row

Borders
--border          #20232b
--border-soft     #191b22
--border-strong   #2c303a

Text
--text            #e9ebef
--text-dim        #9498a3
--text-faint      #5c616d
--text-mute       #474c57

Node states
--added           #4ec46a   (bg wash 10%, line wash 7%)
--removed         #f2604c   (bg wash 10%, line wash 7%)
--modified        #e7a83e   (bg wash 11%)

Severity
--sev-high        #f2604c   (bg rgba(242,96,76,.13))
--sev-med         #e7a83e   (bg rgba(231,168,62,.13))
--sev-low         #6f8bc4   (bg rgba(111,139,196,.13))

Accent / status
--accent          #e7a83e   (selection, primary action)
--live            #4ec46a   (Active pill, watching)
close-hover       #c42b1c

Mica
--mica-1          rgba(40,43,51,0.72)
--mica-2          rgba(22,24,30,0.72)
```

### Typography
```
UI font:    "Segoe UI Variable Text", "Segoe UI Variable", "Segoe UI", system-ui
Mono font:  "Cascadia Code", "Cascadia Mono", "JetBrains Mono", Consolas, monospace
Base size:  13px / line-height 1.5
Section labels: 11px, weight 600, letter-spacing 0.6–0.7px, UPPERCASE, --text-faint
Node name / file name: 13px mono weight 600
Diff code: 12px mono, line-height 1.65
```

### Spacing, radius, shadow
```
Window radius     9px        Cards 9–10px     Buttons/pill 5–6px     Chips/badges 5px
Title bar 38px    Toolbar 46px    Sidebar 256px    Right panel 372px    Desktop pad 20px
Card shadow       0 1px 0 rgba(255,255,255,.02) inset, 0 8px 24px -12px rgba(0,0,0,.6)
Window shadow     0 0 0 1px rgba(0,0,0,.5), 0 40px 90px -28px rgba(0,0,0,.78),
                  0 12px 36px -18px rgba(0,0,0,.6)
Active-node ring  0 0 0 1px <sev>, 0 0 22px -4px <sev>
```

## Assets
No external image assets — the logo, caption-button icons, node glyphs, and all UI icons are
inline SVG (see `app-win.jsx`, the `Ico` object and `TitleBar`). On a real Windows build,
prefer **Segoe Fluent Icons** for caption glyphs. Fonts (Segoe UI Variable, Cascadia Code)
ship with Windows 11 — no bundling needed there.

## Files in this bundle
- `Drift Inspector (Windows 11).html` — the screen shell, all CSS/tokens, and script wiring.
- `app-win.jsx` — React components (TitleBar, Toolbar, Sidebar, Center/NodeCard/DiffBody,
  RightPanel, App) and all interaction logic.
- `data.jsx` — the mock session dataset (the parser-output contract).
Open the HTML in a browser to interact with the reference before building.
