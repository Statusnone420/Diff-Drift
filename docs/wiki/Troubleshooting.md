# Troubleshooting

## The App Says The Folder Is Not A Git Repository

Open a folder inside a git working tree. Diff Drift uses `git2` to discover the repo root from the selected path.

## No Drift Detected

The working tree matches the selected baseline, or the current changes were reverted. Check the toolbar **Scope** menu if you expected committed drift to remain visible, then create or save a change and wait for the watcher to update.

## Changed Files But No Analyzed Files

Diff Drift analyzes changed TypeScript, TSX, JavaScript, JSX, and root `package.json` dependency/script drift. Other changed files can still count as git drift, but they will not appear as AST nodes or dependency nodes.

## A File Shows "Skipped — file too large to analyze"

Files over 2 MB on either side of the drift are not parsed — this is a guard against giant generated bundles exhausting memory or CPU. The file stays in the list so the skip is visible. There is no override; review oversized files by other means (and consider whether a 2 MB+ source file should be in the drift at all).

## A Flag Looks Wrong

Flags are heuristic prompts. Dismiss the flag if it is reviewed and not actionable. If it is noisy in a common code shape, open an issue or Discussion with:

- file type
- import or code pattern
- expected behavior
- screenshot or exported report if useful

## Watcher Looks Stale

Save the file again or reopen the repo. Branch switches, resets, commits, and package metadata changes should trigger a rescan, but watcher behavior can vary by filesystem and editor.

## Export Failed

Check that the chosen path is writable and not locked by another app. In native E2E, export can be controlled with `DIFF_DRIFT_E2E_EXPORT_PATH`.

## Native App Will Not Build

Check the platform prerequisites:

- Rust stable installed.
- Node dependencies installed with `npm install`.
- Tauri prerequisites installed.
- On Windows, Microsoft C++ Build Tools and WebView2 are present.

Then run:

```bash
npm run build
npm run test:rust
```

If those pass but native build fails, include the Tauri error output in the issue.
