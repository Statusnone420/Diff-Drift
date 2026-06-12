# Self-Review — 2026-06-11 (v0.4.0)

## Scope

Diff Drift v0.4.0 was pointed at its own working tree. The baseline was the trust point at `7ca3fe9` (v0.3.1), so the drift spans two releases (v0.3.2 + v0.4.0) plus uncommitted trailer and icon work: **78 changed files**. The raw export (`diff-drift-Diff Drift.md`) stays local and untracked; this page is the durable summary.

The goal was not to find bugs in the repo — it was to ask whether the tool produces useful data on a large, real diff, and to separate genuine signal from the noise this particular repo creates by carrying a planted-secret benchmark corpus.

## Headline

- **78 changed files** → 34 analyzed (source languages + package.json), 44 other (docs, icons, lockfiles, workflows).
- **201 AST drift nodes** — every changed function, import, and script. This is the review surface, not a problem count.
- **8 active risk flags** (+2 dismissed).
- On **193 of the 201** drift nodes the engine stayed silent: the whole Rust engine rewrite (`parse.rs`, `diff.rs`, `watcher.rs`), the five new structural-drift fixtures, the React tweaks, the new CLI binary, a 13-node CLI test file. No false alarms on benign product code.

## The 8 flags

| Type | File | Verdict |
| --- | --- | --- |
| Hardcoded secret | `eval/cases/python-hardcoded-secret.case.mjs` | Noise — planted marker in the benchmark corpus |
| Hardcoded secret | `eval/cases/swift-hardcoded-secret.case.mjs` | Noise — planted marker in the benchmark corpus |
| Hardcoded secret | `src-tauri/src/rules.rs` (tests) | Noise — planted marker in rule unit tests |
| Hardcoded secret | `src-tauri/src/session.rs` (tests) | Noise — planted marker in unit tests |
| Child process execution | `scripts/capture-trailer.mjs` | Correct + benign — spawns ffmpeg for the trailer |
| npm script changed | `package.json` (`demo:trailer`) | Correct + benign — new script surface |
| Undeclared import | `scripts/fix-icon-alpha.mjs` (`module`) | False positive — see finding 1 |
| Undeclared import | `scripts/verify-icon-alpha.mjs` (`module`) | False positive — see finding 1 |

## What the data says

- **The four "noise" flags are all one rule doing its job.** The hardcoded-secret rule is the only rule that deliberately does not suppress in test paths — a real key pasted into a fixture is still a leak (see [Eval Methodology](Eval-Methodology.md), benchmark v4). It is correct everywhere; it only reads as noise here because this repo's test corpus is intentionally full of fake `AKIA…` keys. Outside a security-tool repo, those four flags do not occur.
- **The two correct flags are exactly the review-worthy changes.** A brand-new subprocess spawn and a brand-new npm script are the two things a human reviewing an agent's PR would want surfaced. Both are benign here, and both are the product working as intended — flags are review prompts, not verdicts.
- **Precision on genuine product drift:** after setting aside the corpus self-flags, the tool surfaced 2 useful prompts and produced 2 false positives, both from a single missing-builtin bug. With finding 1 fixed, a re-run of this diff drops to 6 flags (4 corpus + 2 correct) and 0 false positives on real product code.

## Findings (both fixed this session)

1. **`module` was missing from the Undeclared-import built-in allowlist.** `import { createRequire } from 'module'` was flagged as an undeclared npm package (twice), which contradicts the rule's own documented behavior — [Rule Reference](Rule-Reference.md) states it ignores Node built-ins. Fixed in `is_node_builtin` (`rules.rs`) by completing the Node built-in allowlist — `module`, `http2`, `inspector`, `perf_hooks`, and others were missing — with the regression test extended. The import node still appears in the drift; only the incorrect risk badge is gone.

2. **The export report had no cap on per-node diff bodies.** Two flags landed inside large, refactored test modules, and the report rendered each module's entire body (before and after) as diff context around a one-line secret match. Those two flags produced roughly 4,600 of the export's 4,859 lines. Fixed in `report.rs`: each side of a flag's diff is now capped (20 lines) with the remainder summarized, and a test covers it. The same renderer backs `diff-drift check --md`, so the CLI benefits too.

## What worked

- The "quiet when safe" goal held across a two-release diff: 193 of 201 nodes drew no flag.
- The two real review prompts (subprocess spawn, new npm script) landed cleanly.
- The dismiss mechanism and the trust-point baseline behaved as designed.
- Dogfooding surfaced two real bugs in one sitting — a missing built-in and an unbounded report body.
