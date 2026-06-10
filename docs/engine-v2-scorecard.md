# Engine v2 scorecard

Measurement battery for the `engine-v2` branch. Every phase ends with the full battery; a
regression in any metric blocks the next phase. Criterion numbers are absolute medians from
this machine (Win11, same session) — the deltas criterion prints against older local runs
are ignored.

## Battery results by phase

| Metric | Baseline (Phase 0) | Phase 1 (structural core) |
| --- | --- | --- |
| `cargo test` | 104 passed, 0 failed | 112 passed, 0 failed |
| `cargo clippy --all-targets -D warnings` | clean | clean |
| `npm run eval:engine` | 15/15 | 15/15 |
| `npm run test:unit` | 54 passed | 54 passed |
| `npm run test:e2e:web` | 2 passed (incl. axe) | 2 passed |
| `npm run eval:fp-replay` (this repo vs `main`) | 0 changed files, 0 flags | 9 changed files, 0 flags |
| bench `parse_file/100_line_ts` | 408 µs | 402 µs |
| bench `diff_nodes/representative_ts` | 33.1 µs | 33.4 µs |
| bench `analyze_all/25_drifted_files` | 21.4 ms | 22.5 ms (+5.1%) |

Phase 1 adds `structural.rs` (tree-sitter query matching with cached compiled queries,
text-fallback contract) and ports `eval-call` to it: `eval?.()`, `window.eval`,
`globalThis.eval` now caught; `eval(` inside strings/comments no longer flags.

| Metric | Phase 2 (rule ports) |
| --- | --- |
| `cargo test` | 114 passed, 0 failed |
| `cargo clippy` | clean |
| `npm run eval:engine` | 15/15 |
| `npm run test:unit` / `test:e2e:web` | 54 / 2 passed |
| fp-replay | 9 changed files, 0 flags |
| bench `analyze_all` | 24.4 ms (+14.0% vs baseline) |

Phase 2 ports `fn-constructor`, `tls-reject-false`, `broadened-cors`, `removed-if-guard`,
and `verify-to-decode` to structural matching. New catches: quoted object keys
(`{"rejectUnauthorized": false}`, `{"origin": "*"}`), constant-falsy guards (`if (0)`,
`if (null)`, `if (undefined)`), and crypto downgrades masked by `verify` surviving only in
a comment. New silences: any of these patterns inside strings or comments.

Performance note: the first bench run after the ports hit +21.5% (each structural rule
re-parsed the node snippets independently) — over the gate. A per-thread memo of the most
recent snippet parses (one parse per snippet per node, shared across rules) brought it
back to +14.0%. The residual is the real cost of running live tree-sitter queries.

| Metric | Phase 3 (differential family) |
| --- | --- |
| `cargo test` | 119 passed, 0 failed |
| `cargo clippy` | clean |
| `npm run eval:engine` | 20/20 (5 new cases) |
| `npm run test:unit` / `test:e2e:web` | 54 / 2 passed |
| fp-replay | 14 changed files, 0 flags |
| bench `analyze_all` | 24.3 ms (+13.6% vs baseline) |

Phase 3 adds the differential rules — true before-vs-after comparisons only a diff-native
engine can express:

- **Loosened validation regex** (`loose-regex`, differential path): paired regex literals
  compared for weakening — widened to a catch-all, anchors dropped, or length bound
  removed — with the description naming exactly what weakened. Tightening stays quiet.
- **Guard removed** (`guard-removed`): a call whose every before call-site sat inside an
  `if` consequence now runs unconditionally.
- **Error handling removed** (`removed-try-catch`): a `try` that wrapped calls is gone
  while the calls remain.
- **Removed sanitization** upgraded to structural callee comparison: `sanitize` surviving
  only in a comment no longer masks the removal; wrapper-stripping
  (`save(escapeSql(x))` → `save(x)`) is caught.

Eval-driven engine fix: the new `if (0)` case failed through the real binary because
exported functions aren't split into body-level child nodes and `removed-if-guard` only
looked at `IfStatement` nodes. The structural matcher made it safe to lift that
restriction to any Modified node — the case now passes, and the payments-api fixture's
flag set is unchanged.

Note: fp-replay measures this branch against `main`; at Phase 0 the branch has no engine
changes yet, so 0/0 is expected. The meaningful fp-replay reads come at later phases once
rules change — the gate is that benign drift in this repo's own history stays quiet.

## Known out-of-lane evasions

Populated during the red-team loop (Phase 4): in-scope findings become tests and eval
cases; evasions that would require taint tracking, call-graph, or cross-file analysis are
recorded here as documented limits instead.

| Evasion | Why out of lane |
| --- | --- |
| _(pending Phase 4)_ | |
