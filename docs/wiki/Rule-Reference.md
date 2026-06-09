# Rule Reference

Diff Drift rules are small heuristics over changed TypeScript/TSX AST nodes. They favor useful review prompts over complete static analysis.

## Severity

- **High**: likely security-sensitive and should be reviewed before commit.
- **Medium**: suspicious or dependency-related drift that may be intentional.
- **Low**: weaker signal, usually a review reminder.

## Rules

| Rule | Severity | Triggers On | Notes |
| --- | --- | --- | --- |
| Hardcoded secret | High | AWS-style keys, OpenAI-style keys, or private key markers added to source | Suppressed in test-like files. Does not detect every secret format. |
| Dynamic code execution | High | Newly added `eval(...)` or `new Function(...)` | Flags code execution from strings. |
| Child process execution | High | Newly added `child_process` imports or subprocess calls | Suppressed in test-like files. |
| Disabled TLS verification | High | `rejectUnauthorized: false` or `NODE_TLS_REJECT_UNAUTHORIZED=0` introduced | Review for local-dev exceptions before dismissing. |
| Broadened CORS | High | CORS opened to `*` or `true` | Suppressed in test-like files. |
| Weakened cookie flags | High | Removal of `httpOnly`, `secure`, or weakening `sameSite` | Only fires on modified nodes where the before/after comparison shows weakening. |
| Loose regex pattern | High | Validation regex widened to catch-all forms such as `/.*/` | Best effort, not a full regex semantic analyzer. |
| Crypto downgrade | Medium | Verification-like call replaced with decode/parse-like call | Intended for token/JWT review prompts. |
| Undeclared import | Medium | New bare package import not declared in root `package.json` | Ignores relative imports, Node built-ins, and common path aliases. |
| Disabled guard | Low | Guard rewritten to `if (false)` | Review whether a check was intentionally bypassed. |
| Removed sanitization | Low | Sanitization, escaping, or validation call removed | Can produce review prompts for renames or refactors. |
| Permissive logging config | Low | Redaction emptied or log level lowered | Review before committing sensitive logging changes. |

## False Positives

False positives are expected. Use dismiss when a flag is reviewed and not actionable. If a rule repeatedly flags normal code, open a GitHub Discussion or issue with the code shape and expected behavior.

## Adding Rules

Rules live in `src-tauri/src/rules.rs`. The tree walker that attaches flags lives in `src-tauri/src/heuristics.rs`. Add focused Rust tests for every new rule and false-positive suppression.
