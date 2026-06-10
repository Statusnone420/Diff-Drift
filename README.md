# Diff Drift

<p align="center">
  <img src="src-tauri/icons/128x128.png" alt="Diff Drift app icon" width="72" height="72">
</p>

<p align="center">
  <strong>AST-level security review for the code drift left by AI coding agents.</strong>
</p>

<p align="center">
  <a href="LICENSE"><img alt="License: MIT" src="https://img.shields.io/badge/license-MIT-4ec46a?style=for-the-badge"></a>
  <img alt="Version 0.2.1" src="https://img.shields.io/badge/version-0.2.1-e7a83e?style=for-the-badge">
  <img alt="Windows 11" src="https://img.shields.io/badge/platform-Windows%2011-6f8bc4?style=for-the-badge">
  <img alt="Tauri 2" src="https://img.shields.io/badge/Tauri-2-24c8db?style=for-the-badge">
  <img alt="Rust core" src="https://img.shields.io/badge/Rust-core-f2604c?style=for-the-badge">
  <img alt="Playwright and axe" src="https://img.shields.io/badge/E2E-Playwright%20%2B%20axe-4ec46a?style=for-the-badge">
</p>

<p align="center">
  <a href="#install">Install</a> ·
  <a href="#quick-start-from-source">Quick start</a> ·
  <a href="docs/wiki/Home.md">Wiki</a> ·
  <a href="CHANGELOG.md">Changelog</a> ·
  <a href="LICENSE">License</a>
</p>

<p align="center">
  <img src="docs/assets/diff-drift-mock-session.png" alt="Diff Drift reviewing mock payments-api drift" width="920">
</p>

An AI agent just changed your repo. Diff Drift is the deterministic second pass before you trust it: a local desktop reviewer for TypeScript, TSX, JavaScript, and JSX drift — plus package.json dependency drift.

Use it when an agent made a broad edit, a refactor touched security-sensitive code, or a normal diff is too noisy to explain what structurally changed.

- Shows structural AST drift against a baseline you choose: `HEAD`, the **trust point** pinned by your last review (drift stays visible after the agent commits), the merge-base with `main`, or any rev.
- Flags heuristic security concerns such as loosened validation, removed sanitization, disabled TLS checks, undeclared imports, and dependencies the lockfile can't vouch for.
- Lets you review changes node by node with progress tracking, dismiss flags (pinned to content — they resurface if the code changes again), mark the drift reviewed, and export a Markdown or JSON report.
- Doubles as a read-only gate for scripts and agents: `diff-drift check --json` exits with the highest active severity. See [CI and hook recipes](docs/wiki/User-Guide.md#ci-and-hook-recipes).

Diff Drift runs locally and is deliberately not an LLM. It does not send repository contents to a server or model API — it's the reviewer in the loop that can't hallucinate or be prompt-injected. Verify that claim instead of trusting it: [Privacy and Data Flow](docs/wiki/Privacy-and-Data-Flow.md).

## When To Use What

Diff Drift complements these tools; it doesn't replace them.

| Tool | Its job | Diff Drift's lane |
| --- | --- | --- |
| `git diff` | Exact text changes | Structure instead of hunks, plus triage state and exit codes |
| PR review | Judgment | Points reviewers at the risky nodes first — never replaces them |
| Semgrep | Broad rule-based SAST | Drift-scoped, zero-config, local; tracks review across agent commits |
| CodeQL | Deep dataflow analysis | Answers a different question: "what did the agent just change?" |

Need a full static analyzer? Run one — alongside, not instead.

## Install

Download the Windows installer from [Releases](https://github.com/Statusnone420/Diff-Drift/releases) and check it against `SHA256SUMS.txt`. Releases are currently unsigned, so SmartScreen will warn on first run — details and reproducible builds in [Release and Platform Support](docs/wiki/Release-and-Platform-Support.md).

The CLI is the same installed `diff-drift.exe` — add `%LOCALAPPDATA%\Diff Drift` to `PATH` and `diff-drift check` works anywhere ([details](docs/wiki/User-Guide.md#headless-check-for-scripts-and-agents)).

## Quick Start (from source)

Prerequisites: [Node.js](https://nodejs.org/) 20.19.x, 22.12+, or 24+; [Rust](https://rustup.rs/) stable; [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/); and [WebView2](https://developer.microsoft.com/microsoft-edge/webview2/) (preinstalled on Windows 11).

```bash
npm install
npm run tauri dev
```

For fast browser-only UI work:

```bash
npm run dev
```

## Gate CI or an Agent Hook

```bash
# Exit code = highest active severity: 0 none, 1 low, 2 medium, 3 high, 64 usage error.
diff-drift check . --baseline merge-base || echo "drift needs review"
```

Dismissed flags don't count, the CLI never writes state, and an unresolvable `--baseline` fails loudly with exit 64 instead of silently falling back. Copy-paste recipes for GitHub Actions, pre-commit, and agent hooks: [CI and hook recipes](docs/wiki/User-Guide.md#ci-and-hook-recipes).

## Evaluation

The CI gate is deterministic: 15 fixture cases through the real binary, exact expected flags and exit codes (`npm run eval:engine`).

The blind-agent scorecard below is advisory: **98/100 over 15 synthetic cases** (benchmark v2), one blind model evaluator, independent external validation pending. Rubric, limits, and version history in [Eval Methodology](docs/wiki/Eval-Methodology.md).

<p align="center">
  <img src="docs/assets/diff-drift-blind-agent-scorecard.png" alt="Diff Drift blind-agent benchmark scorecard: 98/100 over 15 cases (benchmark v2), single model evaluator, external validation pending" width="920">
</p>

To predict your own triage burden, run `npm run eval:fp-replay` on your repos — that's the number that matters for you.

## Status

- Supported platform: Windows 11.
- macOS: experimental and unsigned. Signing and notarization are not configured.
- Current version: `0.2.1` ([changelog](CHANGELOG.md)).
- License: [MIT](LICENSE). Security policy: [SECURITY.md](SECURITY.md).

## Docs

- [User Guide](docs/wiki/User-Guide.md) · [Concepts](docs/wiki/Concepts.md) · [Rule Reference](docs/wiki/Rule-Reference.md) · [FAQ](docs/wiki/FAQ.md)
- [Threat Model](docs/wiki/Threat-Model.md) · [Privacy and Data Flow](docs/wiki/Privacy-and-Data-Flow.md)
- [Eval Methodology](docs/wiki/Eval-Methodology.md) · [A/B Study Design](docs/wiki/AB-Study-Design.md)
- [Architecture](docs/wiki/Architecture.md) · [Development](docs/wiki/Development.md) · [Release and Platform Support](docs/wiki/Release-and-Platform-Support.md) · [Troubleshooting](docs/wiki/Troubleshooting.md)
- [Questions and ideas](https://github.com/Statusnone420/Diff-Drift/discussions)

The `docs/wiki/` pages are the source copy for the GitHub wiki.
