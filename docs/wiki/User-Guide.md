# User Guide

## Open a Repository

Start Diff Drift, choose **Open a repository**, and select a folder inside a git repo. Diff Drift discovers the repo root and compares the working tree against the chosen baseline (default: `HEAD`).

Browser mode (`npm run dev`) uses mock data. Native mode (`npm run tauri dev`) talks to the Rust backend.

## Choose a Baseline

The toolbar's **Scope** menu sets what the drift is measured against:

- **Current work** — uncommitted changes only (the default).
- **Since last review** — everything since you last clicked **Mark reviewed**. This is the right scope when an agent commits as it works: the drift stays visible after each commit. Locked until a review pins one.
- **Entire branch** — everything this branch adds over `main`/`master`.
- **Custom ref** — type any branch, tag, or SHA, then press Enter.

Unresolvable choices (unknown ref, no trust point yet) show an error and keep the current baseline.

## Read the App

The app has three main work areas:

- **Sidebar**: session counts, review progress, and analyzed files.
- **Center panel**: AST node cards for the selected file.
- **Right panel**: active and dismissed risk flags.

The top toolbar shows the current repo, branch, baseline, review progress, flag count, and review state.

## Understand Counts

- **Changed files**: every path git reports as different from the baseline, any file type.
- **Analyzed files**: changed source files Diff Drift parsed as AST drift (TS, TSX, JS, JSX, Rust, Go, Python, Java, C#, Kotlin, Swift), plus `package.json` when its dependency or script sections changed. Files over 2 MB are not parsed — they stay in the list with the summary "Skipped — file too large to analyze" so the limit is visible. Review giant generated bundles by other means. Core language structural drift plus package.json dependency/script drift; heuristic flags are strongest for JS/TS and package drift.
- **Other changed files**: paths that changed but were not analyzed — unsupported file type (Markdown, TOML, YAML, images, etc.) or `package.json` with no dependency or script drift. These appear by path in the sidebar so they are never invisible. Review them outside Diff Drift.
- **Flags**: active heuristic findings. Dismissed flags stay in the session but do not count as active.
- **Reviewed**: changed nodes you've marked reviewed vs the total (shown per file as `n/m` and drift-wide in the toolbar).
- **Node legend**: added, modified, and removed AST nodes inside the selected analyzed file.

Zero flags means no active heuristic findings. It does not mean the drift is reviewed; if changed nodes remain, review progress still shows what needs a human pass.

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

The CLI ships as `diff-drift-cli.exe`, a console build of the same engine the app runs. The Windows installer puts it next to the app, at `%LOCALAPPDATA%\Diff Drift\diff-drift-cli.exe` by default (per-user install). Since v0.3.2 each release also carries the bare `diff-drift-cli.exe` as a downloadable asset (verify it against `SHA256SUMS.txt`), so scripts and CI can use the CLI without the installer. In a development checkout it is `src-tauri\target\debug\diff-drift-cli.exe` after a build.

Because it is a console binary, every shell waits for it and sees its real exit code — `$LASTEXITCODE` in PowerShell, `%ERRORLEVEL%` in cmd, `$?` in sh. (The app binary `diff-drift.exe` still answers `check` for existing hooks, but shells don't wait for GUI-subsystem binaries, so use `diff-drift-cli.exe` for anything scripted.)

Run it with the full path, or add the install folder to your `PATH` once:

```powershell
& "$env:LOCALAPPDATA\Diff Drift\diff-drift-cli.exe" check . --json
$LASTEXITCODE   # the severity exit code

# Optional: make `diff-drift-cli` available everywhere (new terminals)
[Environment]::SetEnvironmentVariable("Path", "$([Environment]::GetEnvironmentVariable('Path','User'));$env:LOCALAPPDATA\Diff Drift", "User")
```

Usage:

```bash
diff-drift-cli check [path] [--json|--md] [--baseline <head|trust-point|merge-base|rev>]
```

It prints the session (JSON by default) and exits with the highest active severity: `0` none, `1` low, `2` medium, `3` high, `64` usage error (`--help` exits `0`). Dismissed flags don't count — it reads the same per-repo triage state as the app, and never writes anything. An explicit `--baseline` that can't resolve (unknown ref, trust point not pinned, no default branch for a merge-base) fails with exit `64` and the cause on stderr — it never silently falls back to `HEAD`. Example gate in an agent hook or CI step:

```bash
diff-drift-cli check . --baseline trust-point || echo "drift needs review"
```

## CI and Hook Recipes

The contract that makes these safe to wire up: the CLI is **read-only** (never writes triage state), dismissed flags don't count, and an explicit `--baseline` that can't resolve exits `64` loudly instead of silently falling back to `HEAD`. Exit codes: `0` none, `1` low, `2` medium, `3` high.

**Pre-commit hook** (`.git/hooks/pre-commit`) — block commits that introduce high-severity drift:

```bash
#!/bin/sh
diff-drift-cli check . --json > /dev/null
code=$?
if [ "$code" -ge 3 ]; then
  echo "diff-drift: high-severity drift — review in the app or run: diff-drift-cli check . --md"
  exit 1
fi
```

**Agent hook** (e.g. a Claude Code Stop/PostToolUse hook) — make the agent's own loop fail until drift is reviewed:

```bash
#!/bin/sh
diff-drift-cli check . --baseline trust-point --md
code=$?
if [ "$code" -ge 2 ]; then exit 2; fi  # surface medium+ drift back to the agent loop
```

The same hook in PowerShell:

```powershell
& "$env:LOCALAPPDATA\Diff Drift\diff-drift-cli.exe" check . --baseline trust-point --md
if ($LASTEXITCODE -ge 2) { exit 2 }
```

With the `trust-point` baseline the gate keeps seeing everything since *you* last clicked **Mark reviewed**, even while the agent commits as it works.

**GitHub Actions** — gate a PR on medium+ drift vs the merge-base. The published action downloads the release CLI (checksum-verified), runs the check, and writes the report to the job summary. No Rust toolchain needed. Two requirements: a Windows runner, and `fetch-depth: 0` (the `merge-base` baseline needs `origin/main` in the checkout):

```yaml
jobs:
  drift:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: Statusnone420/Diff-Drift@v0.4.0
        with:
          baseline: merge-base
          fail-on: medium
```

Inputs: `baseline` (default `merge-base`), `fail-on` (`none`/`low`/`medium`/`high`, default `medium`), `path` (default `.`), `version` (release tag to download). Outputs: `exit-code` and `report-path`. Pick the threshold deliberately: `high` gates only High (low triage burden), `low` gates everything (strictest).

To post the report as a PR comment, give the job `pull-requests: write` permission and add:

```yaml
      - uses: actions/github-script@v7
        if: always() && github.event_name == 'pull_request'
        with:
          script: |
            const fs = require('fs');
            const report = fs.readFileSync('diff-drift-report.md', 'utf8');
            await github.rest.issues.createComment({
              owner: context.repo.owner,
              repo: context.repo.repo,
              issue_number: context.issue.number,
              body: report,
            });
```

If you'd rather pin to source, build the same binary yourself — it's the CLI the app installs:

```yaml
- run: cargo build --release --manifest-path src-tauri/Cargo.toml --bin diff-drift-cli
- name: Drift gate (medium+ fails)
  run: |
    ./src-tauri/target/release/diff-drift-cli.exe check . --baseline merge-base --md
    if ($LASTEXITCODE -ge 2) { exit 1 }
    exit 0
```

## What Diff Drift Is Not

Diff Drift is not a full static analyzer, package scanner, or proof of safety. It is a focused drift review tool for code changes that deserve a second look.
