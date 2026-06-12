# Security Policy

## Reporting

Report vulnerabilities privately via [GitHub Security Advisories](https://github.com/Statusnone420/Diff-Drift/security/advisories/new) — please don't open a public issue for an unpatched bug. Include the version, reproduction steps, and ideally a proof-of-concept repo, since the main attack surface is the repository content Diff Drift parses.

This is a single-maintainer project: reports are handled best-effort, with an honest assessment and a fix or documented mitigation for confirmed issues. Only the latest 0.4.x release is supported — there is no auto-update, so check [Releases](https://github.com/Statusnone420/Diff-Drift/releases).

## Scope

In scope: the desktop app and the `diff-drift check` CLI; crashes, hangs, or memory exhaustion triggered by repo content; anything that contradicts the local-only posture (network calls, data leaving the machine, writes outside the documented state and export paths); and the release pipeline and published artifacts.

Out of scope: tampering with `repo-state.json` by something that already has write access to your profile (plain JSON by design — see the [Threat Model](docs/wiki/Threat-Model.md)); vulnerabilities in the code Diff Drift reviews (flags are heuristic prompts, and missed findings are a quality topic, not a vulnerability); and macOS hardening while macOS builds are experimental.

## Posture

Local-only: no telemetry, no model calls, no upload, and no HTTP client compiled into the app. [Privacy and Data Flow](docs/wiki/Privacy-and-Data-Flow.md) shows how to verify that yourself.
