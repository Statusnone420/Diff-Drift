# AGENTS.md

This file is for AI agents and contributors working in this repo.

`CLAUDE.md` and `GEMINI.md` intentionally point here. Keep this file as the canonical repo-level instruction source.

## Product Goal

Diff Drift is the deterministic verifier in the AI coding loop: a local desktop reviewer for the drift agents leave behind. It compares a git working tree against a selectable baseline (`HEAD`, the trust point pinned by "Mark reviewed", a merge-base, or any rev), renders structural AST changes for supported source files plus package.json dependency drift, supports per-node review with progress, and raises heuristic security flags for human review. A read-only `diff-drift check` CLI exposes the same analysis with severity exit codes for scripts and agent hooks.

The product test every change must strengthen: *"After an AI agent changes my repo, Diff Drift shows me what changed structurally, what looks risky, and what I need to review before I trust it."*

It is not a full static analyzer, package auditor, cloud service, or LLM reviewer — being deterministic and local IS the identity. No telemetry, no model calls, no rule marketplace.

## Start Here

- Read `README.md` for the product summary.
- Read `docs/wiki/Architecture.md` for the code map.
- Read `docs/wiki/Concepts.md` before changing UI copy or counters.
- Read `docs/wiki/Rule-Reference.md` before changing security rules.
- Read `docs/wiki/Development.md` before changing tests or build scripts.

## Commands

```bash
npm install
npm run build
npm run test:rust
npm run test:e2e:web
npm run tauri dev
```

Native E2E:

```bash
npm run test:e2e:tauri
```

## Architecture Map

- `src-tauri/src/git.rs`: repo discovery, changed files (vs HEAD or any baseline commit), file contents at any rev.
- `src-tauri/src/parse.rs`: supported source parsing (`Lang`) for TS/TSX/JS/JSX plus Rust, Go, Python, Java, C#, Kotlin, and Swift.
- `src-tauri/src/diff.rs`: AST diffing and node IDs.
- `src-tauri/src/rules.rs`: heuristic rule predicates.
- `src-tauri/src/heuristics.rs`: rule walker and flag attachment.
- `src-tauri/src/deps_diff.rs`: package.json dependency/script drift + lockfile rules.
- `src-tauri/src/session.rs`: baseline resolution, analysis orchestration, review marking, counts.
- `src-tauri/src/watcher.rs`: live updates and triage/baseline commands.
- `src-tauri/src/store.rs`: per-repo state — dismissed flags, reviewed fingerprint, baseline choice, trust point, per-node review hashes.
- `src-tauri/src/report.rs`: Markdown/JSON export.
- `src-tauri/src/cli.rs`: read-only `diff-drift check` (JSON/MD + severity exit codes). Must stay read-only.
- `src/App.tsx`: frontend state and command wiring.
- `src/components/`: UI panels.
- `src/types.ts` and `src-tauri/src/model.rs`: shared data contract (`SCHEMA_VERSION` — bump on breaking shape changes).

## Working Rules

- Make surgical changes. Do not refactor unrelated code.
- Keep README short. Put details in `docs/wiki/`.
- Keep UI wording honest about scope: structural drift for supported source languages plus dependency drift for `package.json` and the Cargo, Go, PyPI, Maven, and NuGet manifests; heuristic security flags run across every supported family where the rule's concept exists, with the documented limits in `docs/wiki/Rule-Reference.md`.
- Treat flags as review prompts, not vulnerability verdicts.
- Add Rust tests for rule, parser, diff, git, watcher, deps-diff, CLI, or report changes.
- Update Playwright assertions when UI labels change.
- Keep `SessionData` in sync across `model.rs`, `types.ts`, `mockSession.ts`, reports, and tests; bump `SCHEMA_VERSION` for breaking shape changes.
- Do not add telemetry, remote model calls, or repository upload behavior.
- Owner no-gos (do not propose): mini-SAST depth chasing, cloud/LLM review, team sync/accounts, plugin systems/rule marketplaces. Cross-language rule parity is the existing rules running across the supported families where the concept exists — it is not a license to add new vulnerability classes or chase SAST depth.

## Known Limitations

- Analysis is heuristic. Structural drift is available for supported source languages plus dependency drift for `package.json` and the Cargo, Go, PyPI, Maven, and NuGet manifests. The security rules run across all supported families where the concept exists (parity): each rule declares its families and runs only where the language has the matching idiom — for example error-handling-removed does not run on Go, the TLS env-var rule runs only on JS/TS and Python, and `new Function`, permissive-logging, and undeclared-import stay JS/TS. The honest edges are the numbered "Known limits" in `docs/wiki/Rule-Reference.md`.
- Unsupported changed files can count as git drift but are not parsed as AST nodes.
- Committed-range baselines treat renames as removed + added (no rename detection yet).
- macOS is experimental and unsigned.
- Some watcher edge cases may require reopening the repo.
