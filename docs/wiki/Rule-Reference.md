# Rule Reference

Diff Drift rules favor useful review prompts over complete static analysis. The security heuristics run over changed AST nodes across all supported source languages — TS/TSX/JS/JSX, Rust, Go, Python, Java, C#, Kotlin, and Swift — plus dependency drift for `package.json` and the Cargo, Go, PyPI, Maven, and NuGet manifests. Each rule runs for the language families where its concept exists: a rule about removed `try/catch` does not run on Go, a rule about a process-wide TLS env var runs only where that env var exists, and so on. The [Language coverage](#language-coverage) matrix records exactly which rule runs for which family. This is rule parity where the concept exists. It does not add new vulnerability classes per language, and it is not full static analysis.

Most code rules are AST-aware: the changed node is re-parsed with the file's tree-sitter grammar. JavaScript and TypeScript rules match compiled tree-sitter queries. The other languages match marker patterns over the parsed source, and how much text is masked first depends on the marker. Markers that are code — `eval(`, `InsecureSkipVerify`, `Process.Start`, a `child_process` call, a cookie's `httpOnly` flag — are matched with both comment and string interiors blanked, so naming one in a comment or a string never raises a flag. A few markers are themselves string values — a CORS `origin: "*"`, a `SameSite` value, the `PYTHONHTTPSVERIFY` env key, a regex constructor's pattern, a Go `import "os/exec"` module path — so for those only comments are blanked: a comment mention is ignored, but the same marker written inside an unrelated string can still match. Whether a marker is string-valued is decided per language family: a subprocess import is a string in Go (`import "os/exec"`) but pure code in Rust, Python, and Kotlin (`use std::process::Command`, `import subprocess`), a permissive-CORS marker is a quoted wildcard in most families but pure code in Rust (`CorsLayer::permissive()`, `.allow_origin(Any)`), and the `SameSite` markers are quoted values in most families but enum paths in Rust (`.same_site(SameSite::None)`). For the code-valued families those markers blank strings too, so the same token written inside a string never flags. Context gates keep the string-kept cases rare (a nearby `cors`/`cookie` token, an `environ` reference, an actual regex constructor in code must be present). Reformatting cannot evade either kind. When a structural parse fails, JavaScript and TypeScript rules fall back to a text pattern; for the other languages the rule stays silent rather than guess against a grammar it could not parse.

A `match type` column below records how each rule matches. Rules marked **differential** compare the node's before and after versions, which a snapshot scanner cannot do.

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

## Language coverage

