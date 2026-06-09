# Diff Drift

An **AST-level security inspector for AI coding agents.** Point it at a git repository and it
shows a **structured node-tree diff** (not a raw text diff) of the **uncommitted working-tree
changes vs HEAD** — and live-flags the security risks hiding in that drift: a widened
validation regex, an unvetted dependency, a hardcoded secret, an `eval`/`child_process` sink,
disabled TLS verification, broadened CORS, weakened cookie flags, a removed sanitizer, a
downgraded crypto call, an emptied log-redaction list, and so on.

A "session" is simply **everything that changed since the last commit** — exactly what an AI
coding agent leaves behind after editing your working tree.

It's a native **Windows 11 (Fluent)** desktop app: **Tauri 2.0** (Rust core + WebView2) with a
**React + TypeScript + Vite** frontend. All analysis runs in the Rust core — no Node sidecar.

## Features

- **Open any git repo** — native folder picker; the chosen repo is remembered and reopened on
  next launch. A calm onboarding screen when nothing's open; a clean "No drift detected" state
  when the working tree has no changes.
- **Live watching** — a debounced `notify` watcher re-analyzes on every save and pushes the
  update to the UI in place (no flicker, selection preserved). Re-analysis is **incremental**:
  only the file(s) you touched are re-parsed, not the whole changeset.
- **AST node diff** — node cards with kind glyphs, added/modified/removed state badges,
  severity flag chips, and inline before/after diff bodies; click a flag to jump+pulse its node.
- **A real rule engine** — a registry of CWE-mapped security rules (see below), package.json-aware
  so declared dependencies don't get flagged; each rule is independently unit-tested.
- **Flag triage** — dismiss a false positive per flag (or "Dismiss all"); dismissed flags drop
  out of every count, move to a collapsed "Dismissed" section, and persist per repo across
  restarts. Restoring is one click.
- **Session approval** — "Approve session" records a fingerprint of the exact drift you
  reviewed. Any further change to the drift (new edit, new flag, revert) auto-revokes the
  approval, so a stale "Approved" can never lie at you.
- **Export report** — save the session as a Markdown report (flags grouped by severity with
  before/after diffs, dismissed list, approval status, changed files) or as raw JSON, via the
  native save dialog.
- **Honest session metadata** — project, branch, changed-file count, risk count, and
  "watching since" are all real; nothing is faked.
- **Native Windows 11 chrome** — custom 38px Mica title bar, `decorations:false` window with
  DWM-rounded corners + native drop shadow, working caption buttons, drag + edge-resize.

## Prerequisites

- **Node.js** 18+ and npm
- **Rust** (stable, `x86_64-pc-windows-msvc`) via [rustup](https://rustup.rs)
- **Microsoft C++ Build Tools** ("Desktop development with C++")
- **WebView2 runtime** (preinstalled on Windows 11)

> No system `git` binary is required — git access is via the in-process `git2` (libgit2) crate.

## Run

```bash
npm install
npm run tauri dev      # build the Rust core + launch the app
```

On launch you'll get the onboarding screen — click **Open a repository…** and pick any git
repo with uncommitted changes. The last repo is remembered for next time; the project name in
the toolbar reopens the picker to switch.

Frontend-only (renders in a plain browser on mock data — useful for fast UI iteration; the
native folder picker / watcher only exist inside Tauri):

```bash
npm run dev            # http://localhost:1420
```

Production build / installer: `npm run tauri build`.

Optional: `npm run seed:demo` recreates a small sample repo at `demo/payments-api` (the
original "payments-api" narrative) to try the tool against — purely a dev convenience; the app
never seeds or defaults to it.

## How analysis works

All in the Rust core ([`src-tauri/src`](src-tauri/src)):

1. **git** ([`git.rs`](src-tauri/src/git.rs)) — `git2`: discover/validate the repo, list
   changed `.ts/.tsx` files, read each at `HEAD` (before) vs the working tree (after).
2. **parse** ([`parse.rs`](src-tauri/src/parse.rs)) — `tree-sitter-typescript` → a node tree
   (kinds, names, signatures, dedented source lines).
3. **diff** ([`diff.rs`](src-tauri/src/diff.rs)) — LCS match on `(kind, name)` → added /
   removed / modified / unchanged; whitespace-only changes read as "formatting only". Node ids
   are structure-derived (never body text) so selection survives live re-analysis.
4. **rules** ([`rules.rs`](src-tauri/src/rules.rs)) — a `RuleRegistry` of `trait Rule`
   predicates over each node → risk flags. Each rule has a unit test.
5. **watch** ([`watcher.rs`](src-tauri/src/watcher.rs)) — a debounced `notify` watcher
   re-runs steps 1–4 for the changed path(s), merges into the cached result map, and emits
   `drift://updated` to the frontend.
6. **triage** ([`store.rs`](src-tauri/src/store.rs)) — dismissed flags + the approved drift
   fingerprint persist per repo in `repo-state.json` (app config dir, next to
   `settings.json`); [`report.rs`](src-tauri/src/report.rs) renders the exported
   Markdown/JSON report.

### Rule set (CWE-mapped, low-false-positive)

Hardcoded secret (AWS/OpenAI/PEM markers) · `eval` / `new Function` · `child_process` exec ·
disabled TLS (`rejectUnauthorized:false`, `NODE_TLS_REJECT_UNAUTHORIZED=0`) · broadened CORS ·
weakened cookie flags (`httpOnly`/`secure`/`sameSite`) · widened validation regex ·
`verify`→`decode` crypto downgrade · disabled `if (false)` guard · removed sanitizer ·
permissive logging · unvetted (undeclared) dependency import. Adding a rule = one small
predicate + one test in [`rules.rs`](src-tauri/src/rules.rs).

## Data contract

Frontend and backend meet at [`src/types.ts`](src/types.ts) /
[`src-tauri/src/model.rs`](src-tauri/src/model.rs): a `Session` meta object, a flat `Flag[]`
list, and `FileEntry[]` each holding a recursive `AstNode[]` tree. Each flag maps to a node via
`fileId` + `nodeId`.

## Project structure

```
src/
  App.tsx                  state machine: onboarding/loading/loaded/clean + live updates + triage
  types.ts                 the data contract
  lib/session.ts           commands (open/init/dismiss/approve/export), folder picker, drift listener (+ mock fallback)
  lib/window.ts            caption-button wrapper (Tauri window API + browser no-op)
  lib/icons.tsx            inline SVG icons + glyph/severity maps
  components/              TitleBar, Toolbar, Sidebar, Center, NodeCard, DiffBody, RightPanel, EmptyState
  data/mockSession.ts      typed mock (browser fallback)
  styles/{tokens,app}.css  design tokens + component styles
src-tauri/src/
  lib.rs                   commands (open_repo / init_session / triage / export_report) + persistence + DWM chrome
  git.rs parse.rs diff.rs rules.rs heuristics.rs session.rs watcher.rs store.rs report.rs model.rs
  test_fixture.rs          cfg(test): builds the demo scenario in a temp repo (hermetic `cargo test`)
scripts/seed-demo.mjs      optional sample repo generator
design_handoff_drift_inspector/  the original HTML/CSS/React design reference
```

### Possible next steps

A recent-repos list; richer signature/secret heuristics; cross-platform window chrome
(macOS/Linux); and a parallel initial scan (`rayon`) if a very large monorepo's first load
ever feels slow (steady-state watching is already incremental).

## License

See [LICENSE](LICENSE).
