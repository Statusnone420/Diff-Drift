//! Security-rule registry. Each rule is a small, independently testable predicate
//! over a single diffed `AstNode`. Rules match on the node's STATE + its before/after
//! source `lines` (which carry the node's full subtree text — the parser surfaces
//! statements, not expression-level nodes), so they fire wherever a smell appears in
//! changed code. CWE references noted per rule.
use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use crate::lang::{self, ErrorHandlingStrategy};
use crate::model::{AstNode, NodeState, Severity};
use crate::parse::{Family, Lang, ALL_FAMILIES};

/// Per-file context a rule may consult.
pub struct RuleCtx {
    /// Package names declared in the repo's package.json (deps + devDeps).
    pub deps: HashSet<String>,
    /// Whether this file looks like a test/fixture (suppresses noisy rules).
    pub is_test_file: bool,
    /// Whether this file is a Cargo build script (`build.rs`). Build scripts
    /// legitimately shell out to `git`/`protoc`/`bindgen`, so `child-process` is
    /// suppressed for them — same treatment as a test path.
    pub is_build_script: bool,
    /// The file's language, for structural (tree-sitter query) matching.
    pub lang: Lang,
}

/// True when a repo-relative path is a Cargo build script (`build.rs`), at the
/// crate root or any nested crate. Build scripts shell out to tooling as a
/// matter of course, so the `child-process` rule treats them like test paths.
pub fn is_build_script_path(path: &str) -> bool {
    let slashed = path.replace('\\', "/");
    let file = slashed.rsplit('/').next().unwrap_or(&slashed);
    file.eq_ignore_ascii_case("build.rs")
}

pub struct Finding {
    pub severity: Severity,
    pub r#type: &'static str,
    pub desc: String,
    /// The specific line that triggered the flag, when a rule can point at one
    /// (e.g. the line holding a hardcoded secret). Surfaced in the report so a
    /// truncated node body never hides the evidence.
    pub evidence: Option<String>,
}

fn finding(severity: Severity, r#type: &'static str, desc: impl Into<String>) -> Option<Finding> {
    Some(Finding {
        severity,
        r#type,
        desc: desc.into(),
        evidence: None,
    })
}

fn finding_with_evidence(
    severity: Severity,
    r#type: &'static str,
    desc: impl Into<String>,
    evidence: impl Into<String>,
) -> Option<Finding> {
    Some(Finding {
        severity,
        r#type,
        desc: desc.into(),
        evidence: Some(evidence.into()),
    })
}

pub trait Rule: Send + Sync {
    fn id(&self) -> &'static str;
    /// The language families this rule runs for. Default-closed: JS/TS only —
    /// the historical behavior. A rule must explicitly opt into other families
    /// by overriding this; an undeclared family never runs the rule, and for an
    /// opted-in family with an empty pack field the rule still returns `None`
    /// silently (structural-or-nothing).
    fn families(&self) -> &'static [Family] {
        &[Family::JsTs]
    }
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding>;
}

pub struct RuleRegistry {
    rules: Vec<Box<dyn Rule>>,
}

impl RuleRegistry {
    pub fn new() -> Self {
        RuleRegistry {
            // First match wins per node — order higher-confidence / higher-severity first.
            rules: vec![
                Box::new(HardcodedSecret),
                Box::new(EvalCall),
                Box::new(FnConstructor),
                Box::new(ChildProcess),
                Box::new(TlsRejectFalse),
                Box::new(EnvTlsReject),
                Box::new(BroadenedCors),
                Box::new(CookieHttpOnlyRemoved),
                Box::new(CookieSecureRemoved),
                Box::new(SameSiteWeakened),
                Box::new(LooseRegex),
                Box::new(VerifyToDecode),
                Box::new(RemovedIfGuard),
                Box::new(RemovedSanitize),
                Box::new(PermissiveLogging),
                Box::new(UnvettedPackage),
                // Differential rules (before-vs-after comparisons) sit last so
                // they never shadow a higher-confidence single-state flag.
                Box::new(GuardRemoved),
                Box::new(ErrorHandlingRemoved),
            ],
        }
    }

    /// First matching rule's id + its finding.
    ///
    /// Each rule declares the families it applies to (`Rule::families`,
    /// default-closed to JS/TS). We run only the rules opted into the file's
    /// family, preserving the first-match-wins confidence ordering of `rules`.
    /// A rule that opts into a family but finds an empty pack field for it
    /// returns `None` (structural-or-nothing) — so an unfilled language pack
    /// produces silence, never a JS query compiled against a foreign grammar
    /// (structural.rs's per-grammar cache + debug_assert still guard that path).
    pub fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<(&'static str, Finding)> {
        let family = ctx.lang.family();
        self.rules
            .iter()
            .filter(|r| r.families().contains(&family))
            .find_map(|r| r.check(node, ctx).map(|f| (r.id(), f)))
    }
}

impl Default for RuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

static REGISTRY: LazyLock<RuleRegistry> = LazyLock::new(RuleRegistry::new);

pub fn registry() -> &'static RuleRegistry {
    &REGISTRY
}

// ---------- family applicability sets ----------
// Static `&[Family]` slices the `families()` overrides return. Kept here next to
// the registry so the applicability matrix reads in one place.

/// Every family except Swift — the weakened-cookie rules (cookies live in a
/// plist on Apple platforms, not in source).
const ALL_EXCEPT_SWIFT: &[Family] = &[
    Family::JsTs,
    Family::Rust,
    Family::Go,
    Family::Python,
    Family::Java,
    Family::CSharp,
    Family::Kotlin,
];

/// Every family except Go — `ErrorHandlingRemoved` (Go's error handling is
/// already covered by the guard rules).
const ALL_EXCEPT_GO: &[Family] = &[
    Family::JsTs,
    Family::Rust,
    Family::Python,
    Family::Java,
    Family::CSharp,
    Family::Kotlin,
    Family::Swift,
];

/// `EvalCall` — families with a real dynamic-code-execution primitive.
const EVAL_FAMILIES: &[Family] = &[
    Family::JsTs,
    Family::Python,
    Family::Java,
    Family::CSharp,
    Family::Kotlin,
];

/// `EnvTlsReject` — families with a process-wide TLS-disabling env var.
const ENV_TLS_FAMILIES: &[Family] = &[Family::JsTs, Family::Python];

// ---------- helpers ----------
/// Whether any of `markers` appears as a substring of `s`. Empty `markers` =>
/// false (the rule is silent for that family).
fn any_marker(s: &str, markers: &[&str]) -> bool {
    markers.iter().any(|m| s.contains(m))
}

/// Compile a marker regex source (from a family pack) once and cache it. The
/// sources come from `&'static str` pack fields, so they live for the process;
/// caching keyed by the source pointer/text avoids recompiling per call. A
/// source that fails to compile returns `None` — the rule then stays silent,
/// matching the structural layer's graceful-degradation contract. Marker
/// sources are exercised by each family's unit tests, so a broken source turns
/// CI red before release.
fn compiled_marker(src: &'static str) -> Option<std::sync::Arc<Regex>> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    /// Source text -> compiled regex (or `None` if the source failed to compile).
    type MarkerCache = HashMap<&'static str, Option<Arc<Regex>>>;
    static CACHE: OnceLock<Mutex<MarkerCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = match cache.lock() {
        Ok(g) => g,
        Err(p) => p.into_inner(),
    };
    guard
        .entry(src)
        .or_insert_with(|| {
            let r = Regex::new(src).ok().map(Arc::new);
            debug_assert!(r.is_some(), "rule marker regex failed to compile: {src}");
            r
        })
        .clone()
}

fn joined(lines: &Option<Vec<String>>) -> String {
    lines.as_ref().map(|l| l.join("\n")).unwrap_or_default()
}

/// The before/after snippet text with every comment body and string-literal
/// interior blanked, so a non-JsTs marker rule matches against CODE ONLY — a
/// marker token that appears solely inside a log line, docstring, error message,
/// or comment never fires the rule. Returns `None` on parse failure, which the
/// marker rules treat as "no answer" → the rule stays silent (the existing
/// structural-or-nothing degradation contract). The empty string maps to an
/// empty code-only string (no parse needed) so a missing `before` side stays
/// cheap and total.
fn code_only(lines: &Option<Vec<String>>, lang: Lang) -> Option<String> {
    let src = joined(lines);
    if src.is_empty() {
        return Some(String::new());
    }
    crate::structural::code_only_text(&src, lang)
}

/// Like [`code_only`] but blanks comment bodies ONLY (strings stay intact).
/// Used by the env-var TLS rule, whose marker (the env-var name) lives inside a
/// string key — so strings must survive, but a marker named only in a comment
/// must still be ignored.
fn code_minus_comments(lines: &Option<Vec<String>>, lang: Lang) -> Option<String> {
    let src = joined(lines);
    if src.is_empty() {
        return Some(String::new());
    }
    crate::structural::code_minus_comments_text(&src, lang)
}
fn strip_ws(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn added_or_modified(s: NodeState) -> bool {
    matches!(s, NodeState::Added | NodeState::Modified)
}

/// Whether `s` mentions a CORS construct. Used to gate the ambiguous non-JsTs
/// permissive-origin markers so they only fire on an actual CORS configuration,
/// not on an unrelated struct/builder that happens to expose a similarly named
/// field or method (a custom `ProxyConfig{AllowAllOrigins}`, a `RouteConfig`
/// with an `AllowAnyOrigin()` method, a non-CORS `anyHost()`).
///
/// Accepts any of these case-insensitive tokens, so frameworks whose marker
/// lacks the literal substring "cors" still register context:
/// - `cors` — gin `cors.Config`, ASP.NET `UseCors`/`CorsPolicyBuilder`, Ktor
///   `install(CORS)`, tower-http `CorsLayer`, Flask/FastAPI `CORSMiddleware`.
/// - `crossorigin` — Spring `@CrossOrigin`.
/// - `allowedorigin` (singular, so it also matches the plural `AllowedOrigins`)
///   — gorilla `handlers.AllowedOrigins`, Spring `addAllowedOrigin`/
///   `setAllowedOrigins`/`allowedOrigins`.
fn has_cors_context(s: &str) -> bool {
    let lower = s.to_ascii_lowercase();
    lower.contains("cors") || lower.contains("crossorigin") || lower.contains("allowedorigin")
}

/// Whether `s` mentions a cookie (case-insensitive `cookie`). Used to gate the
/// cookie-flag markers (`http_only`/`secure`/`http_only` builder methods) so a
/// `secure(true)`/`http_only(true)` on an unrelated builder (a `TlsConfig`, a
/// Ktor `sslConnector`, a `ServerConfig`) doesn't read as a weakened cookie.
fn has_cookie_context(s: &str) -> bool {
    s.to_ascii_lowercase().contains("cookie")
}

// ---------- rules ----------

/// CWE-798 — hardcoded credentials. Concrete markers only (AWS / OpenAI / PEM).
struct HardcodedSecret;
impl Rule for HardcodedSecret {
    fn id(&self) -> &'static str {
        "hardcoded-secret"
    }
    fn families(&self) -> &'static [Family] {
        // Genuinely language-neutral: plain AWS/OpenAI/PEM regex over the node's
        // after-text, no grammar assumptions — runs for every family.
        ALL_FAMILIES
    }
    fn check(&self, node: &AstNode, _ctx: &RuleCtx) -> Option<Finding> {
        // Unlike the other rules, secrets are NOT suppressed in test files: a
        // real key pasted into a fixture is still a leak, and the AWS/OpenAI/PEM
        // markers are specific enough that flagging them stays low-noise. The
        // analysis is drift-scoped, so this only fires on a secret an agent just
        // added — not the existing test corpus.
        if !added_or_modified(node.state) {
            return None;
        }
        static AWS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"AKIA[0-9A-Z]{16}").unwrap());
        static OPENAI: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\bsk-[A-Za-z0-9]{16,}").unwrap());
        static PEM: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"-----BEGIN [A-Z ]*PRIVATE KEY-----").unwrap());
        // Match per line so the report can point at the exact offending line —
        // these markers never span a line break.
        let (what, line) = node.after.iter().flatten().find_map(|l| {
            if AWS.is_match(l) {
                Some(("an AWS access key", l))
            } else if OPENAI.is_match(l) {
                Some(("an API key", l))
            } else if PEM.is_match(l) {
                Some(("a private key", l))
            } else {
                None
            }
        })?;
        finding_with_evidence(
            Severity::High,
            "Hardcoded secret",
            format!("{what} is hardcoded in source — move it to an environment variable or secret store."),
            line.trim(),
        )
    }
}

/// CWE-95 — code injection via eval. Structural: matches a real call to `eval`
/// (bare or via member like `window.eval`), never the word inside a string or
/// comment. Falls back to the text pattern when the snippet can't be parsed.
struct EvalCall;

