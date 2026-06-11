# Changelog

All notable changes to Diff Drift are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [SemVer](https://semver.org/) with 0.x meaning the API and data contract may still change between minor versions.

## [Unreleased]

### Added

- Engine v2 — structural rule matching over tree-sitter queries. `eval`, `new Function`, `rejectUnauthorized: false`, broadened CORS (incl. `origin: ['*']`), and constant-falsy guards match real syntax, so patterns in strings/comments don't flag and reformatting can't evade. Closes the `if (0)` guard bypass.
- Differential rules a snapshot scanner can't express: a regex that lost its anchors or length bound (not just a widening to `/.*/`), a call that lost its `if` guard, a `try/catch` removed from surviving calls. Crypto-downgrade and removed-sanitization compare real callee names, so a comment can't mask or fake a removal.
- Five engine eval cases for the new behaviors; engine eval now 20/20. Adversarially hardened over three red-team rounds — receipts and known limits in `docs/engine-v2-scorecard.md`.

### Fixed

- Quieted the drift-listener `console.error` path in the renderer.

## [0.3.0] — 2026-06-10

### Added

- Public feedback funnel: README first screen rewritten around trust-point review, feedback issue templates (tried it, install problem, noisy flag, confusing output), release notes for the public feedback release (`docs/releases/v0.3.0-public-feedback.md`), a demo storyboard (`docs/demo/demo-script.md`), an automated demo GIF capture script (`npm run demo:capture`), and a feedback triage tracker (`docs/feedback/feedback-triage.md`).

## [0.2.1] — 2026-06-10

### Added

- Evaluation harness: a CI-gated engine benchmark (`npm run eval:engine`), blind-review packets, and advisory scorecards with evaluator attribution, precision, and per-rule recall.
- Eval cases for TSX, JSX, and `.mjs` drift, a noisy benign refactor, and the oversized-file skip.
- Tag-triggered release workflow producing NSIS and MSI installers, SHA256 checksums, CycloneDX SBOMs, and a draft GitHub Release (signing scaffolded, not yet configured).
- Oversized-file guard: files over 2 MB are skipped before any content is read and shown as "Skipped — file too large to analyze".
- Skipped-file counts in session data and reports, so "0 active flags" can't be mistaken for "fully analyzed".
- Exact entry-name parsing for `yarn.lock` and `pnpm-lock.yaml` (the old substring check let similar names vouch for each other).
- FP-replay script (`npm run eval:fp-replay`) to measure flag noise on your own repos; it builds the CLI if missing.
- Trust documentation: `SECURITY.md`, threat model, privacy/data-flow page, eval methodology, A/B study design, FAQ, and this changelog.
- Strict CLI baselines: an explicit `--baseline` that cannot resolve exits `64` with the cause on stderr — never a silent `HEAD` fallback. `--help` exits `0`.
- Content-pinned dismissals: each dismissal stores a hash of the flagged node; meaningful changes to the node resurface the flag.
- Watcher full-scans when a root lockfile (`package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`) changes, so phantom-dependency flags track lockfile content.
- CLI subprocess integration tests and vitest + Testing Library component tests.
- CI: native Tauri build smoke job, advisory native E2E, and dependency audit gates (`cargo audit`, `npm audit`).

### Fixed

- Changes to skipped oversized files now revoke a "Mark reviewed" approval; previously they slipped past the drift fingerprint.
- Skipped files get their own center panel instead of reading as "formatting only".

### Changed

- README rewritten: problem-first lead, tool comparison, install-from-release, CI/hook examples, honestly captioned scorecard.
- `schemaVersion` bumped to 3 for the new skipped-file fields.
- Blind-answer validation enforces the full finding shape (severity, file path, risk type, evidence).
- The published benchmark's raw answers and scorecards are committed under `eval/benchmarks/v2/` and `eval/benchmarks/v3/` so scores are reproducible from a fresh clone.
- Blind-agent benchmark moved through v2 and v3; v1's 72/100 is preserved in the methodology history, v2 scored 98/100, and v3 scores 100/100 with severity included in scoring.
- The release workflow refuses tags that don't match all three version fields.
- Privacy/security docs now state the verifiable claim: no HTTP client in the compiled Windows binary (the framework lists one for other platforms).
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
