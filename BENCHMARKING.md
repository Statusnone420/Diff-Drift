# Benchmarking

Diff Drift has two separate measurements. This page summarises both, documents how each version's numbers were reached, and points at the full methodology wiki page for the rubric details.

## Engine benchmark

A deterministic CI gate: `npm run eval:engine` runs 27 fixture cases through the real binary and asserts exact expected flags, risk counts, and exit codes. The fixture cases are real temporary git repos built from in-memory helpers (no first-read penalty). A failing case is a regression.

**Gate:** CI blocker. Current case count: 27.

Rules with at least one dedicated engine case: Hardcoded secret, Dynamic code execution, Child process execution, Disabled TLS verification, Broadened CORS, Weakened cookie flags, Loose regex pattern, Crypto downgrade, Undeclared import, Disabled guard, Removed sanitization, Permissive logging config, Dependency not in lockfile, New dependency, npm script changed, Dependency version changed, test-file hardcoded secret, structural differential cases (guard removed, regex anchors removed, try-catch removed, constant-falsy guard evasion, benign eval in string).

**Limitation:** every fixture is synthetic and small. There is no real-world corpus, so the engine benchmark makes no claim about recall or precision on production diffs. For that, use `npm run eval:fp-replay` on your own repos.

## Blind-agent scorecard

An advisory measurement: each model plays a blind reviewer over the benchmark v4 packets and scores how reliably it reaches the right trust decision from Diff Drift's output. It does not measure detection rate; it measures output clarity — whether a reviewer who sees only Diff Drift's report can make the right call.

The models are rulers; Diff Drift is what's measured. Read the per-model scores and the spread, not a pooled average.

**Current panel (benchmark v4):** Claude Opus 4.8 — 99/100, Claude Sonnet 4.6 — 99/100, Claude Haiku 4.5 — 91/100. Spread: 91–99. Three blind models, independent external validation pending.

No case is missed by every model. Every sub-100 score is a single-model slip, not a Diff Drift detection gap. A case all models miss is the signal to fix the engine or report.

## Version history

| Version | Date | Cases | Score |
| --- | --- | --- | --- |
| v1 | 2026-06-09 | 15 | 72/100 (one model; prompt-contract ambiguity collapsed benign cases to 15–20 pts) |
| v2 | 2026-06-10 | 15 | 98/100 (packet contract clarified: findings = actionable risks only, notes field added) |
| v3 | 2026-06-10 | 15 | 100/100 (severity became a scored part of the contract; three-model batch) |
| v4 | 2026-06-10 | 20 | 99/100 single-model (Opus 4.8); 91–99 panel — engine-v2 differential cases added; hardcoded-secret-in-test-file engine fix |

Every instrument change starts a new version. Old scores are never recomputed under new rules, and new answers are always regenerated from scratch. The frozen-rubric policy is described in [Eval Methodology](docs/wiki/Eval-Methodology.md).

## Reproducing the published score

```bash
npm install
npm run eval:score-agent -- eval/benchmarks/v4/answers
```

That rescores the exact recorded v4 answers (Claude Opus 4.8) through the current rubric. Expected output: 99/100. For the full multi-model panel: `npm run eval:panel`, then `npm run scorecard:capture` to regenerate the README image.

Older benchmark folders (`eval/benchmarks/v1`–`v3`) preserve their raw answers and original scorecards.

## Limitations

- **Synthetic, not real-world.** Every case is a small hand-crafted fixture. No claim is made about recall or precision on production codebases.
- **Model-only evaluators.** All current evaluators are models inside the project. The "independent external validation pending" banner is computed by the harness and stays until a human reviewer outside the project records an answer.
- **Not a comparison.** The scorecard is a product-quality signal, not a benchmark against other tools.

For the full rubric, scoring formula, semantic aliases, and compound-finding rules, see [Eval Methodology](docs/wiki/Eval-Methodology.md).
