//! Security-rule registry. Each rule is a small, independently testable predicate
//! over a single diffed `AstNode`. Rules match on the node's STATE + its before/after
//! source `lines` (which carry the node's full subtree text — the parser surfaces
//! statements, not expression-level nodes), so they fire wherever a smell appears in
//! changed code. CWE references noted per rule.
use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use crate::model::{AstNode, NodeState, Severity};
use crate::parse::Lang;

/// Per-file context a rule may consult.
pub struct RuleCtx {
    /// Package names declared in the repo's package.json (deps + devDeps).
    pub deps: HashSet<String>,
    /// Whether this file looks like a test/fixture (suppresses noisy rules).
    pub is_test_file: bool,
    /// The file's language, for structural (tree-sitter query) matching.
    pub lang: Lang,
}

pub struct Finding {
    pub severity: Severity,
    pub r#type: &'static str,
    pub desc: String,
}

fn finding(severity: Severity, r#type: &'static str, desc: impl Into<String>) -> Option<Finding> {
    Some(Finding {
        severity,
        r#type,
        desc: desc.into(),
    })
}

pub trait Rule: Send + Sync {
    fn id(&self) -> &'static str;
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
    /// SAFETY GATE: every security rule except `hardcoded-secret` is written
    /// against JS/TS grammar node kinds — its tree-sitter queries (eval, CORS,
    /// TLS, regex, try/catch, …) and its kind-string checks
    /// (`VariableDeclaration`/`ImportDeclaration`) only make sense for the JS/TS
    /// family. For any other language family we run ONLY the neutral allowlist,
    /// so a JS query is never compiled against a Rust/Go/Python/Java grammar
    /// (no structural.rs debug_assert panic, no coincidental cross-grammar
    /// match, no JS-regex fallback firing on foreign syntax). The core-four
    /// languages still get full structural drift + review progress; they just
    /// don't get JS-specific security rules. `hardcoded-secret` is genuinely
    /// language-neutral (plain AWS/OpenAI/PEM regex over the node's after-text),
    /// so it runs everywhere.
    pub fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<(&'static str, Finding)> {
        if ctx.lang.family() != crate::parse::Family::JsTs {
            return self
                .rules
                .iter()
                .filter(|r| NEUTRAL_RULES.contains(&r.id()))
                .find_map(|r| r.check(node, ctx).map(|f| (r.id(), f)));
        }
        self.rules
            .iter()
            .find_map(|r| r.check(node, ctx).map(|f| (r.id(), f)))
    }
}

/// Rules that are language-neutral enough to run for every family. Only the
/// hardcoded-secret check qualifies today: it is pure text regex over the node's
/// after-content with markers (AWS / OpenAI / PEM) that are identical in any
/// language, and it carries no JS grammar or kind-string assumptions.
const NEUTRAL_RULES: &[&str] = &["hardcoded-secret"];

impl Default for RuleRegistry {
    fn default() -> Self {
        Self::new()
    }
}

static REGISTRY: LazyLock<RuleRegistry> = LazyLock::new(RuleRegistry::new);

pub fn registry() -> &'static RuleRegistry {
    &REGISTRY
}

// ---------- helpers ----------
fn joined(lines: &Option<Vec<String>>) -> String {
    lines.as_ref().map(|l| l.join("\n")).unwrap_or_default()
}
fn strip_ws(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}
fn added_or_modified(s: NodeState) -> bool {
    matches!(s, NodeState::Added | NodeState::Modified)
}

// ---------- rules ----------

