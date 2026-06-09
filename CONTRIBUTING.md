# Contributing

Diff Drift is early and intentionally focused. Small, tested changes are preferred.

## Setup

```bash
npm install
npm run build
npm run test:rust
```

Run the app:

```bash
npm run tauri dev
```

Browser mock mode:

```bash
npm run dev
```

## Before Opening A PR

Run the checks relevant to your change:

```bash
npm run test:rust
npm run build
npm run test:e2e:web
```

Run native E2E for native app behavior:

```bash
npm run test:e2e:tauri
```

## Change Guidelines

- Rule changes need focused Rust tests.
- Parser, diff, git, watcher, report, and store changes need Rust tests.
- UI label changes may require Playwright updates.
- Documentation should be short and linked. Avoid moving the whole wiki into the README.
- Keep the app local-only. Do not add telemetry or remote analysis.

## Where To Put Docs

- Short product entry: `README.md`.
- Human and agent handbook: `docs/wiki/`.
- AI-agent repo instructions: `AGENTS.md`.
