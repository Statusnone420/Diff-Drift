# User Guide

## Open a Repository

Start Diff Drift, choose **Open a repository**, and select a folder inside a git repo. Diff Drift discovers the repo root and compares the working tree against the chosen baseline (default: `HEAD`).

Browser mode (`npm run dev`) uses mock data. Native mode (`npm run tauri dev`) talks to the Rust backend.

## Choose a Baseline

The toolbar's **Review changes since** picker sets what the drift is measured against:

- **Last commit (HEAD)** — uncommitted changes only (the default).
- **Last review (trust point)** — everything since you last clicked **Mark reviewed**. This is the right baseline when an agent commits as it works: the drift stays visible after each commit. Locked until a review pins one.
- **Branch start (merge-base)** — everything this branch adds over `main`/`master`.
- **Custom ref…** — type any branch, tag, or SHA, then press Enter.

Unresolvable choices (unknown ref, no trust point yet) show an error and keep the current baseline.

## Read the App

The app has three main work areas:

- **Sidebar**: session counts, review progress, and analyzed files.
- **Center panel**: AST node cards for the selected file.
- **Right panel**: active and dismissed risk flags.

The top toolbar shows the current repo, branch, baseline, review progress, flag count, and review state.

## Understand Counts

- **Changed files**: every path git reports as different from the baseline, any file type.
- **Analyzed files**: changed TS/TSX/JS/JSX files Diff Drift parsed, plus `package.json` when its dependencies or scripts drifted.
- **Flags**: active heuristic findings. Dismissed flags stay in the session but do not count as active.
- **Reviewed**: changed nodes you've marked reviewed vs the total ( shown per file as `n/m` and drift-wide in the toolbar).
- **Node legend**: added, modified, and removed AST nodes inside the selected analyzed file.

## Review Flags

Each flag card shows its file path and node path (for example `validateToken › pattern`), so you can see what it points to before clicking. Click a risk flag to jump to the related node in the center panel. The flag text explains what changed and why it may need review.

Use **Dismiss** when a flag is understood and not actionable for the current drift. Dismissed flags are stored per repo and can be restored.

## Review Changes Node by Node

Hover a changed node card and click the check toggle to mark that change reviewed. Reviewed cards render quieter so the unreviewed remainder stands out — on a 40-file sweep, that's your "what's left" view. If a reviewed node's content drifts again, it automatically flips back to unreviewed.

Use **Mark reviewed** when you have reviewed the whole drift. It records the drift fingerprint (auto-clears when the drift changes), marks every changed node reviewed, and pins the **trust point** to the current commit.

## Export a Report

Use **Export report** to save the current session as Markdown or JSON. Markdown is meant for PR comments, handoff notes, or agent review loops. JSON is useful for debugging or automation and carries a `schemaVersion`.

Reports include the repo, branch, baseline, flag state, reviewed state, analyzed files, and node evidence for each flag.

## Headless Check (for scripts and agents)

The CLI is not a separate install — it is the same `diff-drift.exe` the app runs, invoked from a terminal with the `check` subcommand. The Windows installer puts it at `%LOCALAPPDATA%\Diff Drift\diff-drift.exe` by default (per-user install). In a development checkout it is `src-tauri\target\debug\diff-drift.exe` after a build.

Run it with the full path, or add the install folder to your `PATH` once:

```powershell
& "$env:LOCALAPPDATA\Diff Drift\diff-drift.exe" check . --json

# Optional: make `diff-drift` available everywhere (new terminals)
[Environment]::SetEnvironmentVariable("Path", "$([Environment]::GetEnvironmentVariable('Path','User'));$env:LOCALAPPDATA\Diff Drift", "User")
```

Usage:

```bash
diff-drift check [path] [--json|--md] [--baseline <head|trust-point|merge-base|rev>]
```

It prints the session (JSON by default) and exits with the highest active severity: `0` none, `1` low, `2` medium, `3` high, `64` usage error (`--help` exits `0`). Dismissed flags don't count — it reads the same per-repo triage state as the app, and never writes anything. An explicit `--baseline` that can't resolve (unknown ref, trust point not pinned, no default branch for a merge-base) fails with exit `64` and the cause on stderr — it never silently falls back to `HEAD`. Example gate in an agent hook or CI step:

```bash
diff-drift check . --baseline trust-point || echo "drift needs review"
```

## What Diff Drift Is Not

Diff Drift is not a full static analyzer, package scanner, or proof of safety. It is a focused drift review tool for code changes that deserve a second look.
