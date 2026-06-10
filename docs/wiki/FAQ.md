# FAQ

## How much triage work should I expect?

Flags are review prompts, not verdicts. On a typical agent change touching a handful of files, expect a few flags; a security-sensitive refactor will raise more. The intended loop is fast: read the flag, look at the node's before/after, then either dismiss it (reviewed, not actionable) or fix the code. Dismissals are remembered per repo, so re-runs only surface new or changed drift.

To measure noise on *your* codebase instead of guessing, run the [FP-replay script](Eval-Methodology.md#fp-replay-measuring-noise-on-your-own-repos) over a few recent commit ranges.

## A flag is wrong. What do I do?

Dismiss it. That's a designed outcome, not a failure: rules like "Removed sanitization" deliberately over-trigger on renames and refactors because missing a real removal is worse. The dismissal is pinned to the flagged node's content — if that code changes meaningfully later, the flag comes back on its own. Dismissed flags stay visible (struck through) so a reviewer can audit what was waved through.

If a rule is consistently noisy in a way the [Rule Reference](Rule-Reference.md) caveats don't acknowledge, open a [discussion](https://github.com/Statusnone420/Diff-Drift/discussions) with the pattern.

## Why was my file skipped or not shown as AST drift?

Three reasons a changed file isn't rendered as nodes:

- **Not a supported language.** Only `.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, `.cjs` are parsed (plus `package.json` dependency drift). Other changed files count toward git drift but aren't analyzed.
- **Type definitions.** `.d.ts` files are intentionally excluded.
- **Too large.** Files over 2 MB are skipped before parsing and appear in the file list with the summary "Skipped — file too large to analyze". This is a denial-of-service guard for giant generated bundles; review those by other means.

## Can I gate CI or an agent hook on Diff Drift?

Yes — that's what the read-only CLI is for. `diff-drift check` exits with the highest active severity (0 none, 1 low, 2 medium, 3 high; 64 usage error), honors your dismissals, and never writes state. See [CI and Hook Recipes](User-Guide.md#ci-and-hook-recipes) for copy-paste examples.

## Does anything leave my machine?

No. No telemetry, no model calls, no upload, no update pings, and no HTTP client in the dependency tree. [Privacy and Data Flow](Privacy-and-Data-Flow.md) shows how to verify that yourself instead of taking the claim on trust.

## Is a clean run proof the change is safe?

No. Diff Drift is a deterministic second pass that makes structural drift reviewable and flags known-risky patterns. It is not a full static analyzer, and the [Threat Model](Threat-Model.md) explicitly does not claim detection of a deliberately evasive attacker. Use it to focus human review, alongside — not instead of — your normal PR review and any SAST you already run.

## Why is the blind-agent score not a guarantee?

The scorecard is advisory and measured on a small synthetic suite — see [Eval Methodology](Eval-Methodology.md) for the rubric, the case list, and the limitations (including which validations are still pending). The CI gate is the deterministic engine benchmark, not the blind-agent score.

## Where is my triage state stored?

`%APPDATA%\io.github.statusnone420.diffdrift\repo-state.json`, keyed by repo path. Delete it to reset all triage for all repos. It contains flag IDs, hashes, and your baseline choice — not source code.