const EVAL_CALL_QUERY: &str = r#"
(call_expression function: (identifier) @callee (#eq? @callee "eval"))
(call_expression
  function: (member_expression property: (property_identifier) @prop (#eq? @prop "eval")))
"#;

impl Rule for EvalCall {
    fn id(&self) -> &'static str {
        "eval-call"
    }
    fn families(&self) -> &'static [Family] {
        EVAL_FAMILIES
    }
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        // Like child-process and tls, suppress in test/fixture files: a dynamic
        // eval inside an integration test (e.g. a C# test of `CSharpScript`) is
        // test scaffolding, not production drift.
        if ctx.is_test_file || !added_or_modified(node.state) {
            return None;
        }
        let after = joined(&node.after);
        let before = joined(&node.before);
        if ctx.lang.family() == Family::JsTs {
            static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\beval\s*\(").unwrap());
            // Cheap pre-filter: no `eval` text means no parse needed.
            if !after.contains("eval") {
                return None;
            }
            let hit = |src: &str| {
                crate::structural::query_hit(src, ctx.lang, EVAL_CALL_QUERY)
                    .unwrap_or_else(|| RE.is_match(src))
            };
            if hit(&after) && !hit(&before) {
                return finding(
                    Severity::High,
                    "Dynamic code execution",
                    "`eval(…)` executes arbitrary code — a code-injection risk; use a safe alternative.",
                );
            }
            return None;
        }
        // Non-JsTs: marker-driven (e.g. Python eval/exec/compile). Empty pack
        // field => silent, no JS fallback. Match against CODE ONLY so the marker
        // token inside a string/comment (e.g. `# never call eval()`) is ignored;
        // a parse failure => silent.
        let marker = lang::pack(ctx.lang.family()).eval_call?;
        let re = compiled_marker(marker)?;
        let after = code_only(&node.after, ctx.lang)?;
        let before = code_only(&node.before, ctx.lang)?;
        if re.is_match(&after) && !re.is_match(&before) {
            return finding(
                Severity::High,
                "Dynamic code execution",
                "Dynamic code execution was introduced — executing built-from-strings code is a code-injection risk; use a safe alternative.",
            );
        }
        None
    }
}

/// CWE-95 — code injection via the Function constructor. Structural: a real
/// `new Function(…)` expression, not the words in a string or comment.
struct FnConstructor;

const FN_CONSTRUCTOR_QUERY: &str = r#"
(new_expression constructor: (identifier) @ctor (#eq? @ctor "Function"))
"#;

impl Rule for FnConstructor {
    fn id(&self) -> &'static str {
        "fn-constructor"
    }
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if !added_or_modified(node.state) {
            return None;
        }
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"new\s+Function\s*\(").unwrap());
        let after = joined(&node.after);
        if !after.contains("Function") {
            return None;
        }
        let hit = |src: &str| {
            crate::structural::query_hit(src, ctx.lang, FN_CONSTRUCTOR_QUERY)
                .unwrap_or_else(|| RE.is_match(src))
        };
        if hit(&after) && !hit(&joined(&node.before)) {
            return finding(
                Severity::High,
                "Dynamic code execution",
                "`new Function(…)` builds code from strings like `eval` — prefer a static function.",
            );
        }
        None
    }
}

/// CWE-78 — OS command execution via child_process.
struct ChildProcess;
impl Rule for ChildProcess {
    fn id(&self) -> &'static str {
        "child-process"
    }
    fn families(&self) -> &'static [Family] {
        ALL_FAMILIES
    }
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        // Suppress in test/fixture files and in Cargo build scripts: both
        // legitimately spawn subprocesses (test harnesses, `build.rs` calling
        // git/protoc/bindgen).
        if ctx.is_test_file || ctx.is_build_script || !added_or_modified(node.state) {
            return None;
        }
        let after = joined(&node.after);
        let before = joined(&node.before);
        if ctx.lang.family() == Family::JsTs {
            static IMPORT: LazyLock<Regex> = LazyLock::new(|| {
                Regex::new(r#"(require\s*\(\s*['"](?:node:)?child_process['"]|from\s+['"](?:node:)?child_process['"])"#).unwrap()
            });
            static CALL: LazyLock<Regex> =
                LazyLock::new(|| Regex::new(r"\.(exec|execSync|spawn|spawnSync)\s*\(").unwrap());
            if (IMPORT.is_match(&after) && !IMPORT.is_match(&before))
                || (CALL.is_match(&after) && !CALL.is_match(&before))
            {
                return finding(
                    Severity::High,
                    "Child process execution",
                    "Spawns a subprocess (`child_process`) — ensure arguments can't be attacker-controlled.",
                );
            }
            return None;
        }
        // Non-JsTs: marker-driven (per-family subprocess import + call regexes).
        // Either an import or a call newly appearing fires; empty fields => no
        // match => silent.
        //
        // CALL markers (`subprocess.run(`, `Process.Start`, `exec.Command(`,
        // `Command::new(`) are CODE, so they run against the fully code-only text
        // — a call named in a string OR comment is ignored.
        //
        // IMPORT markers split by whether the family's import path is a string.
        // Go's is (`import "os/exec"`), so Go's marker runs against the
        // comments-only text (strings kept) to see the quoted specifier — which
        // still drops an import token named in a `// …` comment. The
        // keyword-import families (Rust `use std::process::Command`, Python
        // `import subprocess`, Kotlin `import …`) have code-valued imports, so
        // their marker runs against the fully code-only text, giving it the same
        // string immunity the CALL markers have. The per-family
        // `subprocess_import_in_strings` flag selects the tier below. Parse
        // failure on either side => silent.
        let pack = lang::pack(ctx.lang.family());
        let after_code = code_only(&node.after, ctx.lang)?;
        let before_code = code_only(&node.before, ctx.lang)?;
        let after_nc = code_minus_comments(&node.after, ctx.lang)?;
        let before_nc = code_minus_comments(&node.before, ctx.lang)?;
        let newly = |marker: Option<&'static str>, after: &str, before: &str| -> bool {
            let Some(src) = marker else { return false };
            let Some(re) = compiled_marker(src) else {
                return false;
            };
            re.is_match(after) && !re.is_match(before)
        };
        // The import marker reads string-kept text ONLY for families whose
        // import paths are string-valued (Go's `import "os/exec"`). For
        // keyword-based imports (Rust/Python/Kotlin) the marker is code, so it
        // reads the fully code-masked text — the same token inside a string
        // literal (a doc example, a detector's own marker definition, a test
        // fixture string) is masked out and never fires.
        let (import_after, import_before) = if pack.subprocess_import_in_strings {
            (&after_nc, &before_nc)
        } else {
            (&after_code, &before_code)
        };
        if newly(pack.subprocess_import, import_after, import_before)
            || newly(pack.subprocess_call, &after_code, &before_code)
        {
            return finding(
                Severity::High,
                "Child process execution",
                "Spawns a subprocess — ensure arguments can't be attacker-controlled.",
            );
        }
        None
    }
}

/// CWE-295 — disabled TLS certificate validation. Structural: an object
/// property `rejectUnauthorized` (identifier or quoted) whose value is the
/// `false` literal — reformatting and quoting can't evade, strings can't
/// false-fire.
struct TlsRejectFalse;

const TLS_REJECT_QUERY: &str = r#"
(pair key: (property_identifier) @key (#eq? @key "rejectUnauthorized") value: (false))
(pair key: (string (string_fragment) @skey (#eq? @skey "rejectUnauthorized")) value: (false))
"#;

impl Rule for TlsRejectFalse {
    fn id(&self) -> &'static str {
        "tls-reject-false"
    }
    fn families(&self) -> &'static [Family] {
        ALL_FAMILIES
    }
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if ctx.is_test_file || !added_or_modified(node.state) {
            return None;
        }
        let after = joined(&node.after);
        let before = joined(&node.before);
        if ctx.lang.family() == Family::JsTs {
            static RE: LazyLock<Regex> =
                LazyLock::new(|| Regex::new(r"rejectUnauthorized\s*:\s*false").unwrap());
            if !after.contains("rejectUnauthorized") {
                return None;
            }
            let hit = |src: &str| {
                crate::structural::query_hit(src, ctx.lang, TLS_REJECT_QUERY)
                    .unwrap_or_else(|| RE.is_match(src))
            };
            if hit(&after) && !hit(&before) {
                return finding(
                    Severity::High,
                    "Disabled TLS verification",
                    "`rejectUnauthorized: false` turns off certificate validation — restore it.",
                );
            }
            return None;
        }
        // Non-JsTs: marker-driven (per-family TLS-disable markers). Empty field
        // => silent. Match against CODE ONLY so a marker named in a comment or
        // string (e.g. `// never set InsecureSkipVerify: true`) is ignored; a
        // parse failure => silent.
        let marker = lang::pack(ctx.lang.family()).tls_disable?;
        let re = compiled_marker(marker)?;
        let after = code_only(&node.after, ctx.lang)?;
        let before = code_only(&node.before, ctx.lang)?;
        if re.is_match(&after) && !re.is_match(&before) {
            return finding_with_evidence(
                Severity::High,
                "Disabled TLS verification",
                "Certificate validation was turned off — restore it.",
                after
                    .lines()
                    .find(|l| re.is_match(l))
                    .map(str::trim)
                    .unwrap_or("")
                    .to_string(),
            );
        }
        None
    }
}

/// CWE-295 — disabled TLS via env var.
struct EnvTlsReject;
impl Rule for EnvTlsReject {
    fn id(&self) -> &'static str {
        "env-tls-reject"
    }
    fn families(&self) -> &'static [Family] {
        ENV_TLS_FAMILIES
    }
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if !added_or_modified(node.state) {
            return None;
        }
        let after = joined(&node.after);
        let before = joined(&node.before);
        if ctx.lang.family() == Family::JsTs {
            static RE: LazyLock<Regex> = LazyLock::new(|| {
                Regex::new(r#"NODE_TLS_REJECT_UNAUTHORIZED\s*=\s*['"]?0"#).unwrap()
            });
            if RE.is_match(&after) && !RE.is_match(&before) {
                return finding(
                    Severity::High,
                    "Disabled TLS verification",
                    "`NODE_TLS_REJECT_UNAUTHORIZED=0` disables TLS verification process-wide — remove it.",
                );
            }
            return None;
        }
        // Non-JsTs (Python): marker-driven env-var disable. Empty field =>
        // silent. The env-var name lives inside a STRING key
        // (`os.environ['PYTHONHTTPSVERIFY'] = '0'`), so we blank comments only —
        // strings must survive for the marker to match — which still drops a
        // marker named in a `# …` comment. Parse failure => silent.
        let marker = lang::pack(ctx.lang.family()).env_tls_disable?;
        let re = compiled_marker(marker)?;
        let after = code_minus_comments(&node.after, ctx.lang)?;
        let before = code_minus_comments(&node.before, ctx.lang)?;
        // Because strings survive, the marker also matches the env-var name in a
        // bare DOC string (`MSG = "Do not set PYTHONHTTPSVERIFY=0"`). The real
        // disable is always an `os.environ[...]` assignment, so require an
        // `environ` token in the after — a prose string that merely names the var
        // (without `environ`) is then ignored.
        if !after.to_ascii_lowercase().contains("environ") {
            return None;
        }
        if re.is_match(&after) && !re.is_match(&before) {
            return finding(
                Severity::High,
                "Disabled TLS verification",
                "An environment variable that disables TLS verification process-wide was introduced — remove it.",
            );
        }
        None
    }
}

/// CWE-942 — overly permissive CORS. Structural: an `origin` property whose
/// value is the `true` literal or the `"*"` string.
struct BroadenedCors;

