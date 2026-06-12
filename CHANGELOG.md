# Changelog

All notable changes to Diff Drift are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/); versions follow [SemVer](https://semver.org/) with 0.x meaning the API and data contract may still change between minor versions.

## [Unreleased]

### Added

- Risk-flag reports now show the exact line that triggered a flag. A hardcoded-secret flag on a large node used to render only the head of the body, so the secret could be truncated out of the export; the report and `diff-drift check --md` now include a **Match:** line with the offending source line wherever it sits in the node.

## [0.4.1] — 2026-06-11

### Fixed

- The "Undeclared import" rule no longer flags Node built-ins it failed to recognize — `module`, `http2`, `inspector`, `perf_hooks`, and others. `import { createRequire } from "module"` was reported as an undeclared package, which contradicted the rule's documented handling of Node built-ins; the import still shows as drift, only the false flag is gone.
- Exported reports now cap each flag's diff body. A flag on a large node — for example a whole refactored test module flagged for a one-line secret — rendered the entire body before and after, inflating one real export to about 4,800 lines. Each side now shows a bounded head with the remainder summarized; the same renderer backs `diff-drift check --md`.

## [0.4.0] — 2026-06-11

### Added

- Structural drift review for seven more languages: Rust, Go, Python, Java, C#, Kotlin, and Swift. Changed functions, methods, types, and other top-level declarations in these files now show as added, modified, and removed AST nodes with the same node-by-node review flow as TypeScript and JavaScript. The heuristic security rules stay scoped to JS/TS and `package.json` drift, where they are tuned; files in the new languages get structural review plus the hardcoded-secret check, and the docs say so plainly.
- "Other changed files": paths git reports as changed but Diff Drift did not analyze — unsupported file types, or a `package.json` with no dependency or script drift — are listed by path in the sidebar and the report, so no part of the drift is invisible. Their content is part of the review fingerprint, so editing one un-pins **Mark reviewed**, and the live watcher refreshes the list when such a file changes.
- Seven new engine benchmark cases covering the new languages (27 total, still a CI blocker).
- [BENCHMARKING.md](BENCHMARKING.md): both measurements (engine benchmark and blind-agent scorecard) on one page, with version history, reproduction steps, and limitations. The README evaluation section now summarizes and links instead of explaining everything inline.
- A trailer capture script (`npm run demo:trailer`) producing an MP4 and thumbnail for the README; like the demo video, the output is not committed.

### Fixed

- Overloaded Java methods no longer cross-match in the diff: when two declarations share a name, nodes match by signature.
- A Python change that only touches a decorator now shows as drift on the decorated function.
- App icons and the scorecard PNG carry a real alpha channel instead of an opaque background.

## [0.3.2] — 2026-06-11

### Added

- A dedicated console CLI, `diff-drift-cli.exe`: the same read-only `check` as the app binary, built as a console program so PowerShell, cmd, and sh all wait for it and see its severity exit code. The installers place it next to the app, and it is a release asset covered by `SHA256SUMS.txt`, so CI jobs and scripts can use the headless check without installing the app.
- A GitHub Action (`uses: Statusnone420/Diff-Drift@v0.3.2`): downloads the release CLI, verifies its checksum, runs `diff-drift check`, writes the Markdown report to the job summary, and fails the job at a configurable severity threshold (`fail-on: none|low|medium|high`). Windows runners only; needs `fetch-depth: 0` for the merge-base baseline. A smoke workflow (`action-smoke.yml`) exercises the action in CI.
- Demo video capture (`npm run demo:video`): drives the demo storyboard through the browser UI at watchable pacing with a title and closing card and produces an MP4 (needs a system ffmpeg; without one it keeps the WebM). The video is a release asset, not a repo file.

### Changed

- The GitHub Actions recipe in the User Guide uses the published action instead of building the CLI from source; the source build remains documented as an alternative.

### Fixed

- `diff-drift.exe check` from PowerShell or cmd never set `$LASTEXITCODE`/`%ERRORLEVEL%`: the release app binary is GUI-subsystem (no console window for the app), and those shells start GUI programs without waiting. Scripts should use the new console `diff-drift-cli.exe` instead, which every shell waits for; the User Guide recipes and the GitHub Action now do.
- `check` no longer panics with "The pipe is being closed (os error 232)" when its stdout reader goes away mid-print (for example `diff-drift-cli check | head`). The report write is abandoned quietly; the exit code still carries the result.

## [0.3.1] — 2026-06-11

### Fixed

- The live watcher now respects `.gitignore`, so ignored build output no longer appears as drift after filesystem events. Tracked files inside ignored directories still count when they differ from the selected baseline.
- Switching baselines now prunes stale per-node review hashes, keeping `repo-state.json` from accumulating dead reviewed-node entries.

## [0.3.0] — 2026-06-10

### Added

- Engine v2 — structural rule matching over tree-sitter queries. `eval`, `new Function`, `rejectUnauthorized: false`, broadened CORS (incl. `origin: ['*']`), and constant-falsy guards match real syntax, so patterns in strings/comments don't flag and reformatting can't evade. Closes the `if (0)` guard bypass.
- Differential rules a snapshot scanner can't express: a regex that lost its anchors or length bound (not just a widening to `/.*/`), a call that lost its `if` guard, a `try/catch` removed from surviving calls. Crypto-downgrade and removed-sanitization compare real callee names, so a comment can't mask or fake a removal.
- Five engine eval cases for the new behaviors; engine eval now 20/20. Adversarially hardened over three red-team rounds — receipts and known limits in `docs/engine-v2-scorecard.md`.
- Hardcoded secrets are now flagged in test files too. The secret rule used to suppress in test/fixture paths like every other rule; a real key pasted into a fixture is still a leak, and the AWS/OpenAI/PEM markers are specific and drift-scoped enough to stay low-noise. The noisier rules (child_process, eval, TLS) still suppress in test paths. Surfaced by the v4 benchmark and the earlier NSIS dogfood note.
- Blind-agent benchmark refreshed to **v4** against the engine-v2 binary (v3's 100/100 was scored on pre-engine-v2 packets). Single-model **Claude Opus 4.8 scores 99/100** with 100% per-rule recall and engine eval 20/20; the rubric and packet prompt were frozen and never tuned. A new **multi-model evaluator panel** (`npm run eval:panel`) has Opus 4.8, Sonnet 4.6, and Haiku 4.5 each play the blind reviewer over the same packets — a **91–99 spread**, reported per-model (not pooled), rendered as a single benchmark leaderboard. No case is missed by every model, so the gaps are reviewer variance, not detection failures. Cross-vendor models drop into `eval/benchmarks/v4/panel/<model>/` and are auto-discovered. Per-case analysis and the panel in [Eval Methodology](docs/wiki/Eval-Methodology.md#multi-model-panel).
- Public feedback funnel: README first screen rewritten around trust-point review, feedback issue templates (tried it, install problem, noisy flag, confusing output), release notes for the public feedback release (`docs/releases/v0.3.0-public-feedback.md`), a demo storyboard (`docs/demo/demo-script.md`), an automated demo GIF capture script (`npm run demo:capture`), and a feedback triage tracker (`docs/feedback/feedback-triage.md`).

### Fixed

- Clarified the no-flags/no-review state: zero heuristic flags no longer reads as "nothing to review" when changed AST/package nodes still need human review, and the sidebar now calls out changed files outside the analyzed-files view.
- Quieted the drift-listener `console.error` path in the renderer.

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
