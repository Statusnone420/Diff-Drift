# Concepts

## Drift

Drift means the uncommitted difference between the current git working tree and `HEAD`.

Diff Drift is intentionally scoped to that review surface because it is what a developer or AI coding agent leaves behind before a commit.

## Session

A session is the current analysis result for one opened repo:

- repo name
- branch
- changed file count
- analyzed files
- active and dismissed flags
- reviewed state

Live watcher updates replace the session as files change.

## Changed File

A changed file is any path git reports as changed, including non-TypeScript files. This count is broader than what Diff Drift can parse as AST nodes.

## Analyzed File

An analyzed file is a changed `.ts` or `.tsx` file that Diff Drift parsed and rendered as AST drift.

## Node

A node is a parsed TypeScript/TSX structure such as an import, function, variable declaration, return statement, guard, or expression. Node cards show whether that structure was added, modified, removed, or unchanged.

## Flag

A flag is a heuristic review prompt attached to a changed node. Flags have severity, type, description, file path, node path, and dismissed state.

Flags are not verdicts. They mean "review this change."

## Dismissed

Dismissed means the user has decided a flag is not active for this repo's current review. Dismissed flags remain visible in a separate section and can be restored.

## Reviewed

Reviewed means the current drift fingerprint was marked as reviewed. Any meaningful drift change clears that reviewed state.

## Export

Export writes the current session as Markdown or JSON. Markdown is for people. JSON is for tooling.
