# Architecture

This page is for contributors and AI agents getting oriented in the codebase.

## Stack

- Desktop shell: Tauri 2
- Backend: Rust
- Frontend: React, TypeScript, Vite
- Parsers: `tree-sitter-typescript`, `tree-sitter-javascript`
- Git access: `git2`
- Watcher: `notify` and `notify-debouncer-full`
- Tests: Rust unit tests, Playwright web E2E, Playwright native Tauri E2E; CI runs cargo test + clippy (Windows) and tsc + web E2E (Ubuntu)

## Backend Flow

1. `src-tauri/src/lib.rs` exposes Tauri commands; `main.rs` routes `check` to the headless CLI first.
2. `git.rs` discovers repo roots, branch names, changed files (vs `HEAD` or any baseline commit), and file contents at any rev.
3. `session.rs` resolves the per-repo baseline (`resolve_baseline`), orchestrates file analysis, and assembles `SessionData` (including per-node review marking).
4. `parse.rs` parses TS/TSX/JS/JSX (`Lang`) into a small `Parsed` tree.
5. `diff.rs` compares before/after parsed trees and assigns stable node IDs.
6. `rules.rs` evaluates security heuristics; `heuristics.rs` walks nodes and attaches flags.
7. `deps_diff.rs` renders a drifted package.json's dependency/script sections as drift nodes with lockfile-aware rules.
8. `watcher.rs` keeps a debounced live session cache, re-resolves the baseline on git-state changes, and emits `drift://updated`.
9. `store.rs` persists per repo: dismissed flags, reviewed fingerprint, baseline choice, trust point, and per-node review hashes.
10. `report.rs` renders Markdown or JSON exports.
11. `cli.rs` implements `diff-drift check` — read-only headless analysis with severity exit codes.

## Frontend Flow

1. `src/lib/session.ts` wraps Tauri commands and provides browser-mode mock behavior.
2. `src/App.tsx` owns app state, repo opening, live updates, selection, triage, review, and export.
3. `src/components/Toolbar.tsx` renders repo, branch, counts, and review actions.
4. `src/components/Sidebar.tsx` renders session counts and analyzed files.
5. `src/components/Center.tsx` renders the selected file's node tree.
6. `src/components/NodeCard.tsx` renders node state, diff body, and flag chips.
7. `src/components/RightPanel.tsx` renders active and dismissed flags.

## Data Contract

The shared TypeScript shape is in `src/types.ts`; the Rust mirror is in `src-tauri/src/model.rs`.

Important objects:

- `SessionData`: full payload sent to React; carries `schemaVersion` (currently 3 — bump it when the shape changes in ways consumers could misread).
- `Session`: repo, branch, baseline (spec/label/trust point), counts, review progress, reviewed state.
- `FileEntry`: analyzed file metadata, review progress, and node tree.
- `AstNode`: parsed structure, before/after lines, and per-node `reviewed` state.
- `Flag`: heuristic finding attached to a node.

Keep these contracts in sync. If a field changes in Rust, update TypeScript, mock data, reports, tests — and `SCHEMA_VERSION` in `model.rs` when the change is breaking.

## Design Constraints

- Stay local-only. Do not add telemetry, model calls, or remote repo uploads.
- Prefer focused heuristics over broad static-analysis claims.
- Keep docs and UI honest about scope: changed TS/TSX/JS/JSX drift plus package.json dependency drift.
- The headless `check` command stays read-only: it must never mutate triage state.
- Make surgical changes. Avoid large rewrites unless a specific bug requires them.
