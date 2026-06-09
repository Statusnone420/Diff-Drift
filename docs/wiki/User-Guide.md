# User Guide

## Open a Repository

Start Diff Drift, choose **Open a repository**, and select a folder inside a git repo. Diff Drift discovers the repo root and compares uncommitted changes against `HEAD`.

Browser mode (`npm run dev`) uses mock data. Native mode (`npm run tauri dev`) talks to the Rust backend.

## Read the App

The app has three main work areas:

- **Sidebar**: session counts and analyzed TypeScript/TSX files.
- **Center panel**: AST node cards for the selected file.
- **Right panel**: active and dismissed risk flags.

The top toolbar shows the current repo, branch, flag count, and review state.

## Understand Counts

- **Changed files**: all uncommitted git changes, any file type.
- **TS/TSX analyzed**: changed `.ts` and `.tsx` files that Diff Drift parsed and displayed.
- **Flags**: active heuristic findings. Dismissed flags stay in the session but do not count as active.
- **Node legend**: added, modified, and removed AST nodes inside the selected analyzed file.

## Review Flags

Click a risk flag to jump to the related node in the center panel. The flag text explains what changed and why it may need review.

Use **Dismiss** when a flag is understood and not actionable for the current drift. Dismissed flags are stored per repo and can be restored.

Use **Mark reviewed** when you have reviewed the current drift. The reviewed state is tied to the current drift fingerprint and clears when the drift changes.

## Export a Report

Use **Export report** to save the current session as Markdown or JSON. Markdown is meant for PR comments, handoff notes, or agent review loops. JSON is useful for debugging or automation.

Reports include the repo, branch, flag state, reviewed state, analyzed files, and node evidence for each flag.

## What Diff Drift Is Not

Diff Drift is not a full static analyzer, package scanner, or proof of safety. It is a focused working-tree review tool for code drift that deserves a second look.