Every code rule declares the language families it runs for. A rule is default-closed: it runs for JS/TS only unless it opts into more families, so an undeclared family never silently runs a rule. A rule runs for a family only where the concept exists in that language. There is no Go `try/catch` to remove, no Rust dynamic-eval primitive, and no Swift cookie-flag API in scope, so the gaps in the matrix below are deliberate. The dependency-drift rules are matched by manifest filename and are listed separately under [Dependency Drift Rules](#dependency-drift-rules-packagejson) and the per-ecosystem manifests.

`ship` means the rule runs for that family; `n/a` means the concept does not exist there (or is covered by another rule).

| Rule | JS/TS | Rust | Go | Python | Java | C# | Kotlin | Swift |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Hardcoded secret | ship | ship | ship | ship | ship | ship | ship | ship |
| Dynamic code execution (`eval`) | ship | n/a | n/a | ship | ship | ship | ship | n/a |
| `new Function` constructor | ship | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| Child process execution | ship | ship | ship | ship | ship | ship | ship | ship |
| Disabled TLS verification | ship | ship | ship | ship | ship | ship | ship | ship |
| TLS-disabling env var | ship | n/a | n/a | ship | n/a | n/a | n/a | n/a |
| Broadened CORS | ship | ship | ship | ship | ship | ship | ship | ship |
| Weakened cookie flags | ship | ship | ship | ship | ship | ship | ship | n/a |
| Loose regex pattern | ship | ship | ship | ship | ship | ship | ship | ship |
| Crypto downgrade (verify→decode) | ship | ship | ship | ship | ship | ship | ship | ship |
| Guard removed | ship | ship | ship | ship | ship | ship | ship | ship |
| Disabled guard (constant-falsy) | ship | ship | ship | ship | ship | ship | ship | ship |
| Removed sanitization | ship | ship | ship | ship | ship | ship | ship | ship |
| Error handling removed | ship | ship | n/a | ship | ship | ship | ship | ship |
| Permissive logging config | ship | n/a | n/a | n/a | n/a | n/a | n/a | n/a |
| Undeclared import | ship | n/a | n/a | n/a | n/a | n/a | n/a | n/a |

Dependency drift ships for six manifest ecosystems: npm (`package.json`), Cargo (`Cargo.toml`, vouched by `Cargo.lock`), Go (`go.mod`, vouched by `go.sum`), PyPI (`requirements.txt`, vouched by `poetry.lock`), Maven (`pom.xml`), and NuGet (`.csproj`, vouched by `packages.lock.json`). Maven and `.csproj` without a lockfile flag new and changed entries but cannot make a not-in-lockfile accusation.

### Known limits

These are the honest edges of the current cut. Each line is something a user might see and the reason it works that way. Flags are review prompts, so the conservative choice is to keep a rule grounded and let a reviewer judge rather than narrow it into silence.

1. A pure rename can read as removed code. On a single-node diff there is no rename detection, so renaming `cfg` to `tlsCfg`, `validateEmail` to `isValidEmail`, or `to_string` to `to_owned` next to a marker can surface a differential flag. The same applies to a signature change on an overloaded function: overloads are told apart by signature, so changing one (for example adding an annotation) reads as a removed and an added function rather than a modified one.
2. Kotlin guard-removed is silent on `?.let`, `takeIf?.also`, and `when`-branch guard idioms. It reads the consequences of an `if` block; these null-safe forms fall outside what it matches.
3. Rust error-handling-removed treats `?` becoming `.expect("msg")` as removed handling, because for error propagation it is.
4. verify→decode is intentionally broad: any verify-prefixed call renamed to a decode-prefixed call flags, including cross-domain pairs.
5. The Go and C# `verify*`/`sanitize*` callee match is lowercase or camelCase-prefix only, so an exported PascalCase `Validate`/`Sanitize` can be missed.
6. Swift TLS-disable matches on bare type and field names, so a type named `DisabledTrustEvaluator` or a field named `disableEvaluation` can flag without a real downgrade. Swift is the lowest-leverage family on a Windows-first build, so the marker stays grounded rather than over-narrowed.
7. Two rare cases stay grounded rather than narrowed: a Python `import subprocess` under `if TYPE_CHECKING:`, and a Kotlin `getEngineByName` call on a non-engine object can each flag.
8. Gradle (`build.gradle`/`build.gradle.kts`) and `Package.swift` dependency manifests are not shipped, because they are executable code rather than declarative manifests.
9. A handful of markers are string values by design (the quoted-wildcard CORS origin in Go/Java/Kotlin/Python/Swift, `SameSite` values, the `PYTHONHTTPSVERIFY` env key, a regex pattern, the Go `import "os/exec"` module path). For those rules a comment mention is ignored, but the same token written inside an unrelated string can still match. The context gates keep this rare, but it is the reason these rules cannot promise the full string-immunity the code-form markers have. Where a family's marker is pure code instead — Rust's subprocess import, its `CorsLayer::permissive()`/`.allow_origin(Any)` CORS marker, and its `.same_site(SameSite::…)` cookie markers — strings are blanked too, so the marker is string-immune there.

## False Positives

False positives are expected. Use dismiss when a flag is reviewed and not actionable. If a rule repeatedly flags normal code, open a GitHub Discussion or issue with the code shape and expected behavior.

## Adding Rules

Source rules live in `src-tauri/src/rules.rs`. The tree walker that attaches flags lives in `src-tauri/src/heuristics.rs`. Dependency drift rules live in `src-tauri/src/deps_diff.rs`. Add focused Rust tests for every new rule and false-positive suppression.
