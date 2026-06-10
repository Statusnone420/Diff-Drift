# Concepts

## Drift

Drift means the difference between a chosen **baseline** commit and the current git working tree — uncommitted edits, plus any commits made since the baseline.

The default baseline is `HEAD`, which scopes drift to uncommitted changes. Other baselines keep drift visible after an agent commits its work.

## Baseline

The baseline is the "before" side of every comparison. Per repo, it can be:

- **Last commit (HEAD)** (default): uncommitted drift only.
- **Last review (trust point)**: the commit pinned by the last **Mark reviewed** — everything since you last trusted the code.
- **Branch start (merge-base)**: the common ancestor with `main`/`master` — everything this branch adds.
- **Custom ref**: any branch, tag, or SHA.

The choice persists per repo via the toolbar's **Review changes since** picker. Choices that can't resolve (no trust point yet, no default branch, unknown ref) are rejected with a message; analysis never silently changes baselines.

## Trust Point

The trust point is the commit SHA recorded when you click **Mark reviewed** — the last state of the code you personally trusted. With the trust-point baseline selected, "reviewed" means "reviewed everything since here", and an agent committing its work doesn't hide the drift.

## Session

A session is the current analysis result for one opened repo:

- repo name and branch
- baseline (spec and resolved label)
- changed file count
- analyzed files
- active and dismissed flags
- per-node review progress
- reviewed state

Live watcher updates replace the session as files change.

## Changed File

A changed file is any path git reports as different from the baseline, including files Diff Drift can't parse. This count is broader than what gets analyzed.

## Analyzed File

An analyzed file is a changed `.ts`, `.tsx`, `.js`, `.jsx`, `.mjs`, or `.cjs` file that Diff Drift parsed and rendered as AST drift — plus `package.json`, which is rendered as a dependency diff when its dependency or script sections changed.

## Node

A node is a parsed source structure such as an import, function, variable declaration, return statement, guard, or expression — or, for `package.json`, a dependency or script entry. Node cards show whether that structure was added, modified, removed, or unchanged.

## Flag

A flag is a heuristic review prompt attached to a changed node. Flags have severity, type, description, file path, node path, and dismissed state.

Flags are not verdicts. They mean "review this change."

## Dismissed

Dismissed means the user has decided a flag is not active for this repo's current review. Dismissed flags remain visible in a separate section and can be restored.

## Node Review

Each changed node can be individually marked reviewed. The review pins the node's content hash: if that node's content drifts again afterwards, it automatically reads as unreviewed — that is the "new since last look" signal. Progress (`reviewed/changed`) shows per file and across the whole drift.

## Reviewed

**Mark reviewed** records three things at once: the drift fingerprint (so any meaningful change clears the reviewed state), the trust point (the current `HEAD` commit), and a review mark on every changed node.

## Export

Export writes the current session as Markdown or JSON. Markdown is for people. JSON is for tooling — it carries a `schemaVersion` field so consumers can detect contract changes.

## Headless Check

`diff-drift check [path]` runs the same analysis without the GUI and prints the session as JSON (or `--md`). The exit code is the highest active severity (0 none / 1 low / 2 medium / 3 high), so scripts and agent hooks can gate on it. An explicit `--baseline` that can't resolve exits 64 with the cause on stderr — never a silent HEAD fallback. It reads the same per-repo triage state as the app and never writes anything.
