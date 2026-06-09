# Diff Drift Wiki

Diff Drift is a local desktop reviewer for uncommitted TypeScript and TSX changes. It compares a git working tree against `HEAD`, renders changed code as an AST-level drift view, and raises heuristic security flags for human review.

This wiki is the handbook. The README stays short on purpose.

## Start Here

- [User Guide](User-Guide.md): open a repo, read the panels, review flags, export a report.
- [Concepts](Concepts.md): drift, sessions, nodes, flags, dismissed, reviewed.
- [Rule Reference](Rule-Reference.md): what each security heuristic means.
- [Architecture](Architecture.md): codebase map for contributors and AI agents.
- [Development](Development.md): setup, commands, tests, fixtures.
- [Release and Platform Support](Release-and-Platform-Support.md): Windows, macOS status, bundle identity.
- [Troubleshooting](Troubleshooting.md): common setup and app behavior issues.
- [GitHub Discussions](https://github.com/Statusnone420/Diff-Drift/discussions): questions, ideas, and support.

## Current Status

- Version: `0.1.1`.
- Supported platform: Windows 11.
- macOS: experimental and unsigned.
- Analysis scope: changed `.ts` and `.tsx` files.
- Rule results are review prompts, not vulnerability verdicts.

## Source Copy

These pages live in `docs/wiki/` so they are available in a normal clone. The GitHub wiki can mirror them, but this repo copy is the source of truth.
