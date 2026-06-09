# Diff Drift

<p align="center">
  <img src="src-tauri/icons/128x128.png" alt="Diff Drift app icon" width="96" height="96">
</p>

<p align="center">
  <strong>AST-level security review for the uncommitted code drift left by AI coding agents.</strong>
</p>

<p align="center">
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-4ec46a?style=for-the-badge"></a>
  <img alt="Version 0.1.1" src="https://img.shields.io/badge/version-0.1.1-e7a83e?style=for-the-badge">
  <img alt="Windows 11" src="https://img.shields.io/badge/platform-Windows%2011-6f8bc4?style=for-the-badge">
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri-2-24c8db?style=for-the-badge">
  <img alt="Rust core" src="https://img.shields.io/badge/Rust-core-f2604c?style=for-the-badge">
  <img alt="Playwright and axe" src="https://img.shields.io/badge/E2E-Playwright%20%2B%20axe-4ec46a?style=for-the-badge">
</p>

<p align="center">
  <a href="#quick-start">Quick start</a> ·
  <a href="#installer">Installer</a> ·
  <a href="#automation">Automation</a> ·
  <a href="LICENSE">License</a>
</p>

![Diff Drift reviewing mock payments-api drift](docs/assets/diff-drift-mock-session.png)

The screenshot uses mock data from the browser-mode fallback. It does not expose a local machine path or private repository.

## What It Does

Diff Drift opens a git repository, compares the working tree against `HEAD`, and renders changed TypeScript/TSX as a structured AST diff instead of a raw text patch. It live-flags security-relevant drift such as widened validation regexes, removed sanitizers, hardcoded secrets, disabled TLS checks, broadened CORS, downgraded crypto verification, permissive logging, and unvetted dependencies.

A session is simply everything that changed since the last commit, which is exactly the review surface an AI coding agent leaves behind.

## Highlights

- Native Windows desktop app built with Tauri 2, Rust, React, TypeScript, Vite, and WebView2.
- Rust core handles git access, tree-sitter parsing, AST diffing, rule evaluation, watching, triage state, approval fingerprints, and Markdown/JSON export.
- Live watcher re-analyzes on save, preserves selection, and revokes approval when meaningful drift changes.
- Triage supports per-flag dismiss/restore, dismiss all, dismissed sections, and per-repo persistence.
- Export produces a Markdown report suitable for PR comments, handoff notes, or agent review loops.
- Version is displayed subtly in the titlebar and injected from `package.json` during Vite builds.
- Automated QA covers Rust unit tests, browser Playwright/axe checks, and native Tauri WebView2 E2E flows.

## Quick Start

Prerequisites:

- Node.js 18+ and npm
- Rust stable with `x86_64-pc-windows-msvc`
- Microsoft C++ Build Tools with "Desktop development with C++"
- WebView2 runtime, included with Windows 11

Run the native app:

```bash
npm install
npm run tauri dev
```

Run browser-mode mock data for fast UI iteration:

```bash
npm run dev
```

Then open the local Vite URL and click **Open a repository**. In browser mode, that loads the mock session used by the README screenshot.

## Installer

Build the Windows NSIS installer:

```bash
npm run tauri -- build --bundles nsis
```

The installer is written under:

```text
src-tauri/target/release/bundle/nsis/Diff Drift_0.1.1_x64-setup.exe
```

The bundled app icon is generated from `src-tauri/icons/icon-source.png` into the Tauri icon set, including `src-tauri/icons/icon.ico` for Windows shortcuts and installed-app listings.

## Automation

Useful commands:

```bash
npm run test:rust          # Rust core and report/rule tests
npm run build              # TypeScript + Vite production build
npm run test:e2e:web       # Browser Playwright + axe checks
npm run test:e2e:tauri     # Native Tauri/WebView2 E2E
npm run test:verify        # Rust + build + browser E2E
npm run test:verify:native # Native E2E wrapper
```

Native E2E launches the built app with isolated environment variables:

- `DIFF_DRIFT_E2E_REPO`
- `DIFF_DRIFT_E2E_EXPORT_PATH`
- `DIFF_DRIFT_E2E_STATE_FILE`
- `DIFF_DRIFT_E2E_BIN` when a test needs a separate executable path

That keeps automated runs away from the user's installed app, normal settings file, and native save dialogs.

## How Analysis Works

All analysis runs in `src-tauri/src`:

1. `git.rs` discovers the repo, lists changed files, and reads `HEAD` versus working-tree contents.
2. `parse.rs` builds TypeScript/TSX AST node trees with tree-sitter.
3. `diff.rs` matches nodes by kind/name and marks added, removed, modified, and unchanged nodes.
4. `rules.rs` evaluates CWE-mapped security predicates against changed AST nodes.
5. `watcher.rs` debounces file-system changes and emits updated sessions to the frontend.
6. `store.rs` persists dismissed flags and approval fingerprints per repo.
7. `report.rs` renders Markdown or JSON exports.

The exported report counts total git drift in the session summary. Its analyzed-file section lists only TypeScript/TSX files that Diff Drift parsed as AST nodes.

## Data and Privacy

Diff Drift runs locally. There is no Node sidecar, remote service, model API call, telemetry pipeline, or app-owned server receiving repository contents. Git access is handled in-process with `git2`/libgit2.

Stored local state is limited to:

- Last opened repository path.
- Per-repo dismissed flag IDs.
- Per-repo approval fingerprint and approval timestamp.

## Project Structure

```text
src/
  App.tsx                  App state, live updates, triage, approval, export
  components/              TitleBar, Toolbar, Sidebar, Center, NodeCard, RightPanel
  data/mockSession.ts      Browser-mode mock data
  lib/session.ts           Tauri commands, mock fallback, E2E export seam
  lib/version.ts           Build-injected app version
  styles/                  Design tokens and app CSS

src-tauri/src/
  git.rs                   libgit2 repo access
  parse.rs                 tree-sitter TypeScript parsing
  diff.rs                  AST node diffing
  rules.rs                 Security rule registry
  watcher.rs               Live watch and session merging
  store.rs                 Per-repo triage persistence
  report.rs                Markdown/JSON report rendering
  lib.rs                   Tauri commands and Windows chrome

tests/
  e2e-web/                 Browser-mode Playwright + axe tests
  e2e-tauri/               Native Tauri/WebView2 E2E tests
```

## License

Diff Drift is open source under the [MIT License](LICENSE).

This project is not affiliated with Microsoft, GitHub, Tauri, or OpenAI.
