# Architecture

This page is for contributors and AI agents getting oriented in the codebase.

## Stack

- Desktop shell: Tauri 2
- Backend: Rust
- Frontend: React, TypeScript, Vite
- Parser: `tree-sitter-typescript`
- Git access: `git2`
- Watcher: `notify` and `notify-debouncer-full`
- Tests: Rust unit tests, Playwright web E2E, Playwright native Tauri E2E

## Backend Flow

1. `src-tauri/src/lib.rs` exposes Tauri commands.
2. `git.rs` discovers repo roots, branch names, changed files, and file contents.
3. `session.rs` orchestrates file analysis and assembles `SessionData`.
4. `parse.rs` parses TypeScript/TSX into a small `Parsed` tree.
5. `diff.rs` compares before/after parsed trees and assigns stable node IDs.
6. `rules.rs` evaluates security heuristics.
7. `heuristics.rs` walks nodes and attaches flags.
8. `watcher.rs` keeps a debounced live session cache and emits `drift://updated`.
9. `store.rs` persists dismissed flags and reviewed fingerprints per repo.
10. `report.rs` renders Markdown or JSON exports.

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

- `SessionData`: full payload sent to React.
- `Session`: repo, branch, counts, reviewed state.
- `FileEntry`: analyzed file metadata and node tree.
- `AstNode`: parsed structure plus before/after lines.
- `Flag`: heuristic finding attached to a node.

Keep these contracts in sync. If a field changes in Rust, update TypeScript, mock data, reports, and tests.

## Design Constraints

- Stay local-only. Do not add telemetry, model calls, or remote repo uploads.
- Prefer focused heuristics over broad static-analysis claims.
- Keep docs and UI honest about scope: changed `.ts` and `.tsx` drift.
- Make surgical changes. Avoid large rewrites unless a specific bug requires them.