const CORS_QUERY: &str = r#"
(pair key: (property_identifier) @key (#eq? @key "origin") value: (true))
(pair key: (property_identifier) @key (#eq? @key "origin")
  value: (string (string_fragment) @v (#eq? @v "*")))
(pair key: (property_identifier) @key (#eq? @key "origin")
  value: (array (string (string_fragment) @av (#eq? @av "*"))))
(pair key: (string (string_fragment) @skey (#eq? @skey "origin")) value: (true))
(pair key: (string (string_fragment) @skey (#eq? @skey "origin"))
  value: (string (string_fragment) @sv (#eq? @sv "*")))
(pair key: (string (string_fragment) @skey (#eq? @skey "origin"))
  value: (array (string (string_fragment) @asv (#eq? @asv "*"))))
"#;

impl Rule for BroadenedCors {
    fn id(&self) -> &'static str {
        "broadened-cors"
    }
    fn families(&self) -> &'static [Family] {
        ALL_FAMILIES
    }
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if ctx.is_test_file || !added_or_modified(node.state) {
            return None;
        }
        let after = joined(&node.after);
        let before = joined(&node.before);
        if ctx.lang.family() == Family::JsTs {
            static WILDCARD: LazyLock<Regex> =
                LazyLock::new(|| Regex::new(r#"origin\s*:\s*['"]\*['"]"#).unwrap());
            static TRUE: LazyLock<Regex> =
                LazyLock::new(|| Regex::new(r"origin\s*:\s*true").unwrap());
            if !after.contains("origin") {
                return None;
            }
            let hit = |src: &str| {
                crate::structural::query_hit(src, ctx.lang, CORS_QUERY)
                    .unwrap_or_else(|| WILDCARD.is_match(src) || TRUE.is_match(src))
            };
            if hit(&after) && !hit(&before) {
                return finding(
                    Severity::High,
                    "Broadened CORS",
                    "CORS origin was opened to any site (`*`/`true`) — credentials can leak; use an allowlist.",
                );
            }
            return None;
        }
        // Non-JsTs: marker-driven (per-framework permissive-origin markers).
        // Empty field => silent. The permissive-origin value is usually a quoted
        // wildcard (`"*"`), so by default blank COMMENTS only — strings must
        // survive for the marker to match — which still drops a marker named in a
        // `// …` comment. A family whose marker is pure code (Rust's `Any` type /
        // `permissive()` constructors, never a string) opts out via
        // `cors_permissive_in_strings: false` and reads fully code-masked text,
        // so the same call named inside a string literal never fires. Parse
        // failure => silent.
        let pack = lang::pack(ctx.lang.family());
        let marker = pack.cors_permissive?;
        let re = compiled_marker(marker)?;
        let (after, before) = if pack.cors_permissive_in_strings {
            (
                code_minus_comments(&node.after, ctx.lang)?,
                code_minus_comments(&node.before, ctx.lang)?,
            )
        } else {
            (
                code_only(&node.after, ctx.lang)?,
                code_only(&node.before, ctx.lang)?,
            )
        };
        // Every non-JsTs permissive-origin marker is ambiguous enough that a
        // custom struct/builder can expose the same field or method name: Go's
        // `AllowAllOrigins` on a `ProxyConfig`, C#'s `AllowAnyOrigin()` on a
        // `RouteConfig`, Kotlin's `anyHost()` on a non-CORS builder, Rust's
        // `.allow_origin(Any)`. Require an explicit CORS token in the node so
        // only an actual CORS configuration fires. `has_cors_context` accepts the
        // framework markers that lack the literal "cors" substring (Spring's
        // `@CrossOrigin`, gorilla's `AllowedOrigins`), so real detections keep
        // firing. JS/TS keeps its own structural path above and is exempt.
        let context_ok = ctx.lang.family() == Family::JsTs || has_cors_context(&after);
        if context_ok && re.is_match(&after) && !re.is_match(&before) {
            return finding(
                Severity::High,
                "Broadened CORS",
                "Cross-origin access was opened to any site — credentials can leak; use an allowlist.",
            );
        }
        None
    }
}

/// CWE-1004 — cookie missing HttpOnly.
struct CookieHttpOnlyRemoved;
impl Rule for CookieHttpOnlyRemoved {
    fn id(&self) -> &'static str {
        "cookie-httponly-removed"
    }
    fn families(&self) -> &'static [Family] {
        ALL_EXCEPT_SWIFT
    }
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if ctx.is_test_file || node.state != NodeState::Modified {
            return None;
        }
        let before = joined(&node.before);
        let after = joined(&node.after);
        if ctx.lang.family() == Family::JsTs {
            static RE: LazyLock<Regex> =
                LazyLock::new(|| Regex::new(r"httpOnly\s*:\s*true").unwrap());
            if RE.is_match(&before) && !RE.is_match(&after) {
                return finding(
                    Severity::High,
                    "Weakened cookie flags",
                    "Cookie `httpOnly` was removed — scripts can now read the cookie (XSS theft risk).",
                );
            }
            return None;
        }
        let marker = lang::pack(ctx.lang.family()).cookie_httponly?;
        let re = compiled_marker(marker)?;
        // Code-only (ignore the marker in a string/comment); parse failure =>
        // silent. Require cookie context so an `http_only(true)` builder method
        // on an unrelated config (not a cookie) doesn't read as a cookie flag.
        let before = code_only(&node.before, ctx.lang)?;
        let after = code_only(&node.after, ctx.lang)?;
        if has_cookie_context(&before) && re.is_match(&before) && !re.is_match(&after) {
            return finding(
                Severity::High,
                "Weakened cookie flags",
                "Cookie HttpOnly was removed — scripts can now read the cookie (XSS theft risk).",
            );
        }
        None
    }
}

/// CWE-614 — cookie missing Secure.
struct CookieSecureRemoved;
impl Rule for CookieSecureRemoved {
    fn id(&self) -> &'static str {
        "cookie-secure-removed"
    }
    fn families(&self) -> &'static [Family] {
        ALL_EXCEPT_SWIFT
    }
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if ctx.is_test_file || node.state != NodeState::Modified {
            return None;
        }
        let before = joined(&node.before);
        let after = joined(&node.after);
        if ctx.lang.family() == Family::JsTs {
            static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"secure\s*:\s*true").unwrap());
            if RE.is_match(&before) && !RE.is_match(&after) {
                return finding(
                    Severity::High,
                    "Weakened cookie flags",
                    "Cookie `secure` was removed — the cookie may now be sent over plain HTTP.",
                );
            }
            return None;
        }
        let marker = lang::pack(ctx.lang.family()).cookie_secure?;
        let re = compiled_marker(marker)?;
        // Code-only (ignore the marker in a string/comment); parse failure =>
        // silent. Require cookie context so a `.secure(true)` builder method on
        // an unrelated config (a TlsConfig, a Ktor sslConnector) doesn't read as
        // a cookie flag.
        let before = code_only(&node.before, ctx.lang)?;
        let after = code_only(&node.after, ctx.lang)?;
        if has_cookie_context(&before) && re.is_match(&before) && !re.is_match(&after) {
            return finding(
                Severity::High,
                "Weakened cookie flags",
                "Cookie Secure was removed — the cookie may now be sent over plain HTTP.",
            );
        }
        None
    }
}

/// CWE-1275 — SameSite downgraded.
struct SameSiteWeakened;
impl Rule for SameSiteWeakened {
    fn id(&self) -> &'static str {
        "samesite-weakened"
    }
    fn families(&self) -> &'static [Family] {
        ALL_EXCEPT_SWIFT
    }
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if ctx.is_test_file || node.state != NodeState::Modified {
            return None;
        }
        let before = joined(&node.before);
        let after = joined(&node.after);
        if ctx.lang.family() == Family::JsTs {
            static STRONG: LazyLock<Regex> = LazyLock::new(|| {
                Regex::new(r#"sameSite\s*:\s*['"]?(Strict|Lax|strict|lax)"#).unwrap()
            });
            static NONE: LazyLock<Regex> =
                LazyLock::new(|| Regex::new(r#"sameSite\s*:\s*['"]?(None|none)"#).unwrap());
            if STRONG.is_match(&before) && NONE.is_match(&after) {
                return finding(
                    Severity::High,
                    "Weakened cookie flags",
                    "Cookie `sameSite` was downgraded to `None` — restores CSRF exposure; use Lax or Strict.",
                );
            }
            return None;
        }
        // Non-JsTs: a strong SameSite marker in the before and a weak one in the
        // after. Both must be present in the pack; either empty => silent.
        let pack = lang::pack(ctx.lang.family());
        let (Some(strong_src), Some(weak_src)) =
            (pack.cookie_samesite, pack.cookie_samesite_weak)
        else {
            return None;
        };
        let (Some(strong), Some(weak)) =
            (compiled_marker(strong_src), compiled_marker(weak_src))
        else {
            return None;
        };
        // For most families the SameSite value is a string literal
        // (`"None"`/`"Strict"`/`"Lax"`), so blank COMMENTS only — strings must
        // survive for the markers to match. A family whose markers are pure code
        // (Rust's `SameSite::None` enum path, never a string) opts out via
        // `cookie_samesite_in_strings: false` and reads fully code-masked text,
        // so the same call written inside a string literal never fires. Require
        // cookie context so a `sameSite`-shaped attribute on an unrelated
        // builder doesn't read as a cookie downgrade. Parse failure => silent.
        let (before, after) = if pack.cookie_samesite_in_strings {
            (
                code_minus_comments(&node.before, ctx.lang)?,
                code_minus_comments(&node.after, ctx.lang)?,
            )
        } else {
            (
                code_only(&node.before, ctx.lang)?,
                code_only(&node.after, ctx.lang)?,
            )
        };
        if has_cookie_context(&before) && strong.is_match(&before) && weak.is_match(&after) {
            return finding(
                Severity::High,
                "Weakened cookie flags",
                "Cookie SameSite was downgraded to None — restores CSRF exposure; use Lax or Strict.",
            );
        }
        None
    }
}

/// Validation regex loosened. Differential: for Modified nodes the before and
/// after regex literals are extracted structurally and compared pairwise —
/// widening to a catch-all, dropping anchors, or unbounding a quantifier all
/// flag, with the description naming exactly what weakened. Added nodes keep
/// the catch-all check (no before to compare).
struct LooseRegex;

const REGEX_LITERAL_QUERY: &str = "(regex) @re";

fn regex_literals(src: &str, lang: Lang) -> Option<Vec<String>> {
    crate::structural::capture_texts(src, lang, REGEX_LITERAL_QUERY, "re")
}

/// The pattern body of a regex literal: `/pat/flags` → `pat`.
fn regex_pattern(lit: &str) -> &str {
    let body = lit.strip_prefix('/').unwrap_or(lit);
    match body.rfind('/') {
        Some(i) => &body[..i],
        None => body,
    }
}

fn is_catch_all_pattern(pat: &str) -> bool {
    matches!(
        pat,
        ".*" | ".+"
            | "^.*$" | "^.+$"
            | "[\\s\\S]*" | "[\\s\\S]+"
            | "^[\\s\\S]*$" | "^[\\s\\S]+$"
            | "[^]*" | "[^]+"
            | "^[^]*$" | "^[^]+$"
    )
}

fn has_anchors(pat: &str) -> bool {
    pat.starts_with('^') || (pat.ends_with('$') && !pat.ends_with("\\$"))
}

/// A quantifier with an upper limit (`{n}` or `{n,m}`). `{n,}` and `{0,}` are
/// unbounded — losing the upper bound is a weakening, so they don't count.
fn has_bounded_quantifier(pat: &str) -> bool {
    static BOUND: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\d+(,\d+)?\}").unwrap());
    BOUND.is_match(pat)
}

/// An unbounded quantifier: `*`, `+`, or `{n,}` with no upper limit.
fn has_unbounded_quantifier(pat: &str) -> bool {
    static OPEN: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\d+,\}").unwrap());
    pat.contains('*') || pat.contains('+') || OPEN.is_match(pat)
}

/// What weakened between two versions of the same regex literal, if anything.
fn regex_weakening(before: &str, after: &str) -> Option<String> {
    let (b, a) = (regex_pattern(before), regex_pattern(after));
    if b == a {
        return None;
    }
    if is_catch_all_pattern(a) && !is_catch_all_pattern(b) {
        return Some(format!(
            "widened to {after} — any string now passes validation"
        ));
    }
    if has_anchors(b) && !has_anchors(a) {
        return Some(format!(
            "lost its anchors (was {before}, now {after}) — partial matches now pass"
        ));
    }
    if has_bounded_quantifier(b) && !has_bounded_quantifier(a) && has_unbounded_quantifier(a) {
        return Some(format!(
            "lost its length bound (was {before}, now {after}) — unbounded input now passes"
        ));
    }
    None
}

/// Pattern bodies of every *real-code* regex-construction call (ISSUE 8). The
/// constructor is code, but its pattern argument is a string, so neither
/// `code_only` (string interiors blanked) nor `code_minus_comments` (strings
/// kept) alone is enough:
/// - `code_minus_comments` keeps the pattern literal so the body is extractable,
///   but it also keeps a constructor mentioned inside a STRING (a help/docs line);
/// - `code_only` blanks string interiors (so a string-embedded constructor's
///   call head is gone) but also blanks the genuine pattern body.
///
/// Both views are byte-length-preserving and offset-aligned. So: find each marker
/// match in `code_minus_comments` (the pattern survives there) and keep only those
/// whose match START byte is still real code in `code_only` (not a blanked string
/// interior). A constructor in a comment never matches either view; a constructor
/// inside a string matches `code_minus_comments` but its start byte is blank in
/// `code_only`, so it is dropped. Parse failure on either view falls back to the
/// raw text (structural-or-nothing degradation).
fn real_regex_call_patterns(lines: &Option<Vec<String>>, lang: Lang, marker: &Regex) -> Vec<String> {
    let raw = joined(lines);
    let kept = code_minus_comments(lines, lang).unwrap_or_else(|| raw.clone());
    let code = code_only(lines, lang).unwrap_or_else(|| raw.clone());
    let code_bytes = code.as_bytes();
    marker
        .captures_iter(&kept)
        .filter(|c| {
            // The whole-match start: the constructor call head. Real code keeps it
            // non-blank in `code_only`; a string-embedded mention is spaced out.
            c.get(0).is_some_and(|m| {
                code_bytes
                    .get(m.start())
                    .is_some_and(|b| !b.is_ascii_whitespace())
            })
        })
        .filter_map(|c| c.get(1).map(|m| m.as_str().to_string()))
        .collect()
}

/// Run the shared added/edited weakening comparison over two sets of regex
/// *pattern bodies* (already unwrapped from their literal/string form). Returns
/// the finding description, if any. Shared by the JS literal path (after
/// stripping `/…/`) and the marker path.
fn regex_weakening_over_patterns(before_pats: &[String], after_pats: &[String]) -> Option<String> {
    let added: Vec<&String> = after_pats.iter().filter(|a| !before_pats.contains(a)).collect();
    let removed: Vec<&String> = before_pats.iter().filter(|b| !after_pats.contains(b)).collect();
    let before_had_catch_all = before_pats.iter().any(|b| is_catch_all_pattern(b));
    if !before_had_catch_all {
        for a in &added {
            if is_catch_all_pattern(a) {
                return Some(format!(
                    "Validation regex was widened to {a} — any string now passes validation."
                ));
            }
        }
    }
    if removed.len() == 1 && added.len() == 1 {
        // Compare the single edited pair. `regex_weakening` strips `/…/`; these
        // are already bare patterns, so pass them through unchanged (no slashes
        // means `regex_pattern` returns them as-is).
        if let Some(weakened) = regex_weakening(removed[0], added[0]) {
            return Some(format!("Validation regex {weakened}."));
        }
    }
    None
}

impl Rule for LooseRegex {
    fn id(&self) -> &'static str {
        "loose-regex"
    }
    fn families(&self) -> &'static [Family] {
        ALL_FAMILIES
    }
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if !added_or_modified(node.state) || node.kind != "VariableDeclaration" {
            return None;
        }
        if ctx.lang.family() != Family::JsTs {
            // Non-JsTs: regexes are string arguments to a construction call
            // (`Regex::new("…")`, `re.compile("…")`). The pack marker captures
            // the pattern body in group 1; empty field => silent.
            let marker_src = lang::pack(ctx.lang.family()).regex_compile?;
            let marker = compiled_marker(marker_src)?;
            // ISSUE 8: only real-code constructors count. A `Regex::new(".*")`
            // named inside a comment or a string (a help/docs line) is not a real
            // regex construction and must not raise a High flag. `real_regex_call_
            // patterns` gates on the code-only view (string/comment interiors
            // blanked) while extracting the pattern body from the strings-kept view.
            let after_pats = real_regex_call_patterns(&node.after, ctx.lang, &marker);
            if after_pats.is_empty() {
                return None;
            }
            let before_pats = real_regex_call_patterns(&node.before, ctx.lang, &marker);
            return regex_weakening_over_patterns(&before_pats, &after_pats)
                .and_then(|desc| finding(Severity::High, "Loose regex pattern", desc));
        }
        let after = joined(&node.after);
        // Cheap pre-filter: a widened/weakened regex must appear in the after
        // version, which requires a `/` delimiter.
        if !after.contains('/') {
            return None;
        }
        let before = joined(&node.before);
        // Differential path: compare only the literals that actually changed.
        // A literal present in both versions (even at a moved position) is
        // unchanged, so set difference — not positional pairing — avoids
        // misreading a reorder as a weakening.
        if node.state == NodeState::Modified {
            if let (Some(bl), Some(al)) = (
                regex_literals(&before, ctx.lang),
                regex_literals(&after, ctx.lang),
            ) {
                let added: Vec<&String> = al.iter().filter(|a| !bl.contains(a)).collect();
                let removed: Vec<&String> = bl.iter().filter(|b| !al.contains(b)).collect();
                // A newly introduced catch-all is a widening — but only if the
                // node wasn't already catch-all (catch-all → catch-all is no
                // new weakening).
                let before_had_catch_all =
                    bl.iter().any(|b| is_catch_all_pattern(regex_pattern(b)));
                if !before_had_catch_all {
                    for a in &added {
                        if is_catch_all_pattern(regex_pattern(a)) {
                            return finding(
                                Severity::High,
                                "Loose regex pattern",
                                format!("Validation regex was widened to {a} — any string now passes validation."),
                            );
                        }
                    }
                }
                // Exactly one literal edited: compare that single pair.
                if removed.len() == 1 && added.len() == 1 {
                    if let Some(weakened) = regex_weakening(removed[0], added[0]) {
                        return finding(
                            Severity::High,
                            "Loose regex pattern",
                            format!("Validation regex {weakened}."),
                        );
                    }
                }
                return None;
            }
        }
        // Added nodes (and structural-parse fallback): catch-all in after, none before.
        if let Some(rx) = permissive_regex(&after) {
            if before.is_empty() || permissive_regex(&before).is_none() {
                return finding(
                    Severity::High,
                    "Loose regex pattern",
                    format!(
                        "Validation regex was widened to {rx} — any string now passes validation."
                    ),
                );
            }
        }
        None
    }
}

/// CWE-347 — signature verification replaced by a non-verifying decode/parse.
/// Structural: compares the actual callee names (bare or member calls) between
/// before and after, so `verify` in a string or comment can't confuse it.
struct VerifyToDecode;

/// Every call's callee name: `verify(…)` and `jwt.verify(…)` both capture `verify`.
const CALLEE_QUERY: &str = r#"
(call_expression function: (identifier) @callee)
(call_expression function: (member_expression property: (property_identifier) @callee))
"#;

/// The callee query for a family: the JS/TS const for `JsTs`, otherwise the
/// family pack's `@callee` query. `None` => the family has no grounded query, so
/// every callee-based rule returns `None` (silent) for it.
fn callee_query(lang: Lang) -> Option<&'static str> {
    match lang.family() {
        Family::JsTs => Some(CALLEE_QUERY),
        f => lang::pack(f).callee,
    }
}

fn callee_list(src: &str, lang: Lang) -> Option<Vec<String>> {
    crate::structural::capture_texts(src, lang, callee_query(lang)?, "callee")
}

fn callee_names(src: &str, lang: Lang) -> Option<HashSet<String>> {
    callee_list(src, lang).map(|names| names.into_iter().collect())
}

fn count_of(names: &[String], callee: &str) -> usize {
    names.iter().filter(|n| *n == callee).count()
}

impl Rule for VerifyToDecode {
    fn id(&self) -> &'static str {
        "verify-to-decode"
    }
    fn families(&self) -> &'static [Family] {
        ALL_FAMILIES
    }
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if node.state != NodeState::Modified {
            return None;
        }
        let before = joined(&node.before);
        // Cheap pre-filter: the downgrade requires a verify/sign call in the
        // before version — skip the structural parse otherwise.
        if !before.contains("verify") && !before.contains("sign") {
            return None;
        }
        let after = joined(&node.after);
        let fired = match (callee_names(&before, ctx.lang), callee_names(&after, ctx.lang)) {
            (Some(b), Some(a)) => {
                // `verify*` covers async/library variants (`verifyAsync`); for
                // signing, only the exact crypto names — `signIn`/`signOut`/
                // `signal` start with "sign" but aren't signature operations.
                let verifies = |s: &HashSet<String>| {
                    s.iter().any(|n| {
                        n.starts_with("verify") || n == "sign" || n == "signAsync" || n == "signSync"
                    })
                };
                // `decode*` (decodeJwt, decodeAsync) counts; bare `parse` counts
                // only as an exact name, never the generic `parseInt`/`parseFloat`.
                let decodes = |s: &HashSet<String>| {
                    s.iter()
                        .any(|n| n.starts_with("decode") || n == "parse" || n == "parseJwt" || n == "parseToken")
                };
                // Only a downgrade if the non-verifying decode/parse is NEW —
                // code that already decoded before isn't regressing here.
                verifies(&b) && !verifies(&a) && decodes(&a) && !decodes(&b)
            }
            // JS/TS-only text fallback when the structural parse can't answer.
            // For non-JsTs families this stays structural-or-nothing: no JS
            // regex ever runs against foreign syntax.
            _ if ctx.lang.family() == Family::JsTs => {
                static VERIFY: LazyLock<Regex> =
                    LazyLock::new(|| Regex::new(r"\b(verify|sign)\w*\s*\(").unwrap());
                // `decode*` or exactly `parse(` — never `parseInt(`/`parseFloat(`.
                static DECODE: LazyLock<Regex> =
                    LazyLock::new(|| Regex::new(r"\b(decode\w*|parse)\s*\(").unwrap());
                VERIFY.is_match(&before)
                    && !VERIFY.is_match(&after)
                    && DECODE.is_match(&after)
                    && !DECODE.is_match(&before)
            }
            _ => false,
        };
        if fired {
            return finding(
                Severity::Medium,
                "Crypto downgrade",
                "A signature `verify`/`sign` was replaced with a non-verifying `decode`/`parse` — forged tokens may pass.",
            );
        }
        None
    }
}

/// A guard clause neutralised to a constant-falsy condition. Structural:
/// `if (false)`, `if (0)`, `if (null)`, and `if (undefined)` all match —
/// closing the literal-`false`-only gap of the old text pattern.
struct RemovedIfGuard;

// The condition is captured with a wildcard and the falsy check happens in
// Rust: `undefined` is a dedicated node kind in the TS grammar but a plain
// identifier in the JS grammar, so naming node kinds here would not compile
// across both.
const IF_CONDITION_QUERY: &str = r#"
(if_statement condition: (parenthesized_expression (_) @cond))
"#;

/// The if-condition query for a family: JS/TS const for `JsTs`, else the pack's
/// `@cond` query. `None` => the rule is silent for the family.
fn if_condition_query(lang: Lang) -> Option<&'static str> {
    match lang.family() {
        Family::JsTs => Some(IF_CONDITION_QUERY),
        f => lang::pack(f).if_condition,
    }
}

/// JS/TS constant-falsy condition texts. Other families bring their own list via
/// the pack (`falsy_literals`).
const JS_FALSY: &[&str] = &["false", "0", "null", "undefined"];

fn has_const_falsy_guard(src: &str, lang: Lang) -> Option<bool> {
    let conds = crate::structural::capture_texts(src, lang, if_condition_query(lang)?, "cond")?;
    let falsy: &[&str] = match lang.family() {
        Family::JsTs => JS_FALSY,
        f => lang::pack(f).falsy_literals,
    };
    Some(conds.iter().any(|c| falsy.contains(&c.trim())))
}

impl Rule for RemovedIfGuard {
    fn id(&self) -> &'static str {
        "removed-if-guard"
    }
    fn families(&self) -> &'static [Family] {
        ALL_FAMILIES
    }
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        // Any Modified node qualifies: exported functions aren't split into
        // body-level child nodes, so the neutralised `if` may sit anywhere in
        // the node's subtree. Structural matching keeps this precise.
        if node.state != NodeState::Modified {
            return None;
        }
        let is_jsts = ctx.lang.family() == Family::JsTs;
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"if\s*\(\s*false\s*\)").unwrap());
        let hit = |src: &str| {
            match has_const_falsy_guard(src, ctx.lang) {
                Some(h) => h,
                // JS/TS text fallback only; non-JsTs stays structural-or-nothing.
                None if is_jsts => RE.is_match(src),
                None => false,
            }
        };
        if hit(&joined(&node.after)) && !hit(&joined(&node.before)) {
            return finding(
                Severity::Low,
                "Disabled guard",
                "A guard condition was replaced with a constant-falsy value — the check no longer runs.",
            );
        }
        None
    }
}

/// A sanitization / validation call removed outright. Structural: compares the
/// actual callee names, so the words in strings or comments neither flag nor
/// mask. Catches wrapper-stripping too (`save(sanitize(x))` → `save(x)`).
struct RemovedSanitize;

fn has_sanitizer_callee(names: &HashSet<String>) -> bool {
    names.iter().any(|n| is_sanitizer_name(n))
}

/// Whether some sanitizer-prefixed callee occurs FEWER times in `after` than in
/// `before` — i.e. a specific sanitize/escape/validate call was dropped. Counts
/// per name (multiplicity), so a genuinely removed sanitizer still registers even
/// when a *different* sanitizer-prefixed callee survives
/// (`sanitizeHtml(x); validateSchema(x);` → `validateSchema(x);` drops
/// `sanitizeHtml`). A pure rename of the surrounding code that keeps the same
/// sanitizer callee leaves every count unchanged and does not register.
fn sanitizer_callee_dropped(before: &[String], after: &[String]) -> bool {
    before
        .iter()
        .filter(|n| is_sanitizer_name(n))
        .any(|name| count_of(after, name) < count_of(before, name))
}

/// Whether an identifier (bare or member, last segment) has a sanitizer prefix.
fn is_sanitizer_name(n: &str) -> bool {
    n.rsplit('.').next().is_some_and(|last| {
        last.starts_with("sanitize") || last.starts_with("escape") || last.starts_with("validate")
    })
}

/// Whether a sanitizer-prefixed identifier (`sanitize*`/`escape*`/`validate*`)
/// still appears in `src` as a *definition header*. Used by `RemovedSanitize` to
/// suppress validator-definition churn inside a container node (the Alamofire
/// `extension DataRequest { func validate … }` case): if the validator's own
/// declaration survives, the sanitization moved or re-shaped rather than
/// disappeared.
///
/// Only a *definition header* counts — a declaration keyword (`func`/`fn`/`def`/
/// `fun`/`void`/`sub`) immediately preceding the sanitizer name, covering
/// languages whose function decl puts the name after a keyword. A surviving
/// *call/reference* deliberately does NOT count: a genuinely removed sanitizer
/// alongside a different surviving sanitizer-prefixed CALL (e.g.
/// `sanitizeHtml(x); validateSchema(x);` → `validateSchema(x);`) must still fire.
///
/// `src` must already be code-only (comment/string interiors blanked) so a name
/// mentioned in prose never falsely suppresses a real removal.
fn sanitizer_name_survives(src: &str) -> bool {
    static DEF: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"\b(?:func|fn|def|fun|void|sub)\s+(?:sanitize|escape|validate)\w*").unwrap()
    });
    DEF.is_match(src)
}

