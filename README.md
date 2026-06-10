# Diff Drift

<p align="center">
  <img src="src-tauri/icons/128x128.png" alt="Diff Drift app icon" width="72" height="72">
</p>

<p align="center">
  <strong>Review what an AI coding agent changed since the last state you trusted.</strong>
</p>

<p align="center">
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-4ec46a?style=for-the-badge"></a>
  <img alt="Version 0.3.0" src="https://img.shields.io/badge/version-0.3.0-e7a83e?style=for-the-badge">
  <img alt="Windows 11" src="https://img.shields.io/badge/platform-Windows%2011-6f8bc4?style=for-the-badge">
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri-2-24c8db?style=for-the-badge">
  <img alt="Rust core" src="https://img.shields.io/badge/Rust-core-f2604c?style=for-the-badge">
</p>

<p align="center">
  <a href="#try-it">Try it</a> ·
  <a href="#give-feedback">Give feedback</a> ·
  <a href="docs/wiki/Home.md">Wiki</a> ·
  <a href="CHANGELOG.md">Changelog</a> ·
  <a href="LICENSE">License</a>
</p>

<p align="center">
  <img src="docs/assets/diff-drift-demo.gif" alt="Diff Drift reviewing a mock payments-api session: picking a baseline, inspecting a loosened token regex, marking nodes reviewed, exporting a report" width="680">
</p>

An AI agent just changed your repo. A normal diff shows you every hunk it touched. What you actually need to know is what structurally changed since the last point you trusted, and which of those changes still needs a human.

Diff Drift is that second pass. It compares your working tree against a baseline you pick — `HEAD`, the **trust point** pinned by your last review, or the merge-base with `main` — and shows the drift as changed AST nodes for TypeScript, TSX, JavaScript, and JSX, plus `package.json` dependency drift. You review node by node, dismiss flags that don't matter in your codebase, and mark the drift reviewed. That pins a trust point: when the agent commits and keeps working, the next session reopens only what changed again.

Flags point you at security-shaped drift — a loosened validation regex, removed sanitization, disabled TLS checks, undeclared imports, dependencies the lockfile can't vouch for. Most rules match structurally against the parsed AST rather than by text, so a pattern inside a string or comment doesn't trigger a flag and reformatting doesn't evade one. Some are differential: they compare a node against your trusted baseline and flag what got *weaker* — a regex that lost its anchors, a call that lost its guard — which a snapshot scanner can't see. An exported Markdown or JSON report gives you evidence for the PR, and the same engine runs headless with severity exit codes for CI and agent hooks.

## Try it

**Desktop:** download the Windows installer from [Releases](https://github.com/Statusnone420/Diff-Drift/releases) and check it against `SHA256SUMS.txt`. Releases are currently unsigned, so SmartScreen will warn on first run — details and reproducible builds in [Release and Platform Support](docs/wiki/Release-and-Platform-Support.md).

**CLI:** the same installed `diff-drift.exe` works headless. Add `%LOCALAPPDATA%\Diff Drift` to `PATH`, then:

```bash
diff-drift check . --baseline merge-base --md > diff-drift-report.md
```

The exit code is the highest active severity (`0` none, `1` low, `2` medium, `3` high, `64` usage error), so it drops straight into CI or a pre-commit hook. Copy-paste recipes: [CI and hook recipes](docs/wiki/User-Guide.md#ci-and-hook-recipes).

**From source:** [Node.js](https://nodejs.org/) 20.19+/22.12+, [Rust](https://rustup.rs/) stable, [C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/), then `npm install && npm run tauri dev`. Browser-only UI work: `npm run dev`.

## What it is not

- It runs locally and uses no AI itself. No model calls, no telemetry, no repository upload — verify that instead of trusting it: [Privacy and Data Flow](docs/wiki/Privacy-and-Data-Flow.md).
- It is not a full SAST engine, and a clean run is not proof that code is safe. Flags are heuristic prompts for a reviewer, not verdicts. Need deep dataflow analysis? Run Semgrep or CodeQL alongside.
- It does not replace human review. It points review at the risky nodes first.

| Tool | Its job | Diff Drift's lane |
| --- | --- | --- |
| `git diff` | Exact text changes | Structure instead of hunks, plus triage state and exit codes |
| PR review | Judgment | Points reviewers at the risky nodes first — never replaces them |
| Semgrep | Broad rule-based SAST | Drift-scoped, zero-config, local; tracks review across agent commits |
| CodeQL | Deep dataflow analysis | Answers a different question: "what did the agent just change?" |

## Give feedback

This is a public feedback release ([release notes](docs/releases/v0.3.0-public-feedback.md)). The question that decides where this project goes:

> Would you use this before trusting code changed by an AI coding agent? Why or why not?

Answer it in [**I tried Diff Drift**](https://github.com/Statusnone420/Diff-Drift/issues/new?template=tried-diff-drift.yml) — or report an [install problem](https://github.com/Statusnone420/Diff-Drift/issues/new?template=install-problem.yml), a [noisy flag](https://github.com/Statusnone420/Diff-Drift/issues/new?template=noisy-flag.yml), or [confusing output](https://github.com/Statusnone420/Diff-Drift/issues/new?template=confusing-output.yml). Not sure it's an issue? [Start a discussion](https://github.com/Statusnone420/Diff-Drift/discussions). Be as blunt as you like.

## Evaluation

A deterministic engine benchmark gates CI: 15 fixture cases through the real binary with exact expected flags and exit codes (`npm run eval:engine`). An advisory blind-agent scorecard exists too, but it's synthetic and self-run — read it with the limits attached: [Eval Methodology](docs/wiki/Eval-Methodology.md). To predict your own triage burden, run `npm run eval:fp-replay` on your repos; that's the number that matters for you.

## Status

- Supported platform: Windows 11. macOS: experimental and unsigned.
- Current version: `0.3.0` ([changelog](CHANGELOG.md)). License: [MIT](LICENSE). Security policy: [SECURITY.md](SECURITY.md).

## Docs

- [User Guide](docs/wiki/User-Guide.md) · [Concepts](docs/wiki/Concepts.md) · [Rule Reference](docs/wiki/Rule-Reference.md) · [FAQ](docs/wiki/FAQ.md)
- [Threat Model](docs/wiki/Threat-Model.md) · [Privacy and Data Flow](docs/wiki/Privacy-and-Data-Flow.md)
- [Eval Methodology](docs/wiki/Eval-Methodology.md) · [A/B Study Design](docs/wiki/AB-Study-Design.md)
- [Architecture](docs/wiki/Architecture.md) · [Development](docs/wiki/Development.md) · [Release and Platform Support](docs/wiki/Release-and-Platform-Support.md) · [Troubleshooting](docs/wiki/Troubleshooting.md)
- [Demo script](docs/demo/demo-script.md) · [Feedback triage](docs/feedback/feedback-triage.md)

The `docs/wiki/` pages are the source copy for the GitHub wiki.
