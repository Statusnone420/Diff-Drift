//! Security-rule registry. Each rule is a small, independently testable predicate
//! over a single diffed `AstNode`. Rules match on the node's STATE + its before/after
//! source `lines` (which carry the node's full subtree text — the parser surfaces
//! statements, not expression-level nodes), so they fire wherever a smell appears in
//! changed code. CWE references noted per rule.
use std::collections::HashSet;
use std::sync::LazyLock;

use regex::Regex;

use crate::model::{AstNode, NodeState, Severity};

/// Per-file context a rule may consult.
pub struct RuleCtx {
    /// Package names declared in the repo's package.json (deps + devDeps).
    pub deps: HashSet<String>,
    /// Whether this file looks like a test/fixture (suppresses noisy rules).
    pub is_test_file: bool,
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
            ],
        }
    }

    /// First matching rule's id + its finding.
    pub fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<(&'static str, Finding)> {
        self.rules
            .iter()
            .find_map(|r| r.check(node, ctx).map(|f| (r.id(), f)))
    }
}

impl Default for RuleRegistry {
    fn default() -> Self {
        Self::new()
    }
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
    fn check(&self, node: &AstNode, ctx: &RuleCtx) -> Option<Finding> {
        if ctx.is_test_file || !added_or_modified(node.state) {
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

/// CWE-95 — code injection via eval.
struct EvalCall;
impl Rule for EvalCall {
    fn id(&self) -> &'static str {
        "eval-call"
    }
    fn check(&self, node: &AstNode, _ctx: &RuleCtx) -> Option<Finding> {
        if !added_or_modified(node.state) {
            return None;
        }
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\beval\s*\(").unwrap());
        let after = joined(&node.after);
        if RE.is_match(&after) && !RE.is_match(&joined(&node.before)) {
            return finding(
                Severity::High,
                "Dynamic code execution",
                "`eval(…)` executes arbitrary code — a code-injection risk; use a safe alternative.",
            );
        }
        None
    }
}

/// CWE-95 — code injection via the Function constructor.
struct FnConstructor;
impl Rule for FnConstructor {
    fn id(&self) -> &'static str {
        "fn-constructor"
    }
    fn check(&self, node: &AstNode, _ctx: &RuleCtx) -> Option<Finding> {
        if !added_or_modified(node.state) {
            return None;
        }
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"new\s+Function\s*\(").unwrap());
        if RE.is_match(&joined(&node.after)) && !RE.is_match(&joined(&node.before)) {
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

/// CWE-295 — disabled TLS certificate validation.
struct TlsRejectFalse;
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
        if RE.is_match(&joined(&node.after)) && !RE.is_match(&joined(&node.before)) {
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
        static RE: LazyLock<Regex> = LazyLock::new(|| {
            Regex::new(r#"NODE_TLS_REJECT_UNAUTHORIZED\s*=\s*['"]?0"#).unwrap()
        });
        if RE.is_match(&joined(&node.after)) {
            return finding(
                Severity::High,
                "Disabled TLS verification",
                "`NODE_TLS_REJECT_UNAUTHORIZED=0` disables TLS verification process-wide — remove it.",
            );
        }
        None
    }
}

/// CWE-942 — overly permissive CORS.
struct BroadenedCors;
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
        static TRUE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"origin\s*:\s*true").unwrap());
        let after = joined(&node.after);
        let before = joined(&node.before);
        let now = WILDCARD.is_match(&after) || TRUE.is_match(&after);
        let was = WILDCARD.is_match(&before) || TRUE.is_match(&before);
        if now && !was {
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
        static RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"httpOnly\s*:\s*true").unwrap());
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

/// Validation regex widened to a catch-all.
struct LooseRegex;
impl Rule for LooseRegex {
    fn id(&self) -> &'static str {
        "loose-regex"
    }
    fn check(&self, node: &AstNode, _ctx: &RuleCtx) -> Option<Finding> {
        if !added_or_modified(node.state) || node.kind != "VariableDeclaration" {
            return None;
        }
        let after = joined(&node.after);
        let before = joined(&node.before);
        if let Some(rx) = permissive_regex(&after) {
            if before.is_empty() || permissive_regex(&before).is_none() {
                return finding(
                    Severity::High,
                    "Loose regex pattern",
                    format!("Validation regex was widened to {rx} — any string now passes validation."),
                );
            }
        }
        None
    }
}

