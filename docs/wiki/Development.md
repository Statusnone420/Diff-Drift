# Development

## Prerequisites

- Node.js 18+ and npm
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
npm run test:e2e:web
```

Native E2E:

```bash
npm run test:e2e:tauri
```

Native E2E builds a debug Tauri app and launches it with isolated environment variables:

- `DIFF_DRIFT_E2E_REPO`
- `DIFF_DRIFT_E2E_EXPORT_PATH`
- `DIFF_DRIFT_E2E_STATE_FILE`
- `DIFF_DRIFT_E2E_BIN`

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
