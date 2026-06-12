# Threat Model

What Diff Drift defends against, what it trusts, and what is explicitly out of scope. Written for security reviewers deciding whether to allow the tool; references point at the code so claims can be checked.

## What Diff Drift Is

A local Windows desktop app (Tauri 2: Rust backend + WebView2 frontend) and a read-only CLI. It opens a git repository you choose, parses supported changed source files and `package.json`, and renders structural drift with heuristic security flags. Security heuristics are strongest for JS/TS and package drift, with language-neutral secret detection across the newer structural language families. Nothing leaves the machine — see [Privacy and Data Flow](Privacy-and-Data-Flow.md).

## Assets

- **Your repository contents** — read, never modified, never transmitted.
- **Per-repo triage state** (`repo-state.json` in the app config dir) — dismissed flags, baseline choice, trust point, review hashes. Integrity matters: stale or forged triage could hide a flag.
- **Exported reports** — written only to paths you choose in the save dialog.

## Trust Boundaries

### Untrusted: repository content

The main attack surface. A cloned repo can contain arbitrary hostile bytes, and Diff Drift parses them:

- **Source files → tree-sitter** ([parse.rs](https://github.com/Statusnone420/Diff-Drift/blob/main/src-tauri/src/parse.rs)). Parse failures return an empty node list instead of panicking; invalid UTF-8 degrades via `utf8_text(...).unwrap_or("")`. Files larger than the parse cap are skipped and labeled "Skipped — file too large to analyze" rather than parsed. The skip decision uses sizes first — baseline size from git object headers (`odb.read_header`), worktree size from filesystem metadata — so deciding *that* a file is over-cap costs neither memory nor parse time. A skipped file's content identity (used to detect changes and revoke a stale review) is then a git blob oid: within the cap the worktree is read directly (bounded by the 2 MB cap); over the cap it is **stream-hashed** by libgit2 in fixed chunks, so even a multi-GB file is never held in memory at once. The whole file is never parsed and never fully allocated.
- **`package.json` and lockfiles → serde_json / line parsing** ([deps_diff.rs](https://github.com/Statusnone420/Diff-Drift/blob/main/src-tauri/src/deps_diff.rs)). Unparseable JSON yields no dependency drift rather than an error path.
- **Git metadata → git2 (libgit2)** ([git.rs](https://github.com/Statusnone420/Diff-Drift/blob/main/src-tauri/src/git.rs)). Read-only object access; Diff Drift never shells out to a `git` binary and never writes to `.git/`.
- **Flag text rendered in the UI** comes from rule templates plus node names extracted by tree-sitter. React escapes rendered text by default, and the CSP (below) blocks remote script regardless.

Repository content is never executed: no `npm install`, no script hooks, no loading of code from the reviewed repo.

### Sandboxed: the renderer

The WebView2 frontend is confined by Tauri:

- **CSP** ([tauri.conf.json](https://github.com/Statusnone420/Diff-Drift/blob/main/src-tauri/tauri.conf.json)): `default-src 'self'; connect-src ipc: http://ipc.localhost` — the renderer can only talk to the local backend. No external network destination is permitted even if the frontend were compromised.
- **Capabilities** ([capabilities/default.json](https://github.com/Statusnone420/Diff-Drift/blob/main/src-tauri/capabilities/default.json)): window controls, the OS file dialog, and the opener plugin. No filesystem, shell, or HTTP capabilities are granted to the renderer.

### Trusted: the local user and OS

Diff Drift runs as you, with your permissions. It trusts the OS, the filesystem, and whoever can already write to your user profile.

## Explicit Non-Goals

- **Triage-state tampering is out of scope.** `repo-state.json` is plain, unsigned JSON in your app-config directory. Anyone with write access to your profile can already modify the Diff Drift binary itself, so signing the state file would add complexity without a real boundary. What the design does defend against is *staleness*: every dismissal is pinned to a content hash of the flagged node, so editing the flagged code resurfaces the flag even if the state file says "dismissed" ([store.rs](https://github.com/Statusnone420/Diff-Drift/blob/main/src-tauri/src/store.rs)).
- **Diff Drift is not a security boundary for the code it reviews.** Flags are heuristic review prompts. A clean run is not proof the change is safe, and the threat model does not claim detection of a deliberately evasive attacker — see [Rule Reference](Rule-Reference.md) for per-rule caveats.
- **Multi-user/team integrity.** Triage state is per-machine, per-user. There is no shared state to protect.

## Denial of Service

- Oversized files: skipped at a fixed byte cap decided from object headers and file metadata before content is loaded, surfaced in the file list (see [User Guide](User-Guide.md)).
- Watcher thrash: file events are debounced (400 ms) in [watcher.rs](https://github.com/Statusnone420/Diff-Drift/blob/main/src-tauri/src/watcher.rs).
- Pathological nesting: tree-sitter is error-tolerant and bounded; analysis surfaces top-level statements plus one level of function-body children, so output size stays proportional to input.

A hostile repo can still make analysis slow (many changed files near the size cap). That costs you time, not integrity: analysis is read-only.

## No-Network Posture

Verifiable, not just claimed:

- No application code performs network I/O, and no HTTP client is compiled into the Windows binary: `cargo tree -i reqwest` on the host target prints nothing. (`Cargo.lock` does list `reqwest`/`hyper` — the Tauri framework pulls an HTTP stack for *other* platforms, visible via `cargo tree --target all -i reqwest`; no enabled feature or plugin uses it on any target.)
- The CSP forbids any non-IPC connection from the renderer, and no networking capability is granted.
- No updater plugin is configured; updates are manual downloads from Releases.
- CI runs `cargo audit` and `npm audit` on every push ([ci.yml](https://github.com/Statusnone420/Diff-Drift/blob/main/.github/workflows/ci.yml)) to catch supply-chain advisories in this dependency tree.

## Residual Risks

- **tree-sitter / git2 / serde** parse untrusted input in native code. They are mature, widely deployed libraries, and CI audits them, but a memory-safety bug in a C dependency (libgit2, tree-sitter runtime) is the most plausible severe vulnerability in this design. Report suspected cases per [SECURITY.md](https://github.com/Statusnone420/Diff-Drift/blob/main/SECURITY.md).
- **Unsigned binaries** (current state): verify downloads against the published `SHA256SUMS.txt` until code signing lands — see [Release and Platform Support](Release-and-Platform-Support.md).
