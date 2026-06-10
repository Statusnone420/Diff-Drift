# Engine v2 scorecard

Measurement battery for the `engine-v2` branch. Every phase ends with the full battery; a
regression in any metric blocks the next phase. Criterion numbers are absolute medians from
this machine (Win11, same session) — the deltas criterion prints against older local runs
are ignored.

## Battery results by phase

| Metric | Baseline (Phase 0) |
| --- | --- |
| `cargo test` | 104 passed, 0 failed |
| `cargo clippy --all-targets -D warnings` | clean |
| `npm run eval:engine` | 15/15 |
| `npm run test:unit` | 54 passed |
| `npm run test:e2e:web` | 2 passed (incl. axe) |
| `npm run eval:fp-replay` (this repo vs `main`) | 0 changed files, 0 flags |
| bench `parse_file/100_line_ts` | 408 µs |
| bench `diff_nodes/representative_ts` | 33.1 µs |
| bench `analyze_all/25_drifted_files` | 21.4 ms |

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
