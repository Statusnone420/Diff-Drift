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
- **Analyzed files**: changed TS/TSX/JS/JSX files Diff Drift parsed, plus `package.json` when its dependencies or scripts drifted. Files over 2 MB are not parsed — they stay in the list with the summary "Skipped — file too large to analyze" so the limit is visible. Review giant generated bundles by other means.
- **Flags**: active heuristic findings. Dismissed flags stay in the session but do not count as active.
- **Reviewed**: changed nodes you've marked reviewed vs the total ( shown per file as `n/m` and drift-wide in the toolbar).
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

The CLI is not a separate install — it is the same `diff-drift.exe` the app runs, invoked from a terminal with the `check` subcommand. The Windows installer puts it at `%LOCALAPPDATA%\Diff Drift\diff-drift.exe` by default (per-user install). Since v0.3.2 each release also carries the bare `diff-drift.exe` as a downloadable asset (verify it against `SHA256SUMS.txt`), so scripts and CI can use the CLI without the installer. In a development checkout it is `src-tauri\target\debug\diff-drift.exe` after a build.

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

## CI and Hook Recipes

The contract that makes these safe to wire up: the CLI is **read-only** (never writes triage state), dismissed flags don't count, and an explicit `--baseline` that can't resolve exits `64` loudly instead of silently falling back to `HEAD`. Exit codes: `0` none, `1` low, `2` medium, `3` high.

**Pre-commit hook** (`.git/hooks/pre-commit`) — block commits that introduce high-severity drift:

```bash
#!/bin/sh
diff-drift check . --json > /dev/null
code=$?
if [ "$code" -ge 3 ]; then
  echo "diff-drift: high-severity drift — review in the app or run: diff-drift check . --md"
  exit 1
fi
```

**Agent hook** (e.g. a Claude Code Stop/PostToolUse hook) — make the agent's own loop fail until drift is reviewed:

```bash
#!/bin/sh
diff-drift check . --baseline trust-point --md
code=$?
if [ "$code" -ge 2 ]; then exit 2; fi  # surface medium+ drift back to the agent loop
```

With the `trust-point` baseline the gate keeps seeing everything since *you* last clicked **Mark reviewed**, even while the agent commits as it works.

A Windows caveat that matters for hooks: the installed `diff-drift.exe` is a GUI-subsystem binary (so the app opens without a console window), which means **PowerShell and cmd start it without waiting and never see its exit code** — `$LASTEXITCODE` stays unset. `sh`/`bash` hooks and Node-based runners wait correctly, so prefer those. If the hook must be PowerShell, wait explicitly:

```powershell
$p = Start-Process "$env:LOCALAPPDATA\Diff Drift\diff-drift.exe" `
  -ArgumentList 'check', '.', '--baseline', 'trust-point', '--md' `
  -NoNewWindow -Wait -PassThru -RedirectStandardOutput drift-report.md
Get-Content drift-report.md
if ($p.ExitCode -ge 2) { exit 2 }
```

**GitHub Actions** — gate a PR on medium+ drift vs the merge-base. The published action downloads the release CLI (checksum-verified), runs the check, and writes the report to the job summary. No Rust toolchain needed. Two requirements: a Windows runner, and `fetch-depth: 0` (the `merge-base` baseline needs `origin/main` in the checkout):

```yaml
jobs:
  drift:
    runs-on: windows-latest
    steps:
      - uses: actions/checkout@v4
        with:
          fetch-depth: 0
      - uses: Statusnone420/Diff-Drift@v0.3.2
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

If you'd rather pin to source, build the same binary yourself — it's the binary the app installs. Use a `bash` step to run it: the release build is a GUI-subsystem binary, and PowerShell/cmd don't wait for those or see their exit codes (see the agent-hook caveat above):

```yaml
- run: cargo build --release --manifest-path src-tauri/Cargo.toml --bin diff-drift
- name: Drift gate (medium+ fails)
  shell: bash
  run: |
    code=0
    ./src-tauri/target/release/diff-drift.exe check . --baseline merge-base --md || code=$?
    if [ "$code" -ge 2 ]; then exit 1; fi
```

## What Diff Drift Is Not

Diff Drift is not a full static analyzer, package scanner, or proof of safety. It is a focused drift review tool for code changes that deserve a second look.
