# Changelog

All notable changes to Diff Drift are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [SemVer](https://semver.org/) with 0.x meaning the API and data contract may still change between minor versions.

## [Unreleased]

### Added

- Evaluation harness: deterministic engine benchmark (`npm run eval:engine`, a CI gate), blind-review packet generation, and advisory blind-agent scorecards with evaluator metadata, precision, and per-rule recall.
- Eval cases covering TSX, JSX, and `.mjs` drift, a noisy benign refactor, and oversized-file skip behavior.
- Release workflow: tag-triggered NSIS installer build with SHA256 checksums, CycloneDX SBOMs, and a draft GitHub Release. Code signing is scaffolded but not yet configured.
- Oversized-file guard: files larger than 2 MB are skipped before parsing and surfaced as "Skipped — file too large to analyze" instead of being parsed or silently ignored.
- Parsed lockfile matching for `yarn.lock` and `pnpm-lock.yaml` (previously a loose substring check that could let a hallucinated package ride on a similarly named real one).
- FP-replay script (`npm run eval:fp-replay`) to measure flag noise over your own repos and commit ranges.
- Trust documentation: `SECURITY.md`, threat model, privacy/data-flow page, eval methodology, A/B study design, FAQ, and this changelog.

### Changed

- README: problem-first lead, tool comparison table, install-from-release section, CI/hook examples, and an honestly captioned eval scorecard.

## [0.2.1] — 2026-06-09

### Added

- Strict CLI baselines: an explicit `--baseline` that cannot resolve exits `64` with the cause on stderr — never a silent `HEAD` fallback. `--help` exits `0`.
- Content-pinned dismissals: each dismissal stores a hash of the flagged node; meaningful changes to the node resurface the flag.
- Watcher full-scans when a root lockfile (`package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`) changes, so phantom-dependency flags track lockfile content.
- CLI subprocess integration tests and vitest + Testing Library component tests.
- CI: native Tauri build smoke job, advisory native E2E, and dependency audit gates (`cargo audit`, `npm audit`).

### Changed

- Binary renamed to `diff-drift` to match the product and docs.
- Plain-language baseline picker ("Review changes since") and baseline-honest copy throughout the UI and docs.

## [0.2.0] — 2026-06-09

### Added

- Trust-point baseline: "Mark reviewed" pins the current commit; drift stays visible across agent commits until re-approved. Baselines: `HEAD`, trust point, merge-base, or any rev.
- Review at scale: per-node review state, content-tracked, with progress counts.
- JavaScript/JSX analysis alongside TypeScript/TSX.
- `package.json` dependency and script drift, including the lockfile-can't-vouch rule for hallucinated packages.
- Headless `diff-drift check [path] [--json|--md] [--baseline <spec>]` with severity exit codes (0 none, 1 low, 2 medium, 3 high).
- CI pipeline, git-layer tests, `SCHEMA_VERSION` on the data contract, and a scale fixture.

## [0.1.1] — 2026-06-09

### Added

- Triage export (Markdown/JSON reports) and NSIS installer packaging.
- Stable bundle identifier (`io.github.statusnone420.diffdrift`).

### Fixed

- TSX parsing uses the TSX grammar (JSX was a parse error under plain TypeScript).
- All git changes are counted; clearer empty state.

## [0.1.0] — 2026-06-09

### Added

- Initial release: native Windows 11 AST drift inspector (Tauri 2 + React/TypeScript + Rust), heuristic security flags, flag dismissal, and per-repo session state.