/// CWE-798 — hardcoded credentials. Concrete markers only (AWS / OpenAI / PEM).
struct HardcodedSecret;
impl Rule for HardcodedSecret {
    fn id(&self) -> &'static str {
        "hardcoded-secret"
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
        let after = joined(&node.after);
        let what = if AWS.is_match(&after) {
            "an AWS access key"
        } else if OPENAI.is_match(&after) {
            "an API key"
        } else if PEM.is_match(&after) {
            "a private key"
        } else {
            return None;
        };
        finding(
            Severity::High,
            "Hardcoded secret",
            format!("{what} is hardcoded in source — move it to an environment variable or secret store."),
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
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if !added_or_modified(node.state) {
            return None;
        }
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\beval\s*\(").unwrap());
        let after = joined(&node.after);
        // Cheap pre-filter: no `eval` text means no parse needed.
        if !after.contains("eval") {
            return None;
        }
        let hit = |src: &str| {
            crate::structural::query_hit(src, ctx.lang, EVAL_CALL_QUERY)
                .unwrap_or_else(|| RE.is_match(src))
        };
        if hit(&after) && !hit(&joined(&node.before)) {
            return finding(
                Severity::High,
                "Dynamic code execution",
                "`eval(…)` executes arbitrary code — a code-injection risk; use a safe alternative.",
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
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if ctx.is_test_file || !added_or_modified(node.state) {
            return None;
        }
        static IMPORT: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r#"(require\s*\(\s*['"](?:node:)?child_process['"]|from\s+['"](?:node:)?child_process['"])"#).unwrap()
        });
        static CALL: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\.(exec|execSync|spawn|spawnSync)\s*\(").unwrap());
        let after = joined(&node.after);
        let before = joined(&node.before);
        if (IMPORT.is_match(&after) && !IMPORT.is_match(&before))
            || (CALL.is_match(&after) && !CALL.is_match(&before))
        {
            return finding(
                Severity::High,
                "Child process execution",
                "Spawns a subprocess (`child_process`) — ensure arguments can't be attacker-controlled.",
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
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if ctx.is_test_file || !added_or_modified(node.state) {
            return None;
        }
        static RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"rejectUnauthorized\s*:\s*false").unwrap());
        let after = joined(&node.after);
        if !after.contains("rejectUnauthorized") {
            return None;
        }
        let hit = |src: &str| {
            crate::structural::query_hit(src, ctx.lang, TLS_REJECT_QUERY)
                .unwrap_or_else(|| RE.is_match(src))
        };
        if hit(&after) && !hit(&joined(&node.before)) {
            return finding(
                Severity::High,
                "Disabled TLS verification",
                "`rejectUnauthorized: false` turns off certificate validation — restore it.",
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
    fn check(&self, node: &AstNode, _ctx: &RuleCtx) -> Option<Finding> {
        if !added_or_modified(node.state) {
            return None;
        }
        static RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r#"NODE_TLS_REJECT_UNAUTHORIZED\s*=\s*['"]?0"#).unwrap());
        if RE.is_match(&joined(&node.after)) && !RE.is_match(&joined(&node.before)) {
            return finding(
                Severity::High,
                "Disabled TLS verification",
                "`NODE_TLS_REJECT_UNAUTHORIZED=0` disables TLS verification process-wide — remove it.",
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
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if ctx.is_test_file || !added_or_modified(node.state) {
            return None;
        }
        static WILDCARD: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r#"origin\s*:\s*['"]\*['"]"#).unwrap());
        static TRUE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"origin\s*:\s*true").unwrap());
        let after = joined(&node.after);
        if !after.contains("origin") {
            return None;
        }
        let hit = |src: &str| {
            crate::structural::query_hit(src, ctx.lang, CORS_QUERY)
                .unwrap_or_else(|| WILDCARD.is_match(src) || TRUE.is_match(src))
        };
        if hit(&after) && !hit(&joined(&node.before)) {
            return finding(
                Severity::High,
                "Broadened CORS",
                "CORS origin was opened to any site (`*`/`true`) — credentials can leak; use an allowlist.",
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
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if ctx.is_test_file || node.state != NodeState::Modified {
            return None;
        }
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"httpOnly\s*:\s*true").unwrap());
        if RE.is_match(&joined(&node.before)) && !RE.is_match(&joined(&node.after)) {
            return finding(
                Severity::High,
                "Weakened cookie flags",
                "Cookie `httpOnly` was removed — scripts can now read the cookie (XSS theft risk).",
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
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if ctx.is_test_file || node.state != NodeState::Modified {
            return None;
        }
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"secure\s*:\s*true").unwrap());
        if RE.is_match(&joined(&node.before)) && !RE.is_match(&joined(&node.after)) {
            return finding(
                Severity::High,
                "Weakened cookie flags",
                "Cookie `secure` was removed — the cookie may now be sent over plain HTTP.",
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
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if ctx.is_test_file || node.state != NodeState::Modified {
            return None;
        }
        static STRONG: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r#"sameSite\s*:\s*['"]?(Strict|Lax|strict|lax)"#).unwrap());
        static NONE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r#"sameSite\s*:\s*['"]?(None|none)"#).unwrap());
        if STRONG.is_match(&joined(&node.before)) && NONE.is_match(&joined(&node.after)) {
            return finding(
                Severity::High,
                "Weakened cookie flags",
                "Cookie `sameSite` was downgraded to `None` — restores CSRF exposure; use Lax or Strict.",
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

impl Rule for LooseRegex {
    fn id(&self) -> &'static str {
        "loose-regex"
    }
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if !added_or_modified(node.state) || node.kind != "VariableDeclaration" {
            return None;
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

fn callee_list(src: &str, lang: Lang) -> Option<Vec<String>> {
    crate::structural::capture_texts(src, lang, CALLEE_QUERY, "callee")
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
            _ => {
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

fn has_const_falsy_guard(src: &str, lang: Lang) -> Option<bool> {
    let conds = crate::structural::capture_texts(src, lang, IF_CONDITION_QUERY, "cond")?;
    Some(
        conds
            .iter()
            .any(|c| matches!(c.trim(), "false" | "0" | "null" | "undefined")),
    )
}

impl Rule for RemovedIfGuard {
    fn id(&self) -> &'static str {
        "removed-if-guard"
    }
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        // Any Modified node qualifies: exported functions aren't split into
        // body-level child nodes, so the neutralised `if` may sit anywhere in
        // the node's subtree. Structural matching keeps this precise.
        if node.state != NodeState::Modified {
            return None;
        }
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"if\s*\(\s*false\s*\)").unwrap());
        let hit = |src: &str| {
            has_const_falsy_guard(src, ctx.lang).unwrap_or_else(|| RE.is_match(src))
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
    names.iter().any(|n| {
        n.rsplit('.').next().is_some_and(|last| {
            last.starts_with("sanitize") || last.starts_with("escape") || last.starts_with("validate")
        })
    })
}

impl Rule for RemovedSanitize {
    fn id(&self) -> &'static str {
        "removed-sanitize"
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
        let after = joined(&node.after);
        let had = |src: &str| {
            callee_names(src, ctx.lang)
                .map(|n| has_sanitizer_callee(&n))
                .unwrap_or_else(|| RE.is_match(src))
        };
        let fired = match node.state {
            NodeState::Removed => had(&before),
            NodeState::Modified => had(&before) && !had(&after),
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

/// Callee names inside `if` consequences, with multiplicity. Nested guards can
/// count a call more than once (outer and inner consequence both capture it),
/// so callers compare with `>=` — erring toward "still guarded", never toward
/// a false flag.
fn guarded_callee_list(src: &str, lang: Lang) -> Option<Vec<String>> {
    let consequences =
        crate::structural::capture_texts(src, lang, IF_CONSEQUENCE_QUERY, "cons")?;
    let mut guarded = Vec::new();
    for cons in &consequences {
        if let Some(names) = callee_list(cons, lang) {
            guarded.extend(names);
        }
    }
    Some(guarded)
}

/// An `if` consequence that is purely an early exit (`return`/`throw`/`break`/
/// `continue`), i.e. a guard clause. Hoisting a wrapping `if` into an inverted
/// guard clause is the most common guard refactor and must not read as a
/// removed guard.
fn is_guard_clause(consequence: &str) -> bool {
    let body = consequence.trim().trim_start_matches('{').trim();
    ["return", "throw", "break", "continue"].iter().any(|kw| {
        // Whole keyword, not an identifier prefix (`returnStatus`, `throwError`).
        body.strip_prefix(kw).is_some_and(|rest| {
            rest.chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric() && c != '_')
        })
    })
}

fn guard_clause_count(src: &str, lang: Lang) -> usize {
    crate::structural::capture_texts(src, lang, IF_CONSEQUENCE_QUERY, "cons")
        .map(|cs| cs.iter().filter(|c| is_guard_clause(c)).count())
        .unwrap_or(0)
}

impl Rule for GuardRemoved {
    fn id(&self) -> &'static str {
        "guard-removed"
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

/// Differential: a `try { … }` that wrapped calls before the change is gone
/// while the calls remain.
struct ErrorHandlingRemoved;

const TRY_QUERY: &str = "(try_statement) @try";

impl Rule for ErrorHandlingRemoved {
    fn id(&self) -> &'static str {
        "removed-try-catch"
    }
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if node.state != NodeState::Modified {
            return None;
        }
        let before = joined(&node.before);
        if !before.contains("try") {
            return None;
        }
        let after = joined(&node.after);
        let tries_before =
            crate::structural::capture_texts(&before, ctx.lang, TRY_QUERY, "try")?;
        if tries_before.is_empty() {
            return None;
        }
        let tries_after = crate::structural::capture_texts(&after, ctx.lang, TRY_QUERY, "try")?;
        if !tries_after.is_empty() {
            return None;
        }
        // try/catch refactored to a promise `.catch(…)` chain still handles
        // errors — not a removal.
        if let Some(after_callees) = callee_names(&after, ctx.lang) {
            if after_callees.contains("catch") {
                return None;
            }
        }
        // The calls that were inside the try must still exist after.
        let mut wrapped = HashSet::new();
        for t in &tries_before {
            if let Some(names) = callee_names(t, ctx.lang) {
                wrapped.extend(names);
            }
        }
        let all_after = callee_names(&after, ctx.lang)?;
        let survivor = wrapped.iter().find(|c| all_after.contains(*c))?;
        finding(
            Severity::Low,
            "Error handling removed",
            format!(
                "The try/catch around `{survivor}(…)` was removed — failures here are now unhandled."
            ),
        )
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
    let module = module.strip_prefix("node:").unwrap_or(module);
    let base = module.split('/').next().unwrap_or(module);
    matches!(
        base,
        "assert"
            | "async_hooks"
            | "buffer"
            | "child_process"
            | "console"
            | "crypto"
            | "dns"
            | "events"
            | "fs"
            | "http"
            | "https"
            | "net"
            | "os"
            | "path"
            | "process"
            | "readline"
            | "stream"
            | "string_decoder"
            | "timers"
            | "tls"
            | "tty"
            | "url"
            | "util"
            | "vm"
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
            lang: Lang::Ts,
        }
    }
    fn test_ctx() -> RuleCtx {
        RuleCtx {
            deps: HashSet::new(),
            is_test_file: true,
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
            lang,
        }
    }

    #[test]
    fn non_js_families_only_run_the_neutral_allowlist() {
        let reg = RuleRegistry::new();
        // An `eval(...)` call is a JS-specific rule. In a non-JS family it must
        // NOT flag — the eval rule is gated out, and no foreign-grammar query is
        // ever compiled.
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
                &node("ExpressionStatement", NodeState::Added, &[], &["eval(userInput);"]),
                &lang_ctx(lang),
            );
            assert!(
                f.is_none(),
                "JS-specific eval rule must not fire for {:?}",
                lang
            );
        }
        // Sanity: the same node DOES flag for the JS/TS family.
        assert!(reg
            .check(
                &node("ExpressionStatement", NodeState::Added, &[], &["eval(userInput);"]),
                &ctx(),
            )
            .is_some());
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
        // Drive realistic snippets from each core-four language through the full
        // registry. The gate means no JS tree-sitter query is ever compiled
        // against these grammars — in a debug build a foreign-grammar query
        // would trip structural.rs's debug_assert, so this passing in
        // `cargo test` (a debug build) proves the gate holds.
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