/// Function/method-declaration node kinds across the families. A node of one of
/// these kinds whose own name is sanitizer-prefixed is a *validator definition*,
/// not a caller — see `RemovedSanitize`'s definition-churn guard.
fn is_function_decl_kind(kind: &str) -> bool {
    matches!(
        kind,
        "FunctionDeclaration" | "MethodDeclaration" | "ExportDeclaration"
    )
}

impl Rule for RemovedSanitize {
    fn id(&self) -> &'static str {
        "removed-sanitize"
    }
    fn families(&self) -> &'static [Family] {
        ALL_FAMILIES
    }
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        static RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\b(sanitize|escape|validate)\w*\s*\(").unwrap());
        let before = joined(&node.before);
        // Cheap pre-filter: the call must have existed in the before version.
        if !before.contains("sanitize") && !before.contains("escape") && !before.contains("validate")
        {
            return None;
        }
        let is_jsts = ctx.lang.family() == Family::JsTs;
        let after = joined(&node.after);
        // FIX 3: a validator DEFINITION churning is not a removed caller-side
        // sanitizer. When the matched node is itself a function/method named with
        // a sanitizer prefix (`validate*`/`sanitize*`/`escape*`) and the after
        // still references that name (its own header / a self-call survives), a
        // signature- or annotation-only change (e.g. Alamofire's `validate`
        // gaining `@Sendable`) must not read as "sanitization removed". Such a
        // self-named validator owns the sanitization; only a CALLER dropping it
        // is the smell this rule is after.
        if is_function_decl_kind(&node.kind)
            && is_sanitizer_name(&node.name)
            && !node.name.is_empty()
            && after.contains(node.name.as_str())
        {
            return None;
        }
        // FIX (v0.5.0 Alamofire cluster): the guard above only catches a node
        // whose OWN name is sanitizer-prefixed. The real Alamofire case surfaces
        // as a container node (`extension DataRequest { @Sendable public func
        // validate(...) {...} }`) named after the type, so the validator the diff
        // restructured is an inner declaration the callee query never sees as a
        // call. If a sanitizer-prefixed name still appears in the AFTER as a
        // DEFINITION (`func validate`/`fn validate`/`def validate`/`void validate`),
        // the protection wasn't removed — just moved or re-shaped. Only a
        // surviving DEFINITION suppresses: a surviving sanitizer CALL must NOT,
        // or a genuinely removed sanitizer that sits next to a *different*
        // surviving sanitizer call would be missed (ISSUE 4). Check the CODE-ONLY
        // after so a name surviving solely in a comment or string never masks a
        // genuine removal (parse failure falls back to the raw text).
        let after_code = code_only(&node.after, ctx.lang).unwrap_or_else(|| after.clone());
        if node.state == NodeState::Modified && sanitizer_name_survives(&after_code) {
            return None;
        }
        // The sanitize/escape/validate prefix list is naming-convention neutral
        // (`sanitize_html` still prefix-matches), so the structural path works
        // for every family. The text-regex fallback is JS/TS-only.
        //
        // Removed node: any sanitizer callee in the before is a removal. Modified
        // node: a sanitizer callee must have DROPPED (count decreased) — set
        // presence isn't enough, because a different surviving sanitizer call
        // would otherwise mask the one that disappeared (ISSUE 4).
        let had = |src: &str| match callee_names(src, ctx.lang) {
            Some(n) => has_sanitizer_callee(&n),
            None if is_jsts => RE.is_match(src),
            None => false,
        };
        let dropped = match (callee_list(&before, ctx.lang), callee_list(&after, ctx.lang)) {
            (Some(b), Some(a)) => sanitizer_callee_dropped(&b, &a),
            // Structural parse failed: JS/TS falls back to text presence
            // (a sanitizer call in before that is gone from after).
            _ if is_jsts => RE.is_match(&before) && !RE.is_match(&after),
            _ => false,
        };
        let fired = match node.state {
            NodeState::Removed => had(&before),
            NodeState::Modified => dropped,
            _ => false,
        };
        if fired {
            return finding(
                Severity::Low,
                "Removed sanitization",
                "An input sanitization / validation call was removed — review for injection exposure.",
            );
        }
        None
    }
}

