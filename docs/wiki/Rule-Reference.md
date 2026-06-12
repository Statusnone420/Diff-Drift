# Rule Reference

Diff Drift rules favor useful review prompts over complete static analysis. Most security heuristics run over changed TS/TSX/JS/JSX AST nodes and package.json dependency drift. Newer structural languages (Rust, Go, Python, Java, C#, Kotlin, Swift) get AST drift and review progress; only language-neutral hardcoded secret detection runs across those language families today.

Most code rules match **structurally**: the changed node's before/after source is re-parsed with the file's tree-sitter grammar and matched against a compiled query, so a pattern inside a string literal or comment never triggers a flag and reformatting cannot evade one. A `match type` column below records how each rule matches. Rules marked **differential** compare the node's before and after versions — a question only a diff-aware tool can ask — and a few intentionally stay text-based where that is the correct tool (secret markers, env-var assignments). When a structural parse fails, a rule falls back to its text pattern rather than going silent.

## Severity

- **High**: likely security-sensitive and should be reviewed before commit.
- **Medium**: suspicious or dependency-related drift that may be intentional.
- **Low**: weaker signal, usually a review reminder.

## Rules

| Rule | Severity | Match type | Triggers On | Notes |
| --- | --- | --- | --- | --- |
| Hardcoded secret | High | Text (markers) | AWS-style keys, OpenAI-style keys, or private key markers added to source | Flagged everywhere, including test files — a real key in a fixture is still a leak (other rules still suppress in test paths). Marker-based, like gitleaks; does not detect every secret format. |
| Dynamic code execution | High | Structural | Newly added `eval(...)`/`window.eval(...)` or `new Function(...)` | Matches the real call syntax (incl. optional chaining and member form); the words inside a string or comment never flag. |
| Child process execution | High | Text | Newly added `child_process` imports or subprocess calls | Suppressed in test-like files. |
| Disabled TLS verification | High | Structural + text | `rejectUnauthorized: false` (object property) or `NODE_TLS_REJECT_UNAUTHORIZED=0` (env var) introduced | The object-property form matches quoted or unquoted keys; the env-var form is text. Review for local-dev exceptions before dismissing. |
| Broadened CORS | High | Structural | CORS `origin` opened to `*`, `true`, or `['*']` | Matches the property value structurally, including the array form. Suppressed in test-like files. |
| Weakened cookie flags | High | Text (differential) | Removal of `httpOnly`, `secure`, or weakening `sameSite` | Only fires on modified nodes where the before/after comparison shows weakening. |
| Loose regex pattern | High | Structural (differential) | Validation regex widened to a catch-all, or — on a modified node — its anchors dropped or length bound removed | Extracts regex literals from before and after and compares them; the flag names exactly what weakened. Tightening a pattern stays quiet. Not a full regex semantic analyzer. |
| Crypto downgrade | Medium | Structural (differential) | A verify/sign call replaced by a newly introduced decode/parse call | Compares real callee names, so async variants (`verifyAsync`, `decodeJwt`) count and generic `parseInt`/`parseFloat` do not. Only fires when the decode is new. |
| Guard removed | Medium | Structural (differential) | A call that ran only inside an `if` guard before now runs unconditionally | Suppresses the common refactor to an early-return/throw guard clause. A diff-only signal. |
| Error handling removed | Low | Structural (differential) | A `try` that wrapped surviving calls is gone after the change | Suppressed when the change converts to a `.catch()` promise chain. |
| Undeclared import | Medium | Node field | New bare package import not declared in root `package.json` | Ignores relative imports, Node built-ins, and common path aliases. |
| Disabled guard | Low | Structural | Guard condition rewritten to a constant-falsy value (`if (false)`, `if (0)`, `if (null)`, `if (undefined)`) | Closes the `if (0)` bypass of the old literal-only check. |
| Removed sanitization | Low | Structural (differential) | A `sanitize`/`escape`/`validate` call removed, including wrapper-stripping (`save(escapeSql(x))` → `save(x)`) | Compares real callee names, so a name surviving only in a comment neither flags nor masks. A rename to a still-validating name can still prompt. |
| Permissive logging config | Low | Text (differential) | Redaction emptied or log level lowered | Review before committing sensitive logging changes. |

## Dependency Drift Rules (package.json)

When a drifted `package.json` changes its dependency or script sections, each changed entry becomes a node with these rules:

| Rule | Severity | Triggers On | Notes |
| --- | --- | --- | --- |
| Dependency not in lockfile | High | A dependency added to package.json whose name the lockfile can't vouch for | The slopsquatting guard: agents sometimes hallucinate package names. Only fires when a lockfile exists — no lockfile, no accusation. `package-lock.json` is parsed as JSON; `yarn.lock` (classic and berry) and `pnpm-lock.yaml` (v5/v6/v9) entry names are parsed exactly, so a real `left-pad` cannot vouch for a hallucinated `pad`. Unrecognized lockfile formats fall back to a loose text check — a false "present" is safer than a false alarm. |
| New dependency | Medium | A dependency added and present in the lockfile | Verify it's intended and vetted. |
| Dependency version changed | Low | A version range changed | Confirm the bump is intentional. |
| npm script changed | Medium | A script added or modified | Scripts run arbitrary shell commands during install and dev. |

Removed dependencies and scripts are shown as drift nodes without flags.

## False Positives

False positives are expected. Use dismiss when a flag is reviewed and not actionable. If a rule repeatedly flags normal code, open a GitHub Discussion or issue with the code shape and expected behavior.

## Adding Rules

Source rules live in `src-tauri/src/rules.rs`. The tree walker that attaches flags lives in `src-tauri/src/heuristics.rs`. Dependency drift rules live in `src-tauri/src/deps_diff.rs`. Add focused Rust tests for every new rule and false-positive suppression.
