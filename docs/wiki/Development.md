# Development

## Prerequisites

- Node.js 20.19.x, 22.12+, or 24+ and npm
- Rust stable
- Tauri prerequisites for your platform
- On Windows: Microsoft C++ Build Tools and WebView2

## Install

```bash
npm install
```

## Run

Native app:

```bash
npm run tauri dev
```

Browser mock mode:

```bash
npm run dev
```

Browser mode does not use the Rust backend. It loads `src/data/mockSession.ts` after clicking **Open a repository**.

## Test

```bash
npm run test:rust
npm run build
npm run test:unit
npm run test:e2e:web
```

`test:rust` includes integration tests in `src-tauri/tests/` that spawn the built `diff-drift` binary. `test:unit` runs the vitest component tests in `tests/unit/`.

Native E2E:

```bash
npm run test:e2e:tauri
```

Native E2E builds a debug Tauri app and launches it with isolated environment variables:

- `DIFF_DRIFT_E2E_REPO`
- `DIFF_DRIFT_E2E_EXPORT_PATH`
- `DIFF_DRIFT_E2E_STATE_FILE`
- `DIFF_DRIFT_E2E_BIN`

## Benchmarks

Rust analyzer benchmarks are local-only and are not part of CI:

```bash
npm run bench
```

For optimization work, capture a Criterion baseline before changing code, then compare after:

```bash
cargo bench --manifest-path src-tauri/Cargo.toml -- --save-baseline pre-opt
cargo bench --manifest-path src-tauri/Cargo.toml -- --baseline pre-opt
```

## Visual Baselines

Visual regression checks are local-only and are not run in CI. They cover the browser mock onboarding, loaded session, and dismissed states.

```bash
npm run test:visual
```

Regenerate baselines intentionally after a reviewed UI change:

```bash
npm run test:visual:update
npm run test:visual
```

## CI

`.github/workflows/ci.yml` runs on every push/PR: `cargo test` + `cargo clippy -- -D warnings` on Windows; `npm run build` + unit tests + the web E2E on Ubuntu; a native Tauri build smoke (debug, no bundle) plus an advisory native E2E on Windows; and `cargo audit` + `npm audit` dependency gates on Ubuntu. Keep clippy clean — warnings fail the build.

## Headless Check

The debug binary also serves the CLI: `cargo run -- check <path> --json` (or run the built `diff-drift.exe check …`). It is read-only by design; see the User Guide for flags and exit codes.

## Fixtures

Rust unit tests use `src-tauri/src/test_fixture.rs` to build temporary git repos with `git2`. They should not depend on `demo/` or a globally installed `git` binary.

The optional demo repo can be seeded with:

```bash
npm run seed:demo
```

## Expected PR Scope

Keep changes narrow:

- Backend analyzer changes need Rust tests.
- Frontend copy or interaction changes need web E2E updates when assertions change.
- Native behavior changes should run native E2E locally when practical.
- Documentation changes should avoid repeating the README in every page.

## Useful Files

- `src-tauri/src/rules.rs`: rule predicates and tests.
- `src-tauri/src/session.rs`: analysis orchestration and counts.
- `src-tauri/src/watcher.rs`: live updates.
- `src/App.tsx`: main frontend state.
- `src/types.ts` and `src-tauri/src/model.rs`: shared contract.