/// Differential: a call that ran behind an `if` guard before the change now
/// runs unconditionally. Only a diff-native engine can express this — snapshot
/// scanners see nothing wrong with the after state.
struct GuardRemoved;

const IF_CONSEQUENCE_QUERY: &str = "(if_statement consequence: (_) @cons)";

/// JS/TS early-exit keywords marking a guard clause. Other families bring their
/// own list via the pack (`guard_exit_keywords`).
const JS_GUARD_EXITS: &[&str] = &["return", "throw", "break", "continue"];

/// The if-consequence query for a family: JS/TS const for `JsTs`, else the
/// pack's `@cons` query. `None` => the rule is silent for the family.
fn if_consequence_query(lang: Lang) -> Option<&'static str> {
    match lang.family() {
        Family::JsTs => Some(IF_CONSEQUENCE_QUERY),
        f => lang::pack(f).if_consequence,
    }
}

fn guard_exit_keywords(lang: Lang) -> &'static [&'static str] {
    match lang.family() {
        Family::JsTs => JS_GUARD_EXITS,
        f => lang::pack(f).guard_exit_keywords,
    }
}

/// Callee names inside `if` consequences, with multiplicity. Nested guards can
/// count a call more than once (outer and inner consequence both capture it),
/// so callers compare with `>=` — erring toward "still guarded", never toward
/// a false flag.
fn guarded_callee_list(src: &str, lang: Lang) -> Option<Vec<String>> {
    let consequences =
        crate::structural::capture_texts(src, lang, if_consequence_query(lang)?, "cons")?;
    let mut guarded = Vec::new();
    for cons in &consequences {
        if let Some(names) = callee_list(cons, lang) {
            guarded.extend(names);
        }
    }
    Some(guarded)
}

/// An `if` consequence that is purely an early exit (`return`/`throw`/`break`/
/// `continue`, per the family's `guard_exit_keywords`), i.e. a guard clause.
/// Hoisting a wrapping `if` into an inverted guard clause is the most common
/// guard refactor and must not read as a removed guard.
fn is_guard_clause(consequence: &str, exits: &[&str]) -> bool {
    let body = consequence.trim().trim_start_matches('{').trim();
    exits.iter().any(|kw| {
        // Whole keyword, not an identifier prefix (`returnStatus`, `throwError`).
        body.strip_prefix(kw).is_some_and(|rest| {
            rest.chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_')
        })
    })
}

fn guard_clause_count(src: &str, lang: Lang) -> usize {
    let Some(q) = if_consequence_query(lang) else {
        return 0;
    };
    let exits = guard_exit_keywords(lang);
    crate::structural::capture_texts(src, lang, q, "cons")
        .map(|cs| cs.iter().filter(|c| is_guard_clause(c, exits)).count())
        .unwrap_or(0)
}

impl Rule for GuardRemoved {
    fn id(&self) -> &'static str {
        "guard-removed"
    }
    fn families(&self) -> &'static [Family] {
        ALL_FAMILIES
    }
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if node.state != NodeState::Modified {
            return None;
        }
        let before = joined(&node.before);
        // Cheap pre-filter: no `if` before means nothing could have been guarded.
        if !before.contains("if") {
            return None;
        }
        let guarded_before = guarded_callee_list(&before, ctx.lang)?;
        if guarded_before.is_empty() {
            return None;
        }
        let after = joined(&node.after);
        // If the change introduced an early-exit guard clause (`if (!ok) return`),
        // the protection was very likely converted, not removed. Suppress —
        // the dominant false positive for this rule.
        if guard_clause_count(&after, ctx.lang) > guard_clause_count(&before, ctx.lang) {
            return None;
        }
        let all_before = callee_list(&before, ctx.lang)?;
        let guarded_after = guarded_callee_list(&after, ctx.lang)?;
        let all_after = callee_list(&after, ctx.lang)?;
        // Every before call site was guarded; at least one after call site isn't.
        let escaped = guarded_before.iter().find(|c| {
            count_of(&guarded_before, c) >= count_of(&all_before, c)
                && count_of(&all_after, c) > count_of(&guarded_after, c)
        })?;
        finding(
            Severity::Medium,
            "Guard removed",
            format!(
                "`{escaped}(…)` ran behind a guard before this change and now runs unconditionally — confirm the check wasn't load-bearing."
            ),
        )
    }
}

/// Differential: error handling that wrapped surviving calls before the change
/// is gone after it. The exact notion of "error handling" is per-family:
/// try/catch for most languages, or a `?`/`match`-on-`Result` becoming an
/// unhandled `.unwrap()`/`.expect(...)` for Rust. Go is covered by the guard
/// rules instead, so the rule does not run there.
struct ErrorHandlingRemoved;

/// The JS/TS try construct. Other try-block families bring their own `@try`
/// query via the pack (`try_block`).
const TRY_QUERY: &str = "(try_statement) @try";

fn try_query(lang: Lang) -> Option<&'static str> {
    match lang.family() {
        Family::JsTs => Some(TRY_QUERY),
        f => lang::pack(f).try_block,
    }
}

impl ErrorHandlingRemoved {
    /// TryBlock strategy: a try/catch-equivalent that wrapped calls is gone while
    /// the calls survive. `allow_promise_catch` enables the JS-only refactor
    /// suppression (try/catch → `.catch(…)`).
    fn check_try_block(
        &self,
        ctx: &RuleCtx,
        before: &str,
        after: &str,
        allow_promise_catch: bool,
    ) -> Option<Finding> {
        if !before.contains("try") && !before.contains("do") {
            // Cheap pre-filter: every try-block family's construct keyword is
            // `try` (or Swift's `do`); none means nothing could have been wrapped.
            return None;
        }
        let q = try_query(ctx.lang)?;
        let tries_before = crate::structural::capture_texts(before, ctx.lang, q, "try")?;
        if tries_before.is_empty() {
            return None;
        }
        let tries_after = crate::structural::capture_texts(after, ctx.lang, q, "try")?;
        if !tries_after.is_empty() {
            return None;
        }
        // JS only: try/catch refactored to a promise `.catch(…)` chain still
        // handles errors — not a removal.
        if allow_promise_catch {
            if let Some(after_callees) = callee_names(after, ctx.lang) {
                if after_callees.contains("catch") {
                    return None;
                }
            }
        }
        // The calls that were inside the try must still exist after.
        let mut wrapped = HashSet::new();
        for t in &tries_before {
            if let Some(names) = callee_names(t, ctx.lang) {
                wrapped.extend(names);
            }
        }
        let all_after = callee_names(after, ctx.lang)?;
        let survivor = wrapped.iter().find(|c| all_after.contains(*c))?;
        finding(
            Severity::Low,
            "Error handling removed",
            format!(
                "The error handling around `{survivor}(…)` was removed — failures here are now unhandled."
            ),
        )
    }

    /// UnwrapTransition strategy (Rust): the before handled a fallible result
    /// (a `handled_marker` such as `?` or `match`) and the after replaced it
    /// with an unhandled unwrap (an `unwrap_marker` such as `.unwrap(` /
    /// `.expect(`). Markers come from the family pack; empty => silent.
    fn check_unwrap_transition(&self, ctx: &RuleCtx, before: &str, after: &str) -> Option<Finding> {
        let pack = lang::pack(ctx.lang.family());
        if pack.unwrap_markers.is_empty() || pack.handled_markers.is_empty() {
            return None;
        }
        let before_handled = any_marker(before, pack.handled_markers);
        let after_handled = any_marker(after, pack.handled_markers);
        let before_unwrapped = any_marker(before, pack.unwrap_markers);
        let after_unwrapped = any_marker(after, pack.unwrap_markers);
        // The error handling that existed before must be gone, and a new
        // unhandled unwrap must have taken its place.
        if before_handled && !after_handled && after_unwrapped && !before_unwrapped {
            return finding(
                Severity::Low,
                "Error handling removed",
                "Error handling was replaced with an unwrap that panics on failure — restore the fallible-result handling.",
            );
        }
        None
    }
}

impl Rule for ErrorHandlingRemoved {
    fn id(&self) -> &'static str {
        "removed-try-catch"
    }
    fn families(&self) -> &'static [Family] {
        ALL_EXCEPT_GO
    }
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if node.state != NodeState::Modified {
            return None;
        }
        let before = joined(&node.before);
        let after = joined(&node.after);
        match ctx.lang.family() {
            // JS/TS keeps its exact historical behavior, including the
            // promise-.catch refactor suppression.
            Family::JsTs => self.check_try_block(ctx, &before, &after, true),
            // Other try-block families: same logic, family `@try` query, no
            // promise-.catch suppression (not a JS concept).
            f => match lang::pack(f).error_handling_strategy {
                ErrorHandlingStrategy::TryBlock => {
                    self.check_try_block(ctx, &before, &after, false)
                }
                ErrorHandlingStrategy::UnwrapTransition => {
                    self.check_unwrap_transition(ctx, &before, &after)
                }
                // Go is covered by the guard rules; None never reaches here
                // because the family isn't in ALL_EXCEPT_GO.
                ErrorHandlingStrategy::CoveredByGuards | ErrorHandlingStrategy::None => None,
            },
        }
    }
}

/// Logger redaction emptied / level lowered.
struct PermissiveLogging;
impl Rule for PermissiveLogging {
    fn id(&self) -> &'static str {
        "permissive-logging"
    }
    fn check(&self, node: &AstNode, _ctx: &RuleCtx) -> Option<Finding> {
        if node.state != NodeState::Modified || node.kind != "VariableDeclaration" {
            return None;
        }
        let after = strip_ws(&joined(&node.after));
        let before = strip_ws(&joined(&node.before));
        let emptied = after.contains("redact:[]") && !before.contains("redact:[]");
        let lowered = after.contains("level:\"debug\"") && !before.contains("level:\"debug\"");
        if !emptied && !lowered {
            return None;
        }
        let mut why = Vec::new();
        if emptied {
            why.push("redaction list was emptied");
        }
        if lowered {
            why.push("level lowered to debug");
        }
        finding(
            Severity::Low,
            "Permissive logging config",
            format!(
                "Logger {} — sensitive values may reach log sinks.",
                why.join(" and ")
            ),
        )
    }
}

/// A newly added dependency not declared in package.json (unvetted).
struct UnvettedPackage;
impl Rule for UnvettedPackage {
    fn id(&self) -> &'static str {
        "unvetted-package"
    }
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if node.state != NodeState::Added || node.kind != "ImportDeclaration" {
            return None;
        }
        let module = node.name.trim();
        if module.is_empty() || module.starts_with('.') || module.starts_with('/') {
            return None; // relative/local import, not a dependency
        }
        // Bundler/tsconfig path aliases (`@/…`, `~/…`, `~…`) and Node subpath
        // imports (`#…`) resolve inside the repo, not to an npm package.
        if module.starts_with("@/") || module.starts_with('~') || module.starts_with('#') {
            return None;
        }
        if is_node_builtin(module) {
            return None; // Node standard library import, not an npm package.
        }
        // Bare specifiers may be scoped or sub-pathed: `@scope/pkg/x` → `@scope/pkg`, `pkg/x` → `pkg`.
        let pkg = package_name(module);
        if ctx.deps.contains(&pkg) {
            return None; // declared dependency — vetted
        }
        finding(
            Severity::Medium,
            "Undeclared import",
            format!("Imports `{module}`, which isn't declared in package.json — verify it's intentional."),
        )
    }
}

fn is_node_builtin(module: &str) -> bool {
    // An explicit `node:` specifier is always a core module — the scheme is
    // reserved, so no npm package imports under it. Honoring the prefix also
    // covers node-only built-ins (node:test, node:sqlite, node:sea) without
    // adding their bare names to the allowlist below, where they would shadow
    // real npm packages such as the popular `sqlite` wrapper.
    if module.starts_with("node:") {
        return true;
    }
    let base = module.split('/').next().unwrap_or(module);
    matches!(
        base,
        "assert"
            | "async_hooks"
            | "buffer"
            | "child_process"
            | "cluster"
            | "console"
            | "constants"
            | "crypto"
            | "dgram"
            | "diagnostics_channel"
            | "dns"
            | "domain"
            | "events"
            | "fs"
            | "http"
            | "http2"
            | "https"
            | "inspector"
            | "module"
            | "net"
            | "os"
            | "path"
            | "perf_hooks"
            | "process"
            | "punycode"
            | "querystring"
            | "readline"
            | "repl"
            | "stream"
            | "string_decoder"
            | "timers"
            | "tls"
            | "trace_events"
            | "tty"
            | "url"
            | "util"
            | "v8"
            | "vm"
            | "wasi"
            | "worker_threads"
            | "zlib"
    )
}

fn package_name(module: &str) -> String {
    if let Some(rest) = module.strip_prefix('@') {
        // scoped: @scope/name[/...]
        let mut it = rest.splitn(3, '/');
        match (it.next(), it.next()) {
            (Some(scope), Some(name)) => format!("@{scope}/{name}"),
            _ => module.to_string(),
        }
    } else {
        module.split('/').next().unwrap_or(module).to_string()
    }
}

/// Return the permissive regex literal found in `s`, if any.
fn permissive_regex(s: &str) -> Option<String> {
    for cand in ["/.*/", "/.+/", "/^.*$/", "/^.+$/", "/[\\s\\S]*/"] {
        if s.contains(cand) {
            return Some(cand.to_string());
        }
    }
    None
}

