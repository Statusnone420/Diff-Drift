# Engine v2 scorecard

Measurement battery for the `engine-v2` branch (structural + differential rules). Criterion
numbers are absolute medians from one machine (Win11); the deltas Criterion prints against
older local runs are ignored.

## Battery (baseline → final)

| Metric | Baseline (`main`) | engine-v2 |
| --- | --- | --- |
| `cargo test` | 104 | 126 |
| `cargo clippy --all-targets -D warnings` | clean | clean |
| `npm run eval:engine` | 15/15 | 20/20 |
| `npm run test:unit` / `test:e2e:web` | 54 / 2 | 54 / 2 |
| `npm run eval:fp-replay` (this repo vs `main`) | 0 flags | 0 flags |
| bench `analyze_all/25_drifted_files` | 21.4 ms | 26.3 ms (+22.9%) |

## What changed

- **Structural matching** (`structural.rs`): rules re-parse each changed node with tree-sitter
  and match real syntax, with a text-fallback when a snippet won't parse. `eval`, `new
  Function`, `rejectUnauthorized: false`, broadened CORS, and constant-falsy guards (`if (0)`,
  `if (null)`) now match syntax — patterns in strings/comments don't flag, reformatting can't
  evade, and the `if (0)` bypass is closed.
- **Differential rules** (before-vs-after, the diff-native part): `loose-regex` compares regex
  literals by set difference and names what weakened (catch-all / anchors dropped / length
  bound removed); `guard-removed` flags a call that lost its `if` guard; `removed-try-catch`
  flags a `try` removed from surviving calls; `removed-sanitize`/`verify-to-decode` compare
  real callee names so comments can't mask or fake them.

## Red-team + review (each finding → failing test → fix → regression test)

- **3 adversarial rounds:** R1 found 8 (incl. the `if (x){f()}` → `if (!x) return; f()` guard
  refactor false positive — the one that would have drawn the loudest roast); R2 found 3
  self-inflicted regressions from R1's fixes; R3 clean.
- **Final review:** README case count corrected (20); `verify-to-decode` no longer treats
  `signIn`/`signOut`/`signal` as signing; `loose-regex` uses set difference so reordered
  literals can't mispair into a false flag.

## Performance trade

+22.9% on the 25-file synthetic batch is ~0.2 ms per changed file — imperceptible in the
debounced single-file watcher path. Accepted rather than cripple detection to hit the
self-imposed 15% estimate. A parse-once-per-node shared tree (rules currently re-run queries
against a memoized parse) would recover most of it without touching detection — logged for v0.4.

## Blind-agent scorecard

The deterministic `eval:engine` gate covers all 20 cases through the real binary (20/20). The
advisory blind-agent scorecard (`eval:score-agent`) was re-run blind against this engine as
**benchmark v4**: all 20 packets regenerated from the engine-v2 binary, fresh model answers
produced from packet-only context, the rubric and prompt frozen and byte-identical across the run.
It scored **94/100** with 100% per-rule recall — the lost points are reviewer-vs-rubric decision
disagreements on three cases, not missed detections. Per-case gap analysis in
[Eval Methodology](wiki/Eval-Methodology.md#benchmark-versions).

## Known limits (out of lane by design)

These need dataflow/symbol analysis Diff Drift deliberately doesn't do — documented, not chased.

| Evasion | Why out of lane |
| --- | --- |
| `const e = eval; e(x)` (aliased sink) | needs local dataflow |
| `import { decode as parseToken }` then `parseToken(t)` | needs import-binding resolution |
| CORS `origin` callback that returns `"*"` | needs to evaluate a return value |
| `if (1 === 2)` always-false guards | needs constant-folding |
| try/catch removed but an unrelated `.catch()` exists | `.catch()` suppresses — accepted false negative |
| `validateForm()` renamed to `checkForm()` | rename vs removal needs symbol tracking; Low, dismissable |
| `/^[A-Z]+$/m` multiline anchors | needs flag-aware regex semantics; rare |
| `guard-removed` across deeply nested `if`s | standalone re-parse may under-count — errs to silence |
