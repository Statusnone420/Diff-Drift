# Security Policy

## Supported Versions

| Version | Supported |
| --- | --- |
| Latest 0.2.x release | Yes |
| Older releases | No — update to the latest release |

Diff Drift has no auto-update mechanism. Check the [Releases page](https://github.com/Statusnone420/Diff-Drift/releases) for new versions.

## Reporting a Vulnerability

Report vulnerabilities privately through [GitHub Security Advisories](https://github.com/Statusnone420/Diff-Drift/security/advisories/new). Do not open a public issue for an unpatched vulnerability.

Include what you can: affected version, reproduction steps, and impact. A proof-of-concept repo is ideal — Diff Drift's main attack surface is the repository content it parses.

This is a single-maintainer project. Reports are handled on a best-effort basis; there is no SLA. You can expect an acknowledgment, an honest assessment, and a fix or documented mitigation for confirmed issues in a subsequent release.

## Scope

In scope:

- The desktop app and the `diff-drift check` CLI.
- Parsing of untrusted repository content (source files, `package.json`, lockfiles, git metadata) — crashes, hangs, or memory exhaustion triggered by repo content.
- Anything that contradicts the local-only posture: network calls, data leaving the machine, or writes outside the documented state and export paths.
- The release pipeline and published artifacts (checksums, SBOMs).

Out of scope:

- Tampering with `repo-state.json` by a user or process that already has write access to your profile directory. The triage state is plain JSON by design — see [Threat Model](docs/wiki/Threat-Model.md).
- Vulnerabilities in the code Diff Drift reviews. Flags are heuristic review prompts; missed findings in your code are a product-quality topic, not a security vulnerability in Diff Drift.
- macOS hardening. macOS builds are experimental and unsigned; see [Release and Platform Support](docs/wiki/Release-and-Platform-Support.md).

## Posture Summary

Diff Drift is local-only: no telemetry, no model calls, no repository upload, no HTTP client in the dependency tree. The renderer's CSP only allows IPC to the local backend. CI gates `cargo audit` and `npm audit` on every push. See [Privacy and Data Flow](docs/wiki/Privacy-and-Data-Flow.md) for how to verify this yourself.
