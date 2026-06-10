# Privacy and Data Flow

One page for anyone who needs to approve Diff Drift: what it reads, what it writes, and what leaves the machine.

## Summary

**Nothing leaves the machine.** Diff Drift has no telemetry, no analytics, no crash reporting, no model calls, no update pings, and no HTTP client compiled into the app. All analysis is local and deterministic.

## What It Reads

| Data | When | How |
| --- | --- | --- |
| Files in the repo you open | On open, on baseline change, on file change (watcher) | Direct filesystem reads, scoped to paths git reports as changed |
| Git objects (file content at the baseline rev, refs, status) | Same | `git2` (libgit2) — read-only, no `git` binary is executed |
| `package.json` + lockfile in the repo root | Same | Read for dependency-drift analysis |
| Per-repo triage state | On open | `repo-state.json` (see below) |

Diff Drift reads only the repository you explicitly open via the folder picker (or pass to `diff-drift check`). It does not enumerate other folders, browse your disk, or index anything in the background.

## What It Writes

| Data | Where | Contents |
| --- | --- | --- |
| Triage state | `%APPDATA%\io.github.statusnone420.diffdrift\repo-state.json` | Dismissed flag IDs + content hashes, baseline choice, trust-point SHA, per-node review hashes, keyed by repo path |
| Exported reports | A path you pick in the save dialog | The Markdown/JSON report you asked for |

That is the complete write surface of the GUI. The CLI writes nothing — `diff-drift check` is read-only by contract and only prints to stdout/stderr.

The triage state contains repo paths, flag IDs, and content hashes — not source code. Exported reports do contain code excerpts (the drifted nodes); they go only where you save them.

## What Leaves the Machine

Nothing. To verify rather than trust:

1. **App code**: grep `src/` and `src-tauri/src/` for `fetch(`, `reqwest`, `http://`, `https://` — zero matches. No application code performs network I/O.
2. **Build graph**: `cargo tree -i reqwest` (and `-i hyper`) on the supported Windows target prints nothing — no HTTP client is compiled into the binary. You will see `reqwest` in `Cargo.lock`: lockfiles pin dependencies for every platform, and the Tauri framework pulls an HTTP stack on *other* targets (`cargo tree --target all -i reqwest` shows tauri as the sole path). It is not part of the Windows build, and no enabled Diff Drift feature or plugin uses it on any target — no updater, no HTTP plugin. The framework dependency can't be removed without leaving Tauri; the build-graph check is the meaningful one.
3. **Renderer CSP**: [tauri.conf.json](../../src-tauri/tauri.conf.json) sets `connect-src ipc: http://ipc.localhost` — the UI process cannot reach any external host even if it tried.
4. **Capabilities**: [src-tauri/capabilities/default.json](../../src-tauri/capabilities/default.json) grants the renderer window controls, the OS file dialog, and `opener:default` (open a URL/path in the system default app — granted by the Tauri template, not invoked by Diff Drift's code) — no HTTP, no filesystem, no shell.
5. **Observe it**: run the app under a local firewall or packet capture; it opens no sockets beyond the loopback IPC channel WebView2 uses internally.

## Related

- [Threat Model](Threat-Model.md) — trust boundaries and non-goals.
- [SECURITY.md](../../SECURITY.md) — reporting vulnerabilities.
- Contributor rule: pull requests adding telemetry, remote analysis, or repository upload are rejected on principle (see `AGENTS.md` and `CONTRIBUTING.md`). Local-only is the product identity, not a current limitation.