pub fn is_test_path(path: &str) -> bool {
    let slashed = path.replace('\\', "/");
    // Original-case filename for CamelCase test-class detection (`FooTest.cs`).
    let orig_file = slashed.rsplit('/').next().unwrap_or(&slashed);
    let p = slashed.to_lowercase();
    let file = p.rsplit('/').next().unwrap_or(&p);
    p.contains(".test.")
        || p.contains(".spec.")
        // `*.case.<ext>` fixture convention (e.g. the eval harness's
        // `foo.case.mjs`): a dotted `.case.` segment in the FILENAME, not the
        // path. A bare path segment like `cases/` stays excluded — too broad.
        || file.contains(".case.")
        || p.contains("__tests__")
        || p.contains("__mocks__")
        || p.contains(".stories.")
        || p.starts_with("test/")
        || p.starts_with("tests/")
        || p.starts_with("fixtures/")
        || p.contains("/test/")
        || p.contains("/tests/")
        || p.contains("/fixtures/")
        // Go: `foo_test.go`. Rust: `tests/` is handled by the `/test`/`tests`
        // segment checks above. Python: `test_foo.py` and `foo_test.py`.
        || file.ends_with("_test.go")
        || (file.starts_with("test_") && file.ends_with(".py"))
        || file.ends_with("_test.py")
        // CamelCase test-class conventions: `FooTest`/`FooTests` (Java/C#/Kotlin)
        // and `FooTests` (Swift XCTest). The CamelCase boundary (`...Test`, not
        // `...test`) keeps `Latest.cs`, `Audit.java`, and `Manifest.kt` — which
        // merely contain "test" as a lowercase substring — from misreading.
        || has_test_class_suffix(orig_file, ".java")
        || has_test_class_suffix(orig_file, ".cs")
        || has_test_class_suffix(orig_file, ".kt")
        || has_test_class_suffix(orig_file, ".swift")
        // Python conftest convention.
        || file == "conftest.py"
}