/// CWE-347 — signature verification replaced by a non-verifying decode/parse.
struct VerifyToDecode;
impl Rule for VerifyToDecode {
    fn id(&self) -> &'static str {
        "verify-to-decode"
    }
    fn check(&self, node: &AstNode, _ctx: &RuleCtx) -> Option<Finding> {
        if node.state != NodeState::Modified {
            return None;
        }
        static VERIFY: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\b(verify|sign)\s*\(").unwrap());
        static DECODE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\b(decode|parse)\s*\(").unwrap());
        let after = joined(&node.after);
        let before = joined(&node.before);
        if VERIFY.is_match(&before) && DECODE.is_match(&after) && !VERIFY.is_match(&after) {
            return finding(
                Severity::Medium,
                "Crypto downgrade",
                "A signature `verify`/`sign` was replaced with a non-verifying `decode`/`parse` — forged tokens may pass.",
            );
        }
        None
    }
}

/// A guard clause neutralised to a constant `if (false)`.
struct RemovedIfGuard;
impl Rule for RemovedIfGuard {
    fn id(&self) -> &'static str {
        "removed-if-guard"
    }
    fn check(&self, node: &AstNode, _ctx: &RuleCtx) -> Option<Finding> {
        if node.state != NodeState::Modified || node.kind != "IfStatement" {
            return None;
        }
        static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"if\s*\(\s*false\s*\)").unwrap());
        if RE.is_match(&joined(&node.after)) && !RE.is_match(&joined(&node.before)) {
            return finding(
                Severity::Low,
                "Disabled guard",
                "A guard condition was replaced with `if (false)` — the check no longer runs.",
            );
        }
        None
    }
}

