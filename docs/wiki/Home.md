# Diff Drift Wiki

Diff Drift is a local desktop reviewer for the code drift AI coding agents leave behind. It compares a git working tree against a chosen baseline (`HEAD`, the pinned trust point, a merge-base, or any rev), renders changed TS/TSX/JS/JSX as an AST-level drift view plus package.json dependency drift, and raises heuristic security flags for human review. A read-only `diff-drift check` command exposes the same analysis to scripts and agents.

This wiki is the handbook. The README stays short on purpose.

## Start Here

- [User Guide](User-Guide.md): open a repo, read the panels, review flags, export a report, CI/hook recipes.
- [Concepts](Concepts.md): drift, sessions, nodes, flags, dismissed, reviewed.
- [Rule Reference](Rule-Reference.md): what each security heuristic means.
- [FAQ](FAQ.md): triage burden, false positives, skipped files, CI integration.
- [Architecture](Architecture.md): codebase map for contributors and AI agents.
- [Development](Development.md): setup, commands, tests, fixtures.
- [Eval Methodology](Eval-Methodology.md): how the engine benchmark and blind-agent scorecards work, and their limits.
- [A/B Study Design](AB-Study-Design.md): pre-registered design for the packet-vs-raw-diff study (no results yet).
- [Threat Model](Threat-Model.md): trust boundaries, attack surface, explicit non-goals.
- [Privacy and Data Flow](Privacy-and-Data-Flow.md): what is read, what is written, what leaves the machine (nothing).
- [Release and Platform Support](Release-and-Platform-Support.md): Windows, macOS status, release runbook, signing, winget.
- [Troubleshooting](Troubleshooting.md): common setup and app behavior issues.
- [Changelog](../../CHANGELOG.md) and [Security Policy](../../SECURITY.md).
- [GitHub Discussions](https://github.com/Statusnone420/Diff-Drift/discussions): questions, ideas, and support.

## Current Status

- Version: `0.2.1`.
- Supported platform: Windows 11.
- macOS: experimental and unsigned.
- Analysis scope: changed `.ts`/`.tsx`/`.js`/`.jsx`/`.mjs`/`.cjs` files plus `package.json` dependency drift, against a selectable baseline.
- Rule results are review prompts, not vulnerability verdicts.

## Source Copy

These pages live in `docs/wiki/` so they are available in a normal clone. The GitHub wiki can mirror them, but this repo copy is the source of truth.
