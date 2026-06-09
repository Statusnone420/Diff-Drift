# AGENTS.md

This file is for AI agents and contributors working in this repo.

`CLAUDE.md` and `GEMINI.md` intentionally point here. Keep this file as the canonical repo-level instruction source.

## Product Goal

Diff Drift is a local desktop reviewer for uncommitted TypeScript/TSX drift. It compares a git working tree against `HEAD`, renders structural AST changes, and raises heuristic security flags for human review.

It is not a full static analyzer, package auditor, or remote AI service.

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

- `src-tauri/src/git.rs`: repo discovery, changed files, file contents.
- `src-tauri/src/parse.rs`: TypeScript/TSX parsing.
- `src-tauri/src/diff.rs`: AST diffing and node IDs.
- `src-tauri/src/rules.rs`: heuristic rule predicates.
- `src-tauri/src/heuristics.rs`: rule walker and flag attachment.
- `src-tauri/src/session.rs`: analysis orchestration and counts.
- `src-tauri/src/watcher.rs`: live updates.
- `src-tauri/src/store.rs`: dismissed flags and reviewed fingerprints.
- `src-tauri/src/report.rs`: Markdown/JSON export.
- `src/App.tsx`: frontend state and command wiring.
- `src/components/`: UI panels.
- `src/types.ts` and `src-tauri/src/model.rs`: shared data contract.

## Working Rules

- Make surgical changes. Do not refactor unrelated code.
- Keep README short. Put details in `docs/wiki/`.
- Keep UI wording honest about scope: changed `.ts` and `.tsx` drift.
- Treat flags as review prompts, not vulnerability verdicts.
- Add Rust tests for rule, parser, diff, git, watcher, or report changes.
- Update Playwright assertions when UI labels change.
- Do not add telemetry, remote model calls, or repository upload behavior.

## Known Limitations

- Analysis is heuristic and scoped to TypeScript/TSX drift.
- Unsupported changed files can count as git drift but are not parsed as AST nodes.
- macOS is experimental and unsigned.
- Some watcher edge cases may require reopening the repo.
