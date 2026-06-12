//! Python language pack. Fills the structural queries and marker tables the
//! security rules consult for `.py` files, with positive + idiomatic-negative
//! unit tests below. Until a field is filled the corresponding rule stays silent
//! for Python; every field here is grounded against the tree-sitter-python 0.25
//! grammar and exercised by a test in this module.
//!
//! `error_handling_strategy` is `TryBlock`: Python has `try`/`except`, so
//! `ErrorHandlingRemoved` uses `try_block` to detect a wrapping `try` that
//! disappeared while its calls survived.
use super::{ErrorHandlingStrategy, FamilyPack};

pub static PACK: FamilyPack = FamilyPack {
    // ---- structural queries (grammar: tree-sitter-python 0.25) ----
    // if_statement has fields condition: (expression) and consequence: (block).
    if_condition: Some("(if_statement condition: (_) @cond)"),
    if_consequence: Some("(if_statement consequence: (_) @cons)"),
    // call has function: (primary_expression); a bare call is an identifier, a
    // method call is an attribute whose `attribute:` field is the method name.
    callee: Some(
        "(call function: (identifier) @callee) (call function: (attribute attribute: (identifier) @callee))",
    ),
    try_block: Some("(try_statement) @try"),

    // ---- literal tables ----
    falsy_literals: &["False", "None", "0"],
    // raise is Python's throw; return/break/continue mark early-exit guards.
    guard_exit_keywords: &["return", "raise", "break", "continue"],

    // ---- error handling ----
    error_handling_strategy: ErrorHandlingStrategy::TryBlock,

    // ---- High-severity marker / regex-source fields ----
    // `import subprocess` or `from subprocess import …` newly appearing.
    subprocess_import: Some(r"(?m)^\s*(import\s+subprocess|from\s+subprocess\s+import)\b"),
    // subprocess.{run,call,Popen,check_output,check_call}( / os.system( / os.popen(.
    subprocess_call: Some(
        r"\b(subprocess\.(run|call|Popen|check_output|check_call)|os\.(system|popen))\s*\(",
    ),
    // requests' verify=False, the ssl CERT_NONE constant, or the unverified-context helper.
    tls_disable: Some(r"(verify\s*=\s*False|ssl\.CERT_NONE|_create_unverified_context)"),
    // os.environ assignment of PYTHONHTTPSVERIFY to 0 (dict-set or setdefault).
    env_tls_disable: Some(r#"PYTHONHTTPSVERIFY["']?\s*[\],]?\s*[=,]\s*["']?0"#),
    // Real dynamic-code primitives. `\b` before `eval` does NOT match
    // `ast.literal_eval` (the preceding `_` is a word char), so the safe parser
    // is not flagged.
    eval_call: Some(r"\b(eval|exec|compile)\s*\("),
    // re.compile("…") / re.compile('…') — group 1 is the pattern body.
    regex_compile: Some(r#"re\.compile\(\s*["']([^"']*)"#),
    // Flask-CORS / FastAPI allow_origins=["*"], origins="*", or Django's
    // CORS_ORIGIN_ALLOW_ALL = True.
    cors_permissive: Some(
        r#"(allow_origins\s*=\s*\[\s*["']\*["']|origins\s*=\s*["']\*["']|CORS_ORIGIN_ALLOW_ALL\s*=\s*True)"#,
    ),
    // set_cookie(… httponly=True …) — fires when present before, absent after.
    cookie_httponly: Some(r"httponly\s*=\s*True"),
    // set_cookie(… secure=True …).
    cookie_secure: Some(r"secure\s*=\s*True"),
    // samesite="Lax"/"Strict" (strong) paired with the weak "None".
    cookie_samesite: Some(r#"samesite\s*=\s*["'](Lax|Strict|lax|strict)"#),
    cookie_samesite_weak: Some(r#"samesite\s*=\s*["'](None|none)"#),

    ..FamilyPack::EMPTY
};

#[cfg(test)]
mod tests {
    use crate::model::{AstNode, NodeState};
    use crate::parse::Lang;
    use crate::rules::{registry, RuleCtx};
    use std::collections::HashSet;

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
            lang: Lang::Python,
        }
    }
    fn test_ctx() -> RuleCtx {
        RuleCtx {
            deps: HashSet::new(),
            is_test_file: true,
            is_build_script: false,
            lang: Lang::Python,
        }
    }

    fn fired(node: &AstNode, ctx: &RuleCtx) -> Option<&'static str> {
        registry().check(node, ctx).map(|(id, _)| id)
    }

    // ---------------- removed-if-guard ----------------

    #[test]
    fn removed_if_guard_fires_on_constant_falsy() {
        // is_admin(user) check replaced with a constant — the guard no longer runs.
        for falsy in ["False", "None", "0"] {
            let after = format!("if {falsy}:");
            let n = node(
                "IfStatement",
                NodeState::Modified,
                &["if is_admin(user):", "    audit()"],
                &[&after, "    audit()"],
            );
            assert_eq!(
                fired(&n, &ctx()),
                Some("removed-if-guard"),
                "should fire for falsy {falsy}"
            );
        }
    }

    #[test]
    fn removed_if_guard_negative_live_condition() {
        // Tightening a live condition (ok -> ok and ready) must not flag.
        let n = node(
            "IfStatement",
            NodeState::Modified,
            &["if ok:", "    run()"],
            &["if ok and ready:", "    run()"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    // ---------------- removed-sanitize ----------------

    #[test]
    fn removed_sanitize_fires_when_call_dropped() {
        // sanitize() wrapper stripped from the stored value.
        let n = node(
            "ExpressionStatement",
            NodeState::Modified,
            &["store(sanitize(body))"],
            &["store(body)"],
        );
        assert_eq!(fired(&n, &ctx()), Some("removed-sanitize"));
    }

    #[test]
    fn removed_sanitize_negative_still_present() {
        // sanitize_html still wraps the value (snake_case prefix counts) — no flag.
        let n = node(
            "ExpressionStatement",
            NodeState::Modified,
            &["store(sanitize_html(body))"],
            &["save(sanitize_html(body))"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    // ---------------- verify-to-decode ----------------

    #[test]
    fn verify_to_decode_fires() {
        // jwt.verify(token, key) downgraded to jwt.decode(token) — forged tokens pass.
        let n = node(
            "VariableDeclaration",
            NodeState::Modified,
            &["claims = jwt.verify(token, key)"],
            &["claims = jwt.decode(token)"],
        );
        assert_eq!(fired(&n, &ctx()), Some("verify-to-decode"));
    }

    #[test]
    fn verify_to_decode_negative_still_verifies() {
        // Still verifying after the change — only the variable was renamed.
        let n = node(
            "VariableDeclaration",
            NodeState::Modified,
            &["c = jwt.verify(token, key)"],
            &["claims = jwt.verify(token, key)"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    // ---------------- guard-removed ----------------

    #[test]
    fn guard_removed_fires_when_call_escapes_guard() {
        // charge(order) ran only when is_valid(order); now it runs every time.
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            &[
                "def settle(order):",
                "    if is_valid(order):",
                "        charge(order)",
            ],
            &["def settle(order):", "    charge(order)"],
        );
        assert_eq!(fired(&n, &ctx()), Some("guard-removed"));
    }

    #[test]
    fn guard_removed_negative_early_return_refactor() {
        // Inverted-guard refactor: the protection is converted, not removed.
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            &[
                "def settle(order):",
                "    if is_valid(order):",
                "        charge(order)",
            ],
            &[
                "def settle(order):",
                "    if not is_valid(order):",
                "        return",
                "    charge(order)",
            ],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    // ---------------- error-handling-removed (try block) ----------------

    #[test]
    fn error_handling_removed_fires_when_try_vanishes() {
        // The try/except that wrapped fetch() is gone; fetch() still runs.
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            &[
                "def load():",
                "    try:",
                "        return fetch()",
                "    except Exception:",
                "        return None",
            ],
            &["def load():", "    return fetch()"],
        );
        assert_eq!(fired(&n, &ctx()), Some("removed-try-catch"));
    }

    #[test]
    fn error_handling_removed_negative_try_moved_to_helper() {
        // The try still exists in the after (moved around fetch) — error handling
        // was relocated, not removed.
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            &[
                "def load():",
                "    try:",
                "        return fetch()",
                "    except Exception:",
                "        return None",
            ],
            &[
                "def load():",
                "    try:",
                "        return fetch_with_retry()",
                "    except Exception:",
                "        return None",
            ],
        );
        // fetch is gone (replaced) and the try survives — not a removal.
        assert_eq!(fired(&n, &ctx()), None);
    }

    // ---------------- eval-call ----------------

    #[test]
    fn eval_call_fires_on_eval_exec_compile() {
        for src in ["eval(user_input)", "exec(payload)", "compile(src, '<s>', 'exec')"] {
            let n = node("ExpressionStatement", NodeState::Added, &[], &[src]);
            assert_eq!(fired(&n, &ctx()), Some("eval-call"), "should fire: {src}");
        }
    }

    #[test]
    fn eval_call_fires_on_constant_literal_too() {
        // Flagging eval of a constant is acceptable; the description stays calm.
        let n = node(
            "ExpressionStatement",
            NodeState::Added,
            &[],
            &["eval('1 + 1')"],
        );
        assert_eq!(fired(&n, &ctx()), Some("eval-call"));
    }

    #[test]
    fn eval_call_negative_literal_eval() {
        // ast.literal_eval is the safe parser — must NOT flag.
        let n = node(
            "VariableDeclaration",
            NodeState::Added,
            &[],
            &["data = ast.literal_eval(raw)"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    #[test]
    fn eval_call_ignores_marker_in_comment_or_string() {
        // FIX 1: `eval(` named only in a comment or a string is not a call.
        let comment = node(
            "ExpressionStatement",
            NodeState::Added,
            &[],
            &["# never call eval(user_input) here"],
        );
        assert_eq!(fired(&comment, &ctx()), None, "marker in comment");
        let string = node(
            "VariableDeclaration",
            NodeState::Added,
            &[],
            &["msg = \"do not use eval(payload)\""],
        );
        assert_eq!(fired(&string, &ctx()), None, "marker in string");
        // Sanity: a real eval still fires.
        let real = node("ExpressionStatement", NodeState::Added, &[], &["eval(payload)"]);
        assert_eq!(fired(&real, &ctx()), Some("eval-call"));
    }

    #[test]
    fn eval_call_suppressed_in_test_file() {
        // FIX 5: like child-process/tls, eval is suppressed in test fixtures.
        let n = node("ExpressionStatement", NodeState::Added, &[], &["eval(payload)"]);
        assert_eq!(fired(&n, &test_ctx()), None);
    }

    #[test]
    fn subprocess_ignores_marker_in_comment_or_string() {
        // FIX 1: a subprocess call named only in a comment/string must not fire.
        let comment = node(
            "ExpressionStatement",
            NodeState::Added,
            &[],
            &["# do not subprocess.run([cmd]) on user input"],
        );
        assert_eq!(fired(&comment, &ctx()), None, "marker in comment");
        let string = node(
            "VariableDeclaration",
            NodeState::Added,
            &[],
            &["doc = \"calls subprocess.run([cmd]) internally\""],
        );
        assert_eq!(fired(&string, &ctx()), None, "marker in string");
    }

    #[test]
    fn tls_disable_ignores_marker_in_comment_or_string() {
        // FIX 1: `verify=False` named only in a comment/docstring must not fire.
        let comment = node(
            "ExpressionStatement",
            NodeState::Added,
            &[],
            &["# requests.get(url, verify=False) would be insecure"],
        );
        assert_eq!(fired(&comment, &ctx()), None, "marker in comment");
        let string = node(
            "VariableDeclaration",
            NodeState::Added,
            &[],
            &["hint = \"never pass verify=False\""],
        );
        assert_eq!(fired(&string, &ctx()), None, "marker in string");
    }

    #[test]
    fn cors_permissive_ignores_marker_in_comment() {
        // FIX 1: a permissive-CORS marker named only in a comment must not fire.
        let n = node(
            "ExpressionStatement",
            NodeState::Added,
            &[],
            &["# allow_origins=[\"*\"] is forbidden in production"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    #[test]
    fn samesite_weakened_ignores_marker_in_comment() {
        // FIX 1: the SameSite=None downgrade named only in a comment must not
        // fire (the strong→weak transition lives entirely in prose here).
        let n = node(
            "ExpressionStatement",
            NodeState::Modified,
            &["resp.set_cookie('sid', sid, samesite='Strict')  # was samesite='Lax'"],
            &["resp.set_cookie('sid', sid, samesite='Strict')  # avoid samesite='None'"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    #[test]
    fn eval_call_negative_already_present() {
        // Already calling eval before the change — nothing newly introduced.
        let n = node(
            "ExpressionStatement",
            NodeState::Modified,
            &["eval(a)"],
            &["eval(b)"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    // ---------------- subprocess (child-process) ----------------

    #[test]
    fn subprocess_fires_on_import() {
        let n = node(
            "ImportDeclaration",
            NodeState::Added,
            &[],
            &["import subprocess"],
        );
        assert_eq!(fired(&n, &ctx()), Some("child-process"));
    }

    #[test]
    fn subprocess_fires_on_calls() {
        for src in [
            "subprocess.run([cmd, arg])",
            "subprocess.Popen(argv)",
            "os.system(command)",
            "os.popen(command)",
        ] {
            let n = node("ExpressionStatement", NodeState::Added, &[], &[src]);
            assert_eq!(fired(&n, &ctx()), Some("child-process"), "should fire: {src}");
        }
    }

    #[test]
    fn subprocess_negative_test_file() {
        // A subprocess call inside a test file is suppressed.
        let n = node(
            "ExpressionStatement",
            NodeState::Added,
            &[],
            &["subprocess.run([cmd])"],
        );
        assert_eq!(fired(&n, &test_ctx()), None);
    }

    #[test]
    fn subprocess_negative_already_present() {
        let n = node(
            "ExpressionStatement",
            NodeState::Modified,
            &["subprocess.run([a])"],
            &["subprocess.run([b])"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    // ---------------- tls-disable ----------------

    #[test]
    fn tls_disable_fires() {
        for src in [
            "requests.get(url, verify=False)",
            "ctx = ssl.CERT_NONE",
            "ctx = ssl._create_unverified_context()",
        ] {
            let n = node("ExpressionStatement", NodeState::Added, &[], &[src]);
            assert_eq!(fired(&n, &ctx()), Some("tls-reject-false"), "should fire: {src}");
        }
    }

    #[test]
    fn tls_disable_negative_verify_true() {
        // verify=True is the secure default — must not flag.
        let n = node(
            "ExpressionStatement",
            NodeState::Added,
            &[],
            &["requests.get(url, verify=True)"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    #[test]
    fn tls_disable_negative_already_present() {
        let n = node(
            "ExpressionStatement",
            NodeState::Modified,
            &["requests.get(a, verify=False)"],
            &["requests.get(b, verify=False)"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    // ---------------- env-tls-reject ----------------

    #[test]
    fn env_tls_reject_fires() {
        let n = node(
            "ExpressionStatement",
            NodeState::Added,
            &[],
            &["os.environ['PYTHONHTTPSVERIFY'] = '0'"],
        );
        assert_eq!(fired(&n, &ctx()), Some("env-tls-reject"));
    }

    #[test]
    fn env_tls_reject_ignores_marker_in_comment() {
        // FIX 1: the env-disable named only in a comment must not fire. (The
        // env-var name lives in a string key, so strings are kept — but a comment
        // mention is still dropped.)
        let n = node(
            "ExpressionStatement",
            NodeState::Added,
            &[],
            &["log.warning('tls on')  # never set PYTHONHTTPSVERIFY=0"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    #[test]
    fn env_tls_reject_negative_enabled() {
        // Setting it to 1 (verification on) must not flag.
        let n = node(
            "ExpressionStatement",
            NodeState::Added,
            &[],
            &["os.environ['PYTHONHTTPSVERIFY'] = '1'"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    // ---------------- loose-regex ----------------

    #[test]
    fn loose_regex_fires_on_widening() {
        // An anchored email check loosened to a catch-all.
        let n = node(
            "VariableDeclaration",
            NodeState::Modified,
            &[r#"EMAIL = re.compile("^[^@]+@[^@]+$")"#],
            &[r#"EMAIL = re.compile(".*")"#],
        );
        assert_eq!(fired(&n, &ctx()), Some("loose-regex"));
    }

    #[test]
    fn loose_regex_negative_unchanged() {
        // Same pattern, only the variable renamed — no weakening.
        let n = node(
            "VariableDeclaration",
            NodeState::Modified,
            &[r#"PAT = re.compile("^[a-z]+$")"#],
            &[r#"SLUG = re.compile("^[a-z]+$")"#],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    // ---------------- cors-permissive ----------------

    #[test]
    fn cors_permissive_fires() {
        for src in [
            r#"app.add_middleware(CORSMiddleware, allow_origins=["*"])"#,
            r#"CORS(app, origins="*")"#,
            "CORS_ORIGIN_ALLOW_ALL = True",
        ] {
            let n = node("ExpressionStatement", NodeState::Added, &[], &[src]);
            assert_eq!(fired(&n, &ctx()), Some("broadened-cors"), "should fire: {src}");
        }
    }

    #[test]
    fn cors_permissive_negative_allowlist() {
        let n = node(
            "ExpressionStatement",
            NodeState::Added,
            &[],
            &[r#"app.add_middleware(CORSMiddleware, allow_origins=["https://app.example.com"])"#],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    #[test]
    fn cors_permissive_negative_already_present() {
        let n = node(
            "ExpressionStatement",
            NodeState::Modified,
            &[r#"CORS(app, origins="*")"#],
            &[r#"CORS(api, origins="*")"#],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    // ---------------- cookie httponly ----------------

    #[test]
    fn cookie_httponly_removed_fires() {
        let n = node(
            "ExpressionStatement",
            NodeState::Modified,
            &["resp.set_cookie('sid', sid, httponly=True, secure=True)"],
            &["resp.set_cookie('sid', sid, secure=True)"],
        );
        assert_eq!(fired(&n, &ctx()), Some("cookie-httponly-removed"));
    }

    #[test]
    fn cookie_httponly_negative_still_set() {
        let n = node(
            "ExpressionStatement",
            NodeState::Modified,
            &["resp.set_cookie('sid', a, httponly=True)"],
            &["resp.set_cookie('sid', b, httponly=True)"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    // ---------------- cookie secure ----------------

    #[test]
    fn cookie_secure_removed_fires() {
        // httponly stays so the httponly rule doesn't fire first; secure dropped.
        let n = node(
            "ExpressionStatement",
            NodeState::Modified,
            &["resp.set_cookie('sid', sid, httponly=True, secure=True)"],
            &["resp.set_cookie('sid', sid, httponly=True)"],
        );
        assert_eq!(fired(&n, &ctx()), Some("cookie-secure-removed"));
    }

    // ---------------- samesite weakened ----------------

    #[test]
    fn samesite_weakened_fires() {
        let n = node(
            "ExpressionStatement",
            NodeState::Modified,
            &["resp.set_cookie('sid', sid, samesite='Lax')"],
            &["resp.set_cookie('sid', sid, samesite='None')"],
        );
        assert_eq!(fired(&n, &ctx()), Some("samesite-weakened"));
    }

    #[test]
    fn samesite_negative_still_strict() {
        let n = node(
            "ExpressionStatement",
            NodeState::Modified,
            &["resp.set_cookie('sid', a, samesite='Strict')"],
            &["resp.set_cookie('sid', b, samesite='Lax')"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    // ---------------- structural-only negative (whole-refactor) ----------------

    #[test]
    fn benign_refactor_does_not_flag() {
        // Rename + reorder with protections preserved: no flag of any kind.
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            &[
                "def handle(req):",
                "    if not is_valid(req):",
                "        return None",
                "    return persist(sanitize(req.body))",
            ],
            &[
                "def handle(request):",
                "    if not is_valid(request):",
                "        return None",
                "    clean = sanitize(request.body)",
                "    return persist(clean)",
            ],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }
}