/// A sanitization / validation call removed outright.
struct RemovedSanitize;
impl Rule for RemovedSanitize {
    fn id(&self) -> &'static str {
        "removed-sanitize"
    }
    fn check(&self, node: &AstNode, _ctx: &RuleCtx) -> Option<Finding> {
        static RE: LazyLock<Regex> =
            LazyLock::new(|| Regex::new(r"\b(sanitize|escape|validate)\w*\s*\(").unwrap());
        let fired = match node.state {
            NodeState::Removed => RE.is_match(&joined(&node.before)),
            NodeState::Modified => {
                RE.is_match(&joined(&node.before)) && !RE.is_match(&joined(&node.after))
            }
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
            format!("Logger {} — sensitive values may reach log sinks.", why.join(" and ")),
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
            "Unvetted nested package",
            format!("Imports `{module}`, which isn't declared in package.json — no audit trail."),
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
    let p = path.replace('\\', "/").to_lowercase();
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
        }
    }
    fn test_ctx() -> RuleCtx {
        RuleCtx {
            deps: HashSet::new(),
            is_test_file: true,
        }
    }

    #[test]
    fn hardcoded_secret_fires_on_markers_only() {
        let r = HardcodedSecret;
        assert!(r
            .check(&node("VariableDeclaration", NodeState::Added, &[], &["const k = \"AKIA0123456789ABCDEF\";"]), &ctx())
            .is_some());
        assert!(r
            .check(&node("VariableDeclaration", NodeState::Added, &[], &["const k = \"sk-abcdefghij0123456789\";"]), &ctx())
            .is_some());
        // plain base64-ish string → no fire (markers only)
        assert!(r
            .check(&node("VariableDeclaration", NodeState::Added, &[], &["const k = \"aGVsbG8gd29ybGQ=\";"]), &ctx())
            .is_none());
        // suppressed in test files
        assert!(r
            .check(&node("VariableDeclaration", NodeState::Added, &[], &["const k = \"AKIA0123456789ABCDEF\";"]), &test_ctx())
            .is_none());
        // removed (not added) → no fire
        assert!(r
            .check(&node("VariableDeclaration", NodeState::Removed, &["const k = \"AKIA0123456789ABCDEF\";"], &[]), &ctx())
            .is_none());
    }

    #[test]
    fn eval_and_fn_constructor() {
        assert!(EvalCall
            .check(&node("ExpressionStatement", NodeState::Added, &[], &["eval(userInput);"]), &ctx())
            .is_some());
        // already present before → not newly introduced
        assert!(EvalCall
            .check(&node("ExpressionStatement", NodeState::Modified, &["eval(a);"], &["eval(b);"]), &ctx())
            .is_none());
        assert!(FnConstructor
            .check(&node("VariableDeclaration", NodeState::Added, &[], &["const f = new Function(\"return 1\");"]), &ctx())
            .is_some());
    }

    #[test]
    fn child_process() {
        assert!(ChildProcess
            .check(&node("ImportDeclaration", NodeState::Added, &[], &["import { exec } from \"child_process\";"]), &ctx())
            .is_some());
        assert!(ChildProcess
            .check(&node("ExpressionStatement", NodeState::Added, &[], &["cp.execSync(cmd);"]), &ctx())
            .is_some());
        assert!(ChildProcess
            .check(&node("ExpressionStatement", NodeState::Added, &[], &["doThing();"]), &ctx())
            .is_none());
    }

    #[test]
    fn tls_and_env() {
        assert!(TlsRejectFalse
            .check(&node("VariableDeclaration", NodeState::Added, &[], &["const o = { rejectUnauthorized: false };"]), &ctx())
            .is_some());
        assert!(TlsRejectFalse
            .check(&node("VariableDeclaration", NodeState::Added, &[], &["const o = { rejectUnauthorized: true };"]), &ctx())
            .is_none());
        assert!(EnvTlsReject
            .check(&node("ExpressionStatement", NodeState::Added, &[], &["process.env.NODE_TLS_REJECT_UNAUTHORIZED = '0';"]), &ctx())
            .is_some());
    }

    #[test]
    fn cors_and_cookies() {
        assert!(BroadenedCors
            .check(&node("ExpressionStatement", NodeState::Added, &[], &["app.use(cors({ origin: '*' }));"]), &ctx())
            .is_some());
        assert!(CookieHttpOnlyRemoved
            .check(&node("VariableDeclaration", NodeState::Modified, &["const o = { httpOnly: true };"], &["const o = {  };"]), &ctx())
            .is_some());
        assert!(CookieSecureRemoved
            .check(&node("VariableDeclaration", NodeState::Modified, &["const o = { secure: true };"], &["const o = {};"]), &ctx())
            .is_some());
        assert!(SameSiteWeakened
            .check(&node("VariableDeclaration", NodeState::Modified, &["const o = { sameSite: 'Strict' };"], &["const o = { sameSite: 'None' };"]), &ctx())
            .is_some());
        // not flagged in test fixtures
        assert!(BroadenedCors
            .check(&node("ExpressionStatement", NodeState::Added, &[], &["app.use(cors({ origin: '*' }));"]), &test_ctx())
            .is_none());
    }

    #[test]
    fn regex_guard_verify_sanitize_logging() {
        assert!(LooseRegex
            .check(&node("VariableDeclaration", NodeState::Modified, &["const p = /^[A-Z]{3}$/;"], &["const p = /.*/;"]), &ctx())
            .is_some());
        assert!(LooseRegex
            .check(&node("VariableDeclaration", NodeState::Added, &[], &["const parser = /.*/;"]), &ctx())
            .is_some());
        assert!(LooseRegex
            .check(&node("VariableDeclaration", NodeState::Modified, &["const p = /.*/;"], &["const p = /^.*$/;"]), &ctx())
            .is_none());
        assert!(RemovedIfGuard
            .check(&node("IfStatement", NodeState::Modified, &["if (ok) {"], &["if (false) {"]), &ctx())
            .is_some());
        assert!(VerifyToDecode
            .check(&node("ReturnStatement", NodeState::Modified, &["return verify(t, KEY);"], &["return decode(t);"]), &ctx())
            .is_some());
        assert!(RemovedSanitize
            .check(&node("ExpressionStatement", NodeState::Removed, &["sanitizeInput(token);"], &[]), &ctx())
            .is_some());
        assert!(PermissiveLogging
            .check(&node("VariableDeclaration", NodeState::Modified, &["const l = createLogger({ redact: [\"x\"] });"], &["const l = createLogger({ redact: [] });"]), &ctx())
            .is_some());
    }

    #[test]
    fn unvetted_package_respects_deps() {
        let mut deps = HashSet::new();
        deps.insert("react".to_string());
        deps.insert("@tauri-apps/api".to_string());
        let with_deps = RuleCtx { deps, is_test_file: false };

        let mut import = node("ImportDeclaration", NodeState::Added, &[], &["import x from \"react\";"]);
        import.name = "react".into();
        assert!(UnvettedPackage.check(&import, &with_deps).is_none(), "declared dep not flagged");

        import.name = "@tauri-apps/api/window".into();
        assert!(UnvettedPackage.check(&import, &with_deps).is_none(), "scoped sub-path of declared dep not flagged");

        import.name = "jwt-tiny-decode".into();
        assert!(UnvettedPackage.check(&import, &with_deps).is_some(), "undeclared dep flagged");

        import.name = "./local".into();
        assert!(UnvettedPackage.check(&import, &with_deps).is_none(), "relative import not flagged");
    }

    #[test]
    fn unvetted_package_ignores_node_builtins() {
        let with_deps = RuleCtx { deps: HashSet::new(), is_test_file: false };
        let mut import = node("ImportDeclaration", NodeState::Added, &[], &["import path from \"node:path\";"]);

        for module in ["node:fs/promises", "node:path", "node:net", "fs", "path", "net", "os", "util"] {
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
        let import = node("ImportDeclaration", NodeState::Added, &[], &["import { spawn } from \"node:child_process\";"]);

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
            assert!(is_test_path(path), "`{path}` should be classified as test-like");
        }
        assert!(!is_test_path("src/runtime/app.ts"));
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
}