/// True when `file` is a CamelCase test class for the given extension —
/// `<Base>Test<ext>` or `<Base>Tests<ext>` where the `T` in `Test`/`Tests` is a
/// real CamelCase boundary. `file` must be the ORIGINAL (non-lowercased) name:
/// `ServiceTest.cs` matches, `Latest.cs` (lowercase `t`) does not. The extension
/// match itself is case-insensitive.
fn has_test_class_suffix(file: &str, ext: &str) -> bool {
    let lower = file.to_ascii_lowercase();
    let Some(ext_len) = lower.strip_suffix(ext).map(|s| s.len()) else {
        return false;
    };
    let stem = &file[..ext_len]; // original case, extension removed
    for marker in ["Tests", "Test"] {
        if let Some(base) = stem.strip_suffix(marker) {
            if !base.is_empty() {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(kind: &str, state: NodeState, before: &[&str], after: &[&str]) -> AstNode {
        AstNode {
            id: "t".into(),
            kind: kind.into(),
            name: "x".into(),
            signature: None,
            state,
            reviewed: false,
            flag_id: None,
            before: if before.is_empty() {
                None
            } else {
                Some(before.iter().map(|s| s.to_string()).collect())
            },
            after: if after.is_empty() {
                None
            } else {
                Some(after.iter().map(|s| s.to_string()).collect())
            },
            children: None,
        }
    }
    fn ctx() -> RuleCtx {
        RuleCtx {
            deps: HashSet::new(),
            is_test_file: false,
            is_build_script: false,
            lang: Lang::Ts,
        }
    }
    fn test_ctx() -> RuleCtx {
        RuleCtx {
            deps: HashSet::new(),
            is_test_file: true,
            is_build_script: false,
            lang: Lang::Ts,
        }
    }

    #[test]
    fn hardcoded_secret_fires_on_markers_only() {
        let r = HardcodedSecret;
        assert!(r
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Added,
                    &[],
                    &["const k = \"AKIA0123456789ABCDEF\";"]
                ),
                &ctx()
            )
            .is_some());
        assert!(r
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Added,
                    &[],
                    &["const k = \"sk-abcdefghij0123456789\";"]
                ),
                &ctx()
            )
            .is_some());
        // plain base64-ish string → no fire (markers only)
        assert!(r
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Added,
                    &[],
                    &["const k = \"aGVsbG8gd29ybGQ=\";"]
                ),
                &ctx()
            )
            .is_none());
        // Secrets are flagged even in test files: a real key pasted into a
        // fixture is still a leak, and the AWS/OpenAI/PEM markers are specific
        // enough to stay low-noise. Other rules still suppress in test paths.
        assert!(r
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Added,
                    &[],
                    &["const k = \"AKIA0123456789ABCDEF\";"]
                ),
                &test_ctx()
            )
            .is_some());
        // removed (not added) → no fire
        assert!(r
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Removed,
                    &["const k = \"AKIA0123456789ABCDEF\";"],
                    &[]
                ),
                &ctx()
            )
            .is_none());
    }

    #[test]
    fn eval_and_fn_constructor() {
        assert!(EvalCall
            .check(
                &node(
                    "ExpressionStatement",
                    NodeState::Added,
                    &[],
                    &["eval(userInput);"]
                ),
                &ctx()
            )
            .is_some());
        // already present before → not newly introduced
        assert!(EvalCall
            .check(
                &node(
                    "ExpressionStatement",
                    NodeState::Modified,
                    &["eval(a);"],
                    &["eval(b);"]
                ),
                &ctx()
            )
            .is_none());
        assert!(FnConstructor
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Added,
                    &[],
                    &["const f = new Function(\"return 1\");"]
                ),
                &ctx()
            )
            .is_some());
    }

    #[test]
    fn eval_structural_catches_text_evasions() {
        // Forms the old text pattern missed: optional chaining, member calls.
        for src in ["eval?.(payload);", "window.eval(payload);", "globalThis.eval(payload);"] {
            assert!(
                EvalCall
                    .check(
                        &node("ExpressionStatement", NodeState::Added, &[], &[src]),
                        &ctx()
                    )
                    .is_some(),
                "structural match should catch: {src}"
            );
        }
    }

    #[test]
    fn structural_ports_catch_text_evasions() {
        // Quoted object keys — the old `key\s*:` patterns never matched these.
        assert!(TlsRejectFalse
            .check(
                &node(
                    "ExpressionStatement",
                    NodeState::Added,
                    &[],
                    &["request({ \"rejectUnauthorized\": false });"]
                ),
                &ctx()
            )
            .is_some());
        assert!(BroadenedCors
            .check(
                &node(
                    "ExpressionStatement",
                    NodeState::Added,
                    &[],
                    &["app.use(cors({ \"origin\": \"*\" }));"]
                ),
                &ctx()
            )
            .is_some());
        // Constant-falsy guards beyond the literal `false`.
        for cond in ["0", "null", "undefined"] {
            let after = format!("if ({cond}) {{ audit(); }}");
            assert!(
                RemovedIfGuard
                    .check(
                        &node(
                            "IfStatement",
                            NodeState::Modified,
                            &["if (isAdmin(user)) { audit(); }"],
                            &[&after]
                        ),
                        &ctx()
                    )
                    .is_some(),
                "constant-falsy guard `if ({cond})` must flag"
            );
        }
        // `verify` surviving only inside a comment must not mask the downgrade.
        assert!(VerifyToDecode
            .check(
                &node(
                    "ReturnStatement",
                    NodeState::Modified,
                    &["return jwt.verify(token, key);"],
                    &["// verify(token) was slow\nreturn jwt.decode(token);"]
                ),
                &ctx()
            )
            .is_some());
    }

    #[test]
    fn structural_ports_ignore_strings_and_comments() {
        assert!(FnConstructor
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Added,
                    &[],
                    &["const tip = \"avoid new Function(code)\";"]
                ),
                &ctx()
            )
            .is_none());
        assert!(TlsRejectFalse
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Added,
                    &[],
                    &["const note = 'never set rejectUnauthorized: false in prod';"]
                ),
                &ctx()
            )
            .is_none());
        assert!(BroadenedCors
            .check(
                &node(
                    "ExpressionStatement",
                    NodeState::Added,
                    &[],
                    &["// do NOT use origin: \"*\" here\nconfigureCors(allowlist);"]
                ),
                &ctx()
            )
            .is_none());
        // A live condition whose BODY merely mentions `if (false)` in a string.
        assert!(RemovedIfGuard
            .check(
                &node(
                    "IfStatement",
                    NodeState::Modified,
                    &["if (cond) { run(); }"],
                    &["if (cond) { log(\"if (false) would disable this\"); run(); }"]
                ),
                &ctx()
            )
            .is_none());
    }

    #[test]
    fn loosened_regex_differential_names_what_weakened() {
        // Anchors dropped — the after pattern is NOT a catch-all, so only the
        // differential comparison can see the weakening.
        let f = LooseRegex
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Modified,
                    &["const idPattern = /^[a-z0-9]{8}$/;"],
                    &["const idPattern = /[a-z0-9]{8}/;"]
                ),
                &ctx(),
            )
            .expect("anchor removal must flag");
        assert!(f.desc.contains("anchors"), "desc names the weakening: {}", f.desc);

        // Length bound dropped.
        let f = LooseRegex
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Modified,
                    &["const token = /^[A-Z]{4,16}$/;"],
                    &["const token = /^[A-Z]+$/;"]
                ),
                &ctx(),
            )
            .expect("unbounded quantifier must flag");
        assert!(f.desc.contains("length bound"), "desc: {}", f.desc);

        // Catch-all still flags (the demo money shot).
        assert!(LooseRegex
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Modified,
                    &["const pattern = /^[A-Za-z0-9_\\-]{32,}$/;"],
                    &["const pattern = /.*/;"]
                ),
                &ctx(),
            )
            .is_some());
    }

    #[test]
    fn loosened_regex_differential_stays_quiet_on_tightening_or_unrelated_change() {
        // Tightened pattern — no flag.
        assert!(LooseRegex
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Modified,
                    &["const p = /[a-z]+/;"],
                    &["const p = /^[a-z]{3,8}$/;"]
                ),
                &ctx(),
            )
            .is_none());
        // Equivalent rewrite with same anchors/bounds — no flag.
        assert!(LooseRegex
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Modified,
                    &["const p = /^[a-z]{3}$/;"],
                    &["const p = /^[a-z0-9]{3}$/;"]
                ),
                &ctx(),
            )
            .is_none());
    }

    #[test]
    fn guard_removed_differential() {
        // chargeCard ran behind a verification guard; now unconditional.
        assert!(GuardRemoved
            .check(
                &node(
                    "FunctionDeclaration",
                    NodeState::Modified,
                    &["function pay(order) {\n  if (isVerified(order)) {\n    chargeCard(order);\n  }\n}"],
                    &["function pay(order) {\n  chargeCard(order);\n}"]
                ),
                &ctx(),
            )
            .is_some());
        // Guard still present — quiet.
        assert!(GuardRemoved
            .check(
                &node(
                    "FunctionDeclaration",
                    NodeState::Modified,
                    &["function pay(order) {\n  if (isVerified(order)) {\n    chargeCard(order);\n  }\n}"],
                    &["function pay(order) {\n  if (isVerified(order) && !order.flagged) {\n    chargeCard(order);\n  }\n}"]
                ),
                &ctx(),
            )
            .is_none());
        // Callee already had an unguarded call site before — quiet.
        assert!(GuardRemoved
            .check(
                &node(
                    "FunctionDeclaration",
                    NodeState::Modified,
                    &["function f() {\n  log(1);\n  if (x) {\n    log(2);\n  }\n}"],
                    &["function f() {\n  log(1);\n  log(2);\n}"]
                ),
                &ctx(),
            )
            .is_none());
    }

    #[test]
    fn error_handling_removed_differential() {
        assert!(ErrorHandlingRemoved
            .check(
                &node(
                    "FunctionDeclaration",
                    NodeState::Modified,
                    &["function sync() {\n  try {\n    pushEvents(queue);\n  } catch (e) {\n    retryLater(e);\n  }\n}"],
                    &["function sync() {\n  pushEvents(queue);\n}"]
                ),
                &ctx(),
            )
            .is_some());
        // try still present — quiet.
        assert!(ErrorHandlingRemoved
            .check(
                &node(
                    "FunctionDeclaration",
                    NodeState::Modified,
                    &["function sync() {\n  try {\n    pushEvents(queue);\n  } catch (e) {}\n}"],
                    &["function sync() {\n  try {\n    pushEvents(queue, opts);\n  } catch (e) {}\n}"]
                ),
                &ctx(),
            )
            .is_none());
        // Call removed along with the try — nothing left unprotected, quiet.
        assert!(ErrorHandlingRemoved
            .check(
                &node(
                    "FunctionDeclaration",
                    NodeState::Modified,
                    &["function sync() {\n  try {\n    pushEvents(queue);\n  } catch (e) {}\n}"],
                    &["function sync() {\n  return;\n}"]
                ),
                &ctx(),
            )
            .is_none());
    }

    #[test]
    fn removed_sanitize_structural_is_not_masked_by_comments() {
        // `sanitize` surviving only in a comment must not mask the removal.
        assert!(RemovedSanitize
            .check(
                &node(
                    "ExpressionStatement",
                    NodeState::Modified,
                    &["render(sanitizeHtml(body));"],
                    &["// sanitizeHtml(body) was redundant\nrender(body);"]
                ),
                &ctx(),
            )
            .is_some());
        // Wrapper-stripping: the sanitize call disappears from inside another call.
        assert!(RemovedSanitize
            .check(
                &node(
                    "ExpressionStatement",
                    NodeState::Modified,
                    &["save(escapeSql(input));"],
                    &["save(input);"]
                ),
                &ctx(),
            )
            .is_some());
    }

    #[test]
    fn removed_sanitize_fires_when_only_a_different_sanitizer_call_survives() {
        // ISSUE 4 regression: `sanitizeHtml` is genuinely removed; a *different*
        // sanitizer-prefixed CALL (`validateSchema(...)`) survives in the after.
        // A surviving call must NOT suppress the removal — only a surviving
        // sanitizer DEFINITION (a validator the protection moved into) should.
        let f = RemovedSanitize.check(
            &node(
                "ExpressionStatement",
                NodeState::Modified,
                &["sanitizeHtml(input); validateSchema(input);"],
                &["validateSchema(input);"],
            ),
            &ctx(),
        );
        assert!(
            f.is_some(),
            "a removed sanitizer must still flag when only a different sanitizer CALL survives"
        );
    }

    #[test]
    fn removed_sanitize_stays_silent_when_a_validator_definition_survives() {
        // ISSUE 4: the Alamofire container case. The node is an `extension`
        // whose own name (`DataRequest`) is NOT sanitizer-prefixed, so the
        // self-named-validator guard does not apply. The inner `func validate`
        // DEFINITION survives the signature churn (gaining `@Sendable`), so the
        // sanitization moved/re-shaped — not removed. Must stay silent.
        let before = "extension DataRequest {\n  public func validate(_ statusCode: Int) -> Self {\n    return self\n  }\n}";
        let after = "extension DataRequest {\n  @Sendable public func validate(_ statusCode: Int) -> Self {\n    return self\n  }\n}";
        let mut n = node("ExportDeclaration", NodeState::Modified, &[before], &[after]);
        n.name = "DataRequest".into();
        assert!(
            RemovedSanitize.check(&n, &lang_ctx(Lang::Swift)).is_none(),
            "a surviving validator DEFINITION must keep signature churn silent"
        );
    }

    // ---- Red-team round 1: confirmed findings become regression tests ----

    #[test]
    fn guard_removed_ignores_early_return_refactor() {
        // THE most common guard refactor: hoist the guard into an early return.
        // The call is still protected; this must NOT flag.
        assert!(GuardRemoved
            .check(
                &node(
                    "FunctionDeclaration",
                    NodeState::Modified,
                    &["function pay(order) {\n  if (isVerified(order)) {\n    chargeCard(order);\n  }\n}"],
                    &["function pay(order) {\n  if (!isVerified(order)) return;\n  chargeCard(order);\n}"]
                ),
                &ctx(),
            )
            .is_none());
    }

    #[test]
    fn guard_removed_ignores_throw_guard_clause() {
        assert!(GuardRemoved
            .check(
                &node(
                    "FunctionDeclaration",
                    NodeState::Modified,
                    &["function pay(order) {\n  if (isVerified(order)) {\n    chargeCard(order);\n  }\n}"],
                    &["function pay(order) {\n  if (!isVerified(order)) { throw new Error('denied'); }\n  chargeCard(order);\n}"]
                ),
                &ctx(),
            )
            .is_none());
    }

    #[test]
    fn verify_to_decode_requires_decode_to_be_new() {
        // decode was already present before; removing a verify call for
        // unrelated reasons must not read as a crypto downgrade.
        assert!(VerifyToDecode
            .check(
                &node(
                    "FunctionDeclaration",
                    NodeState::Modified,
                    &["function f(t) {\n  const meta = decode(t);\n  return verify(t, key);\n}"],
                    &["function f(t) {\n  const meta = decode(t);\n  return meta;\n}"]
                ),
                &ctx(),
            )
            .is_none());
    }

    #[test]
    fn verify_to_decode_catches_async_name_variants() {
        // verifyAsync → decodeJwt: prefix/suffix variants a real refactor emits.
        assert!(VerifyToDecode
            .check(
                &node(
                    "ReturnStatement",
                    NodeState::Modified,
                    &["return jwt.verifyAsync(token, key);"],
                    &["return jwt.decodeJwt(token);"]
                ),
                &ctx(),
            )
            .is_some());
    }

    #[test]
    fn try_catch_to_promise_catch_is_not_flagged() {
        // try/catch refactored to a .catch() chain still handles errors.
        assert!(ErrorHandlingRemoved
            .check(
                &node(
                    "FunctionDeclaration",
                    NodeState::Modified,
                    &["async function f() {\n  try {\n    await push(q);\n  } catch (e) {\n    log(e);\n  }\n}"],
                    &["async function f() {\n  await push(q).catch((e) => log(e));\n}"]
                ),
                &ctx(),
            )
            .is_none());
    }

    #[test]
    fn cors_array_wildcard_is_caught() {
        assert!(BroadenedCors
            .check(
                &node(
                    "ExpressionStatement",
                    NodeState::Added,
                    &[],
                    &["app.use(cors({ origin: [\"*\"] }));"]
                ),
                &ctx(),
            )
            .is_some());
    }

    #[test]
    fn loose_regex_unbounded_quantifier_is_caught() {
        // {0,} and {n,} are unbounded — losing the upper bound is a weakening.
        let f = LooseRegex
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Modified,
                    &["const p = /^[A-Z]{4,16}$/;"],
                    &["const p = /^[A-Z]{4,}$/;"]
                ),
                &ctx(),
            )
            .expect("losing the upper bound must flag");
        assert!(f.desc.contains("length bound"), "desc: {}", f.desc);
    }

    #[test]
    fn loose_regex_skips_comparison_when_literal_counts_differ() {
        // A new regex inserted before an existing one shifts positions; pairing
        // by position would misread. With unequal counts we must not invent a
        // weakening on the tightened/unchanged survivor.
        assert!(LooseRegex
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Modified,
                    &["const zip = /^[0-9]{5}$/;"],
                    &["const dbg = /x/; const zip = /^[0-9]{5}$/;"]
                ),
                &ctx(),
            )
            .is_none());
    }

    #[test]
    fn loose_regex_anchored_catch_all_is_caught() {
        let f = LooseRegex
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Modified,
                    &["const p = /^[a-z]{2,20}$/;"],
                    &["const p = /^[\\s\\S]*$/;"]
                ),
                &ctx(),
            )
            .expect("anchored catch-all must flag");
        assert!(!f.desc.is_empty());
    }

    // ---- Red-team round 2: fixes must not over-suppress ----

    #[test]
    fn guard_removed_word_boundary_does_not_oversuppress() {
        // `return_status = …` is an assignment, not a guard clause — a real
        // unconditional call must still flag.
        assert!(GuardRemoved
            .check(
                &node(
                    "FunctionDeclaration",
                    NodeState::Modified,
                    &["function pay(order) {\n  if (isVerified(order)) {\n    chargeCard(order);\n  }\n}"],
                    &["function pay(order) {\n  if (loggingEnabled) {\n    returnStatus = 'ok';\n  }\n  chargeCard(order);\n}"]
                ),
                &ctx(),
            )
            .is_some());
    }

    #[test]
    fn verify_to_decode_ignores_generic_parsers() {
        // Removing a verify and adding an unrelated parseInt is not a crypto
        // downgrade.
        assert!(VerifyToDecode
            .check(
                &node(
                    "FunctionDeclaration",
                    NodeState::Modified,
                    &["function f(token) {\n  return jwt.verify(token, key);\n}"],
                    &["function f(token) {\n  return parseInt(token.slice(0, 3));\n}"]
                ),
                &ctx(),
            )
            .is_none());
    }

    // ---- Final review: residual false-positive fixes ----

    #[test]
    fn verify_to_decode_ignores_signin_signout() {
        // `signOut` / `signIn` start with "sign" but aren't signature ops;
        // a login-flow refactor that drops one and adds a decode must not flag.
        assert!(VerifyToDecode
            .check(
                &node(
                    "FunctionDeclaration",
                    NodeState::Modified,
                    &["function f(t) {\n  signOut(session);\n}"],
                    &["function f(t) {\n  return decode(t);\n}"]
                ),
                &ctx(),
            )
            .is_none());
    }

    #[test]
    fn loose_regex_ignores_reordered_literals() {
        // Pure reorder: the same two literals, positions swapped, nothing
        // changed. Positional pairing would have compared the unchanged catch-all
        // against the strict pattern and false-flagged; set difference sees both
        // literals in each version and stays silent.
        assert!(LooseRegex
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Modified,
                    &["const a = /.*/; const b = /^[0-9]{3}$/;"],
                    &["const b = /^[0-9]{3}$/; const a = /.*/;"]
                ),
                &ctx(),
            )
            .is_none());
    }

    #[test]
    fn eval_structural_ignores_strings_and_comments() {
        // Forms the old text pattern wrongly flagged.
        for src in [
            "const msg = \"calls eval(x) at runtime\";",
            "// eval(input) was removed in the refactor",
            "log(`avoid eval(${name})`);",
        ] {
            assert!(
                EvalCall
                    .check(
                        &node("ExpressionStatement", NodeState::Added, &[], &[src]),
                        &ctx()
                    )
                    .is_none(),
                "no real call, must not flag: {src}"
            );
        }
    }

    #[test]
    fn child_process() {
        assert!(ChildProcess
            .check(
                &node(
                    "ImportDeclaration",
                    NodeState::Added,
                    &[],
                    &["import { exec } from \"child_process\";"]
                ),
                &ctx()
            )
            .is_some());
        assert!(ChildProcess
            .check(
                &node(
                    "ExpressionStatement",
                    NodeState::Added,
                    &[],
                    &["cp.execSync(cmd);"]
                ),
                &ctx()
            )
            .is_some());
        assert!(ChildProcess
            .check(
                &node(
                    "ExpressionStatement",
                    NodeState::Added,
                    &[],
                    &["doThing();"]
                ),
                &ctx()
            )
            .is_none());
    }

    #[test]
    fn tls_and_env() {
        assert!(TlsRejectFalse
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Added,
                    &[],
                    &["const o = { rejectUnauthorized: false };"]
                ),
                &ctx()
            )
            .is_some());
        assert!(TlsRejectFalse
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Added,
                    &[],
                    &["const o = { rejectUnauthorized: true };"]
                ),
                &ctx()
            )
            .is_none());
        assert!(EnvTlsReject
            .check(
                &node(
                    "ExpressionStatement",
                    NodeState::Added,
                    &[],
                    &["process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0';"]
                ),
                &ctx()
            )
            .is_some());
        // already present before the edit → not newly introduced, no fire
        assert!(EnvTlsReject
            .check(
                &node(
                    "ExpressionStatement",
                    NodeState::Modified,
                    &["process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0'; // legacy"],
                    &["process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0';"],
                ),
                &ctx()
            )
            .is_none());
    }

    #[test]
    fn cors_and_cookies() {
        assert!(BroadenedCors
            .check(
                &node(
                    "ExpressionStatement",
                    NodeState::Added,
                    &[],
                    &["app.use(cors({ origin: '*' }));"]
                ),
                &ctx()
            )
            .is_some());
        assert!(CookieHttpOnlyRemoved
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Modified,
                    &["const o = { httpOnly: true };"],
                    &["const o = {  };"]
                ),
                &ctx()
            )
            .is_some());
        assert!(CookieSecureRemoved
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Modified,
                    &["const o = { secure: true };"],
                    &["const o = {};"]
                ),
                &ctx()
            )
            .is_some());
        assert!(SameSiteWeakened
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Modified,
                    &["const o = { sameSite: 'Strict' };"],
                    &["const o = { sameSite: 'None' };"]
                ),
                &ctx()
            )
            .is_some());
        // not flagged in test fixtures
        assert!(BroadenedCors
            .check(
                &node(
                    "ExpressionStatement",
                    NodeState::Added,
                    &[],
                    &["app.use(cors({ origin: '*' }));"]
                ),
                &test_ctx()
            )
            .is_none());
    }

    #[test]
    fn regex_guard_verify_sanitize_logging() {
        assert!(LooseRegex
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Modified,
                    &["const p = /^[A-Z]{3}$/;"],
                    &["const p = /.*/;"]
                ),
                &ctx()
            )
            .is_some());
        assert!(LooseRegex
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Added,
                    &[],
                    &["const parser = /.*/;"]
                ),
                &ctx()
            )
            .is_some());
        assert!(LooseRegex
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Modified,
                    &["const p = /.*/;"],
                    &["const p = /^.*$/;"]
                ),
                &ctx()
            )
            .is_none());
        assert!(RemovedIfGuard
            .check(
                &node(
                    "IfStatement",
                    NodeState::Modified,
                    &["if (ok) {"],
                    &["if (false) {"]
                ),
                &ctx()
            )
            .is_some());
        assert!(VerifyToDecode
            .check(
                &node(
                    "ReturnStatement",
                    NodeState::Modified,
                    &["return verify(t, KEY);"],
                    &["return decode(t);"]
                ),
                &ctx()
            )
            .is_some());
        assert!(RemovedSanitize
            .check(
                &node(
                    "ExpressionStatement",
                    NodeState::Removed,
                    &["sanitizeInput(token);"],
                    &[]
                ),
                &ctx()
            )
            .is_some());
        assert!(PermissiveLogging
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Modified,
                    &["const l = createLogger({ redact: [\"x\"] });"],
                    &["const l = createLogger({ redact: [] });"]
                ),
                &ctx()
            )
            .is_some());
    }

    #[test]
    fn unvetted_package_respects_deps() {
        let mut deps = HashSet::new();
        deps.insert("react".to_string());
        deps.insert("@tauri-apps/api".to_string());
        let with_deps = RuleCtx {
            deps,
            is_test_file: false,
            is_build_script: false,
            lang: Lang::Ts,
        };

        let mut import = node(
            "ImportDeclaration",
            NodeState::Added,
            &[],
            &["import x from \"react\";"],
        );
        import.name = "react".into();
        assert!(
            UnvettedPackage.check(&import, &with_deps).is_none(),
            "declared dep not flagged"
        );

        import.name = "@tauri-apps/api/window".into();
        assert!(
            UnvettedPackage.check(&import, &with_deps).is_none(),
            "scoped sub-path of declared dep not flagged"
        );

        import.name = "jwt-tiny-decode".into();
        assert!(
            UnvettedPackage.check(&import, &with_deps).is_some(),
            "undeclared dep flagged"
        );

        import.name = "./local".into();
        assert!(
            UnvettedPackage.check(&import, &with_deps).is_none(),
            "relative import not flagged"
        );
    }

    #[test]
    fn unvetted_package_ignores_path_aliases() {
        let no_deps = RuleCtx {
            deps: HashSet::new(),
            is_test_file: false,
            is_build_script: false,
            lang: Lang::Ts,
        };
        let mut import = node(
            "ImportDeclaration",
            NodeState::Added,
            &[],
            &["import x from \"@/components/x\";"],
        );

        for module in ["@/components/x", "~/lib/x", "~utils/x", "#app/x"] {
            import.name = module.into();
            assert!(
                UnvettedPackage.check(&import, &no_deps).is_none(),
                "path alias `{module}` resolves inside the repo, not to an npm package"
            );
        }

        // a real undeclared package still fires, and with the new wording
        import.name = "left-pad".into();
        let f = UnvettedPackage
            .check(&import, &no_deps)
            .expect("undeclared dep flagged");
        assert_eq!(f.r#type, "Undeclared import");
    }

    #[test]
    fn unvetted_package_ignores_node_builtins() {
        let with_deps = RuleCtx {
            deps: HashSet::new(),
            is_test_file: false,
            is_build_script: false,
            lang: Lang::Ts,
        };
        let mut import = node(
            "ImportDeclaration",
            NodeState::Added,
            &[],
            &["import path from \"node:path\";"],
        );

        for module in [
            "node:fs/promises",
            "node:path",
            "node:net",
            "fs",
            "path",
            "net",
            "os",
            "util",
            // Regression: the v0.4.0 self-review dogfood flagged
            // `import { createRequire } from 'module'` as an undeclared package
            // because these stdlib modules were missing from the allowlist.
            "module",
            "node:module",
            "cluster",
            "dgram",
            "perf_hooks",
            "querystring",
            "v8",
            "http2",
            "node:http2",
            "inspector",
            "domain",
            "trace_events",
            "wasi",
            // `node:`-prefixed specifiers are built-ins by the scheme, including
            // node-only modules whose bare names we deliberately do NOT allowlist.
            "node:test",
            "node:test/reporters",
            "node:sqlite",
            "node:sea",
            "node:sys",
        ] {
            import.name = module.into();
            assert!(
                UnvettedPackage.check(&import, &with_deps).is_none(),
                "Node builtin `{module}` is not an npm package"
            );
        }

        import.name = "jwt-tiny-decode".into();
        assert!(
            UnvettedPackage.check(&import, &with_deps).is_some(),
            "true undeclared third-party import still flagged"
        );

        // A bare `sqlite` is the popular npm package, not node:sqlite — it must
        // still flag, which is why these node-only names are not bare-allowlisted.
        import.name = "sqlite".into();
        assert!(
            UnvettedPackage.check(&import, &with_deps).is_some(),
            "bare `sqlite` is an npm package and must still flag"
        );
    }

    #[test]
    fn child_process_handles_node_protocol_and_suppresses_tests() {
        let prod = ctx();
        let test = test_ctx();
        let import = node(
            "ImportDeclaration",
            NodeState::Added,
            &[],
            &["import { spawn } from \"node:child_process\";"],
        );

        assert!(
            ChildProcess.check(&import, &prod).is_some(),
            "production node:child_process import is a subprocess risk"
        );
        assert!(
            ChildProcess.check(&import, &test).is_none(),
            "test harness node:child_process import is suppressed"
        );
    }

    #[test]
    fn test_path_detection_includes_root_level_test_dirs() {
        for path in [
            "tests/e2e-tauri/tauriApp.ts",
            "test/helpers/app.ts",
            "fixtures/sample.ts",
            "src/auth/login.spec.ts",
            "src/auth/login.test.ts",
        ] {
            assert!(
                is_test_path(path),
                "`{path}` should be classified as test-like"
            );
        }
        assert!(!is_test_path("src/runtime/app.ts"));
    }

    #[test]
    fn test_path_detection_includes_core_four_conventions() {
        for path in [
            "pkg/server_test.go",          // Go
            "tests/integration_test.go",
            "app/test_handler.py",         // Python `test_*`
            "app/handler_test.py",         // Python `*_test`
            "conftest.py",
            "src/test/java/com/FooTest.java", // Java (segment + suffix)
            "ServiceTests.java",
        ] {
            assert!(
                is_test_path(path),
                "`{path}` should be classified as test-like"
            );
        }
        // Non-test sources of each language stay non-test.
        assert!(!is_test_path("src/lib.rs"));
        assert!(!is_test_path("cmd/server.go"));
        assert!(!is_test_path("app/handler.py"));
        assert!(!is_test_path("src/main/java/com/Service.java"));
        // `Audit.java` ends in "it.java" only as a substring — must NOT match.
        assert!(!is_test_path("src/Audit.java"));
    }

    #[test]
    fn test_path_detection_includes_stretch_conventions() {
        for path in [
            "src/ServiceTest.cs",   // C# xUnit/NUnit
            "tests/HandlerTests.cs",
            "app/ProcessorTest.kt", // Kotlin
            "app/ProcessorTests.kt",
            "ServiceTests.swift",   // Swift XCTest
        ] {
            assert!(
                is_test_path(path),
                "`{path}` should be classified as test-like"
            );
        }
        // Non-test sources of each stretch language stay non-test.
        assert!(!is_test_path("src/Service.cs"));
        assert!(!is_test_path("app/Processor.kt"));
        assert!(!is_test_path("Sources/App/main.swift"));
        // A file merely ending in a substring (not the suffix) must NOT match.
        assert!(!is_test_path("src/Latest.cs"));
        assert!(!is_test_path("src/Manifest.kt"));
    }

    #[test]
    fn test_path_detection_includes_case_fixture_convention() {
        // `*.case.<ext>` is a fixture/test-case naming convention (the eval
        // harness uses `foo.case.mjs`). The planted-snippet text inside such a
        // fixture is test material, not repo drift.
        for path in [
            "eval/cases/java-child-process.case.mjs",
            "fixtures/foo.case.js",
            "x/y/scenario.case.ts",
            "deep/a.case.mjs",
        ] {
            assert!(
                is_test_path(path),
                "`{path}` (.case. filename) should be classified as test-like"
            );
        }
        // The convention is the dotted `.case.` segment in the FILENAME — a bare
        // `cases/` path segment alone is too broad and must NOT match.
        assert!(!is_test_path("src/cases/handler.ts"));
        assert!(!is_test_path("app/usecases/login.ts"));
        // A file merely containing "case" as a substring must NOT match.
        assert!(!is_test_path("src/lowercase.ts"));
        assert!(!is_test_path("src/casemap.rs"));
    }

    #[test]
    fn registry_dispatches() {
        let reg = RuleRegistry::new();
        let f = reg.check(
            &node("ExpressionStatement", NodeState::Added, &[], &["eval(x);"]),
            &ctx(),
        );
        assert!(f.is_some());
    }

    #[test]
    fn registry_is_a_single_shared_instance() {
        let a = registry();
        let b = registry();
        assert!(std::ptr::eq(a, b));
        assert!(a
            .check(
                &node("ExpressionStatement", NodeState::Added, &[], &["eval(x);"]),
                &ctx(),
            )
            .is_some());
    }

    // ---- Cross-language rule safety (structural-drift languages) ----

    fn lang_ctx(lang: Lang) -> RuleCtx {
        RuleCtx {
            deps: HashSet::new(),
            is_test_file: false,
            is_build_script: false,
            lang,
        }
    }

    #[test]
    fn eval_rule_matches_each_family_marker_for_bare_eval_text() {
        let reg = RuleRegistry::new();
        // The bare JS-shaped `eval(userInput);` snippet is meaningful only where a
        // family's `eval_call` marker actually matches it. Rust/Go/Swift are not in
        // EVAL_FAMILIES (structurally not applicable) and stay silent. Python's
        // marker is `\b(eval|exec|compile)\s*\(`, so `eval(` legitimately fires.
        // Java/C#/Kotlin markers are call-form-specific (ScriptEngineManager /
        // getEngineByName / CSharpScript), so bare `eval(` text must NOT fire for
        // them — structural-or-nothing keeps the JS text from leaking across.
        let silent = [
            Lang::Rust,   // not in EVAL_FAMILIES
            Lang::Go,     // not in EVAL_FAMILIES
            Lang::Swift,  // not in EVAL_FAMILIES
            Lang::Java,   // marker requires ScriptEngineManager(/getEngineByName(
            Lang::CSharp, // marker requires CSharpScript.*
            Lang::Kotlin, // marker requires getEngineByName(
        ];
        for lang in silent {
            let f = reg.check(
                &node("ExpressionStatement", NodeState::Added, &[], &["eval(userInput);"]),
                &lang_ctx(lang),
            );
            assert!(
                f.is_none(),
                "bare JS eval text must not fire for {:?}",
                lang
            );
        }
        // Python's eval/exec/compile marker matches `eval(` by design.
        let py = reg.check(
            &node("ExpressionStatement", NodeState::Added, &[], &["eval(userInput);"]),
            &lang_ctx(Lang::Python),
        );
        assert_eq!(
            py.map(|(id, _)| id),
            Some("eval-call"),
            "Python eval/exec/compile marker must fire on `eval(`"
        );
        // Sanity: the same node DOES flag for the JS/TS family.
        assert!(reg
            .check(
                &node("ExpressionStatement", NodeState::Added, &[], &["eval(userInput);"]),
                &ctx(),
            )
            .is_some());
    }

    #[test]
    fn default_closed_rule_never_fires_outside_jsts_even_with_matching_text() {
        // FnConstructor opts into no family (default-closed = JS/TS only). Even
        // with text its JS query would match, it must never fire for another
        // family — the families() filter excludes it before check() runs.
        let reg = RuleRegistry::new();
        for lang in [
            Lang::Rust,
            Lang::Go,
            Lang::Python,
            Lang::Java,
            Lang::CSharp,
            Lang::Kotlin,
            Lang::Swift,
        ] {
            let f = reg.check(
                &node(
                    "VariableDeclaration",
                    NodeState::Added,
                    &[],
                    &["const f = new Function(\"return 1\");"],
                ),
                &lang_ctx(lang),
            );
            assert!(
                f.is_none(),
                "default-closed fn-constructor must not fire for {:?}",
                lang
            );
        }
        // It DOES fire for JS/TS.
        assert!(reg
            .check(
                &node(
                    "VariableDeclaration",
                    NodeState::Added,
                    &[],
                    &["const f = new Function(\"return 1\");"],
                ),
                &ctx(),
            )
            .is_some());
    }

    #[test]
    fn none_pack_field_means_rule_silence_for_an_opted_in_family() {
        // The contract: a `None` pack field makes its consuming rule silent for an
        // opted-in family, even when text the *other* family's marker would catch
        // is present. We prove it two honest ways now that the packs are filled.

        // (1) Direct, at the lang layer: the EMPTY pack leaves subprocess_import
        // None, so no import marker can ever fire from it.
        assert!(
            lang::FamilyPack::EMPTY.subprocess_import.is_none(),
            "EMPTY pack must leave subprocess_import None — the contract's base case"
        );

        // (2) End-to-end through ChildProcess for a family that deliberately fills
        // only ONE of the two subprocess fields: Java sets subprocess_call but
        // leaves subprocess_import = None (it has no import-shaped subprocess
        // signal worth a query). An import-shaped Java snippet that does NOT match
        // Java's call marker therefore exercises only the None field — and must
        // stay silent. If a future edit accidentally gave Java a subprocess_import
        // marker, this would flip and catch it.
        assert!(
            lang::pack(Family::Java).subprocess_import.is_none(),
            "precondition: Java deliberately leaves subprocess_import None (call-only)"
        );
        let reg = RuleRegistry::new();
        let java_import = reg.check(
            &node(
                "ImportDeclaration",
                NodeState::Added,
                &[],
                &["import java.lang.ProcessBuilder;"],
            ),
            &lang_ctx(Lang::Java),
        );
        assert!(
            java_import.is_none(),
            "Java's None subprocess_import must keep ChildProcess silent on an \
             import-shaped snippet (no subprocess_call match)"
        );
    }

    #[test]
    fn hardcoded_secret_flags_cross_language() {
        let reg = RuleRegistry::new();
        // A fake AWS key pasted into a Python file flags through the registry —
        // the secret marker is language-neutral and runs for every family.
        let py = reg.check(
            &node(
                "VariableDeclaration",
                NodeState::Added,
                &[],
                &["KEY = \"AKIA0123456789ABCDEF\""],
            ),
            &lang_ctx(Lang::Python),
        );
        assert_eq!(py.map(|(id, _)| id), Some("hardcoded-secret"));

        // And in a Rust file (PEM marker).
        let rs = reg.check(
            &node(
                "VariableDeclaration",
                NodeState::Added,
                &[],
                &["const KEY: &str = \"-----BEGIN RSA PRIVATE KEY-----\";"],
            ),
            &lang_ctx(Lang::Rust),
        );
        assert_eq!(rs.map(|(id, _)| id), Some("hardcoded-secret"));

        // And in each stretch language (OpenAI marker).
        for lang in [Lang::CSharp, Lang::Kotlin, Lang::Swift] {
            let f = reg.check(
                &node(
                    "VariableDeclaration",
                    NodeState::Added,
                    &[],
                    &["let key = \"sk-abcdefghij0123456789\""],
                ),
                &lang_ctx(lang),
            );
            assert_eq!(
                f.map(|(id, _)| id),
                Some("hardcoded-secret"),
                "secret must flag for {lang:?}"
            );
        }
    }

    #[test]
    fn cross_language_nodes_never_panic_or_compile_foreign_queries() {
        // Drive realistic snippets from each non-JS language through the full
        // registry. At the foundation stage every non-JS pack field is empty, so
        // each opted-in rule resolves a `None` query/marker and returns silently
        // (structural-or-nothing) — no JS tree-sitter query is ever compiled
        // against these grammars. In a debug build a foreign-grammar query would
        // trip structural.rs's debug_assert, so this passing in `cargo test`
        // (a debug build) proves no such compilation happens. (before == after
        // also means the differential rules have nothing to compare.)
        let reg = RuleRegistry::new();
        let snippets: &[(Lang, &str)] = &[
            (Lang::Rust, "if condition { sanitize(x); verify(t); }"),
            (Lang::Go, "if cond { eval(x); return verify(t) }"),
            (Lang::Python, "if cond:\n    sanitize(x)\n    return decode(t)"),
            (Lang::Java, "if (cond) { sanitize(x); return decode(t); }"),
            (Lang::CSharp, "if (cond) { sanitize(x); return decode(t); }"),
            (Lang::Kotlin, "if (cond) { sanitize(x); return decode(t) }"),
            (Lang::Swift, "if cond { sanitize(x); return decode(t) }"),
        ];
        for (lang, src) in snippets {
            // Modified state exercises the differential rules too (guard-removed,
            // verify-to-decode, removed-sanitize) — all of which are gated off.
            let f = reg.check(
                &node("FunctionDeclaration", NodeState::Modified, &[src], &[src]),
                &lang_ctx(*lang),
            );
            assert!(f.is_none(), "no rule should fire for {:?}: {src}", lang);
        }
    }
}
