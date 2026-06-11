# NSIS Dogfood - 2026-06-10

## Scope

This dogfood run used the installed Windows app from the NSIS installer:

```text
C:\Users\Antho\AppData\Local\Diff Drift\diff-drift.exe
```

The test repo was `D:\Diff Drift` on a throwaway dogfood branch. Scenario changes were intentionally uncommitted and were not meant to land. Raw local artifacts live under:

```text
.eval/dogfood/2026-06-10-nsis/
```

Those artifacts include screenshots, CLI JSON, exported Markdown reports, raw diffs, and the blind agent response. They should stay local/ignored; this page is the durable summary.

## Run Matrix

| Run | Scenario | Expected | Observed |
| --- | --- | --- | --- |
| R00 | Clean baseline | No drift, no flags | Passed. UI and CLI showed 0 changed files and 0 flags. |
| R01 | Formatting-only TypeScript change | 1 changed file, no risk flags | Passed. UI and CLI stayed calm with 0 flags. |
| R02 | Risky added TypeScript | High flags for secret, eval, child process, disabled TLS, broad CORS, loose regex; Medium undeclared import | Passed. UI and CLI showed 8 flags in the risky file. |
| R03 | Dependency and script drift | High dependency-not-in-lockfile; Medium script change | Passed. UI and CLI showed 10 total flags across the risky file and package.json. |
| R04 | Risky-looking test file | No new active flags if test suppression is intentional | Passed mechanically. The test file stayed at 0 flags. |
| R05 | Full packet review | A reviewer should block or investigate | Passed. A blind agent blocked the change using the exported report plus raw diff. |

## What Worked

- The installed NSIS app and CLI agreed on run-level counts and severity exits.
- The export report gave enough context for an independent reviewer to block the final packet.
- Package drift rules caught both a fake dependency without lockfile support and a new `postinstall` script.
- Test-path suppression behaved according to the current rule policy.
- Formatting-only drift did not create security noise.

## Product Findings

1. Test suppression needs clearer review semantics.

   The blind reviewer flagged `tests/dogfood/spawn.test.ts` because the raw diff exported an `execSync(command)` helper and an AWS-shaped fixture string. Diff Drift intentionally suppressed child-process and secret rules in test-like paths, but the exported packet did not make that policy obvious. Either the report should clearly state that test-like files were suppressed, or the rule policy should be revisited for exported helpers that accept caller-controlled command strings.

   **Resolved (benchmark v4, 2026-06-10):** the secret half of this was revisited — the hardcoded-secret rule no longer suppresses in test files (a real key in a fixture is still a leak; the AWS/OpenAI/PEM markers are specific and drift-scoped enough to stay low-noise). The child-process suppression in test paths stays. See [Eval Methodology](Eval-Methodology.md#benchmark-versions).

2. Export packet generation must include untracked files.

   The first raw diff helper used plain `git diff`, which omitted untracked scenario files. The final packet was regenerated with untracked files included. Any dogfood/export workflow that feeds an agent should treat untracked files as first-class review input.

3. Ignored eval artifacts can confuse the app if they appear in the watched repo.

   A local automation helper under `.eval/dogfood/.../dogfood-runner.mjs` appeared in the installed app with flags even though `.eval/` is ignored and the sidebar count did not clearly line up with the flagged file. Dogfood automation should prefer temp directories outside the repo, and the app should be checked for consistent ignored-file behavior.

4. Formatting-only exports still show unchanged structural cards.

   R01 was correct overall, but the report/UI can feel busier than necessary when a file is classified as formatting-only. This is not a correctness issue, but it affects the "calm when safe" product goal.

5. Desktop automation fallback is useful.

   Computer Use failed to bootstrap in this environment, but WebView2/CDP automation was enough to drive the installed app, collect screenshots, run exports, and compare against CLI output. This is a useful fallback path for future dogfood loops.

## CI Note

The GitHub Actions run for commit `dc7cd51` failed in `Rust (test + clippy + eval)` during `actions/setup-node@v6`, before `npm ci`, Rust tests, engine eval, or clippy ran. The `Web (tsc + vite + Playwright)` job passed. Locally, `npm run build` failed only while the intentional dogfood fixtures were present, then passed after those throwaway changes were stashed.

## Recovery Notes

The throwaway dogfood worktree changes were preserved in a stash named:

```text
dogfood throwaway drift fixtures 2026-06-10
```

The final local packet remains available at:

```text
.eval/dogfood/2026-06-10-nsis/tracker.md
.eval/dogfood/2026-06-10-nsis/R05/R05-raw-git-diff.patch
.eval/dogfood/2026-06-10-nsis/R05/R05-blind-agent-response.json
```
