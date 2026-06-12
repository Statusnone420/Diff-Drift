//! Java language pack. Fills the structural queries and high-severity marker
//! tables the security rules consult so they run against Java syntax without any
//! JS node kinds or JS text regexes leaking in. Every field is either a grounded
//! tree-sitter query (compiled+cached, exercised by the tests below) or a Java-
//! specific regex source; an unfilled field keeps the corresponding rule silent.
//!
//! `error_handling_strategy` is `TryBlock`: Java has `try`/`catch`, so
//! `ErrorHandlingRemoved` uses `try_block` to detect a wrapping `try` (or
//! try-with-resources) that disappeared while its calls survived.
//!
//! Grammar ground truth (tree-sitter-java 0.23.5 node-types.json):
//! - `if_statement` — `condition: (parenthesized_expression …)`, `consequence:
//!   (statement)`. So the if-condition lives one level inside the parens.
//! - `method_invocation` — `name: (identifier)` is the called method for BOTH
//!   bare `foo()` and member `obj.foo()` forms (the receiver is the separate
//!   `object` field), so a single capture covers both.
//! - `try_statement` and `try_with_resources_statement` are distinct kinds; both
//!   are try/catch-equivalents.
use super::{ErrorHandlingStrategy, FamilyPack};

pub static PACK: FamilyPack = FamilyPack {
    // ---- structural queries ----
    if_condition: Some("(if_statement condition: (parenthesized_expression (_) @cond))"),
    if_consequence: Some("(if_statement consequence: (_) @cons)"),
    callee: Some("(method_invocation name: (identifier) @callee)"),
    try_block: Some("[(try_statement) (try_with_resources_statement)] @try"),

    // ---- literal tables ----
    falsy_literals: &["false"],
    guard_exit_keywords: &["return", "throw", "break", "continue"],

    error_handling_strategy: ErrorHandlingStrategy::TryBlock,

    // ---- high-severity markers (regex sources; differential where noted) ----
    // ChildProcess: Runtime.getRuntime().exec(…) or new ProcessBuilder(…).
    subprocess_import: None,
    subprocess_call: Some(r"(?:Runtime\s*\.\s*getRuntime\s*\(\s*\)\s*\.\s*exec\s*\(|\bnew\s+ProcessBuilder\s*\()"),
    // TlsRejectFalse: an always-trusting hostname verifier / trust-all strategy.
    // Each alternative is a concrete, named insecure idiom — never normal code.
    tls_disable: Some(
        r#"(?:ALLOW_ALL_HOSTNAME_VERIFIER|NoopHostnameVerifier|new\s+TrustAllStrategy|setHostnameVerifier\s*\(\s*\([^)]*\)\s*->\s*true|verify\s*\([^)]*\)\s*\{[^}]*return\s+true)"#,
    ),
    env_tls_disable: None,
    // EvalCall: a ScriptEngine evaluating a string (ScriptEngineManager +
    // engine.eval(…)). `.eval(` alone is too broad, so require the engine type
    // or a getEngineByName seam.
    eval_call: Some(r"(?:new\s+ScriptEngineManager\s*\(|getEngineByName\s*\(|ScriptEngine\b[^;]*\.\s*eval\s*\()"),
    // LooseRegex: Pattern.compile("…"). Group 1 = the pattern body (the rule
    // runs the SAME catch-all/anchor/length-bound weakening comparison the JS
    // literal path uses over these bodies).
    regex_compile: Some(r#"Pattern\s*\.\s*compile\s*\(\s*"([^"]*)""#),
    // BroadenedCors: @CrossOrigin(origins = "*"), addAllowedOrigin("*"),
    // setAllowedOrigins(List.of("*")) / allowedOrigins("*").
    cors_permissive: Some(
        r#"(?:@CrossOrigin\s*\([^)]*origins\s*=\s*"\*"|addAllowedOrigin\s*\(\s*"\*"|setAllowedOrigins\s*\([^)]*"\*"|allowedOrigins\s*\(\s*"\*")"#,
    ),
    // Cookie* differential markers: matched in before, not after => removed.
    cookie_httponly: Some(r"setHttpOnly\s*\(\s*true\s*\)"),
    cookie_secure: Some(r"setSecure\s*\(\s*true\s*\)"),
    // SameSite strong in before + weak in after => weakened. Covers the string
    // attribute form (`SameSite=Strict`) and the builder form (`sameSite("Lax")`).
    cookie_samesite: Some(r#"(?i)SameSite\s*[=(]\s*"?(?:Strict|Lax)"#),
    cookie_samesite_weak: Some(r#"(?i)SameSite\s*[=(]\s*"?None"#),

    ..FamilyPack::EMPTY
};

#[cfg(test)]
mod tests {
    use crate::model::{AstNode, NodeState};
    use crate::parse::Lang;
    use crate::rules::{registry, RuleCtx};
    use std::collections::HashSet;

    /// Build a Modified node from before/after Java source blocks.
    fn modified(kind: &str, before: &str, after: &str) -> AstNode {
        AstNode {
            id: "t".into(),
            kind: kind.into(),
            name: "n".into(),
            signature: None,
            state: NodeState::Modified,
            reviewed: false,
            flag_id: None,
            before: Some(before.lines().map(|s| s.to_string()).collect()),
            after: Some(after.lines().map(|s| s.to_string()).collect()),
            children: None,
        }
    }

    fn added(kind: &str, after: &str) -> AstNode {
        AstNode {
            id: "t".into(),
            kind: kind.into(),
            name: "n".into(),
            signature: None,
            state: NodeState::Added,
            reviewed: false,
            flag_id: None,
            before: None,
            after: Some(after.lines().map(|s| s.to_string()).collect()),
            children: None,
        }
    }

    fn ctx(is_test_file: bool) -> RuleCtx {
        RuleCtx {
            deps: HashSet::new(),
            is_test_file,
            lang: Lang::Java,
        }
    }

    fn fired(node: &AstNode, is_test_file: bool) -> Option<&'static str> {
        registry().check(node, &ctx(is_test_file)).map(|(id, _)| id)
    }

    // ---------- removed-if-guard ----------

    #[test]
    fn removed_if_guard_fires_on_constant_false() {
        // A real guard condition replaced with `false` — the check no longer runs.
        let node = modified(
            "IfStatement",
            "if (isAdmin(user)) {\n    audit();\n}",
            "if (false) {\n    audit();\n}",
        );
        assert_eq!(fired(&node, false), Some("removed-if-guard"));
    }

    #[test]
    fn removed_if_guard_ignores_live_condition_tightening() {
        // Idiomatic refactor: condition stays live (tightened). Must NOT flag.
        let node = modified(
            "IfStatement",
            "if (ok) {\n    run();\n}",
            "if (ok && ready) {\n    run();\n}",
        );
        assert_eq!(fired(&node, false), None);
    }

    #[test]
    fn removed_if_guard_ignores_false_in_a_string() {
        // The word "false" inside a string literal is not a constant-falsy guard.
        let node = modified(
            "IfStatement",
            "if (flag) {\n    log(\"ok\");\n}",
            "if (flag) {\n    log(\"false\");\n}",
        );
        assert_eq!(fired(&node, false), None);
    }

    // ---------- removed-sanitize ----------

    #[test]
    fn removed_sanitize_fires_when_wrapper_stripped() {
        // sanitize(...) wrapper removed around the saved value.
        let node = modified(
            "ExpressionStatement",
            "repo.save(sanitize(input));",
            "repo.save(input);",
        );
        assert_eq!(fired(&node, false), Some("removed-sanitize"));
    }

    #[test]
    fn removed_sanitize_ignores_overload_rename_keeping_validate() {
        // Overload-heavy class: a method renamed, but validate(...) still runs.
        let node = modified(
            "ExpressionStatement",
            "store(validate(input));",
            "persist(validate(input));",
        );
        assert_eq!(fired(&node, false), None);
    }

    // ---------- verify-to-decode ----------

    #[test]
    fn verify_to_decode_fires_on_jwt_downgrade() {
        let node = modified(
            "VariableDeclaration",
            "Claims c = parser.verify(token);",
            "Claims c = parser.decode(token);",
        );
        assert_eq!(fired(&node, false), Some("verify-to-decode"));
    }

    #[test]
    fn verify_to_decode_ignores_kept_verify() {
        // verify(...) survives — no downgrade.
        let node = modified(
            "VariableDeclaration",
            "Claims c = parser.verify(token);",
            "Claims c = parser.verify(refresh(token));",
        );
        assert_eq!(fired(&node, false), None);
    }

    // ---------- guard-removed ----------

    #[test]
    fn guard_removed_fires_when_guarded_call_escapes() {
        // chargeCard ran behind a guard; now it runs unconditionally.
        let node = modified(
            "FunctionDeclaration",
            "if (isVerified(order)) {\n    chargeCard(order);\n}",
            "chargeCard(order);",
        );
        assert_eq!(fired(&node, false), Some("guard-removed"));
    }

    #[test]
    fn guard_removed_ignores_early_return_refactor() {
        // Inverting a wrapping if into an early-return guard clause is a refactor,
        // not a removal — the call is still gated.
        let node = modified(
            "FunctionDeclaration",
            "if (isVerified(order)) {\n    chargeCard(order);\n}",
            "if (!isVerified(order)) {\n    return;\n}\nchargeCard(order);",
        );
        assert_eq!(fired(&node, false), None);
    }

    #[test]
    fn guard_removed_ignores_try_with_resources_refactor() {
        // Wrapping a guarded call in try-with-resources keeps the guard intact.
        let node = modified(
            "FunctionDeclaration",
            "if (isOpen(conn)) {\n    write(conn, data);\n}",
            "if (isOpen(conn)) {\n    try (var s = conn.session()) {\n        write(s, data);\n    }\n}",
        );
        assert_eq!(fired(&node, false), None);
    }

    // ---------- error-handling-removed (TryBlock) ----------

    #[test]
    fn error_handling_removed_fires_when_try_vanishes() {
        // try/catch around a surviving call removed — failures now unhandled.
        let node = modified(
            "FunctionDeclaration",
            "try {\n    send(payload);\n} catch (IOException e) {\n    log(e);\n}",
            "send(payload);",
        );
        assert_eq!(fired(&node, false), Some("removed-try-catch"));
    }

    #[test]
    fn error_handling_removed_ignores_try_with_resources_refactor() {
        // try -> try-with-resources still has a try wrapping the call: not removed.
        let node = modified(
            "FunctionDeclaration",
            "try {\n    InputStream s = open(path);\n    read(s);\n} catch (IOException e) {\n    log(e);\n}",
            "try (InputStream s = open(path)) {\n    read(s);\n} catch (IOException e) {\n    log(e);\n}",
        );
        assert_eq!(fired(&node, false), None);
    }

    // ---------- eval-call ----------

    #[test]
    fn eval_call_fires_on_script_engine() {
        let node = modified(
            "ExpressionStatement",
            "int n = compute(input);",
            "ScriptEngine engine = new ScriptEngineManager().getEngineByName(\"js\");\nObject n = engine.eval(input);",
        );
        assert_eq!(fired(&node, false), Some("eval-call"));
    }

    #[test]
    fn eval_call_ignores_ordinary_evaluate_method() {
        // A domain method named evaluate(...) is not dynamic code execution.
        let node = modified(
            "ExpressionStatement",
            "var r = scorer.score(input);",
            "var r = scorer.evaluate(input);",
        );
        assert_eq!(fired(&node, false), None);
    }

    // ---------- child-process ----------

    #[test]
    fn child_process_fires_on_runtime_exec() {
        let node = modified(
            "ExpressionStatement",
            "var out = render(cmd);",
            "Process p = Runtime.getRuntime().exec(cmd);",
        );
        assert_eq!(fired(&node, false), Some("child-process"));
    }

    #[test]
    fn child_process_fires_on_process_builder() {
        let node = added("ExpressionStatement", "var pb = new ProcessBuilder(\"ls\", \"-la\");");
        assert_eq!(fired(&node, false), Some("child-process"));
    }

    #[test]
    fn child_process_silent_in_test_file() {
        // A subprocess spawned in a test harness is suppressed.
        let node = added("ExpressionStatement", "Process p = Runtime.getRuntime().exec(cmd);");
        assert_eq!(fired(&node, true), None);
    }

    // ---------- tls-disable ----------

    #[test]
    fn tls_disable_fires_on_allow_all_hostname_verifier() {
        let node = modified(
            "ExpressionStatement",
            "conn.setHostnameVerifier(strictVerifier);",
            "conn.setHostnameVerifier(SSLConnectionSocketFactory.ALLOW_ALL_HOSTNAME_VERIFIER);",
        );
        assert_eq!(fired(&node, false), Some("tls-reject-false"));
    }

    #[test]
    fn tls_disable_fires_on_always_true_lambda() {
        let node = modified(
            "ExpressionStatement",
            "builder.setSSLHostnameVerifier(defaultVerifier);",
            "builder.setHostnameVerifier((hostname, session) -> true);",
        );
        assert_eq!(fired(&node, false), Some("tls-reject-false"));
    }

    #[test]
    fn tls_disable_ignores_strict_verifier_config() {
        // Configuring a strict verifier is normal, secure code.
        let node = modified(
            "ExpressionStatement",
            "conn.setHostnameVerifier(old);",
            "conn.setHostnameVerifier(new DefaultHostnameVerifier());",
        );
        assert_eq!(fired(&node, false), None);
    }

    // ---------- loose-regex ----------

    #[test]
    fn loose_regex_fires_when_pattern_widened_to_catch_all() {
        let node = modified(
            "VariableDeclaration",
            "Pattern p = Pattern.compile(\"^[a-z0-9]+$\");",
            "Pattern p = Pattern.compile(\".*\");",
        );
        assert_eq!(fired(&node, false), Some("loose-regex"));
    }

    #[test]
    fn loose_regex_ignores_anchored_safe_constant() {
        // An anchored, bounded pattern is not a weakening.
        let node = modified(
            "VariableDeclaration",
            "Pattern p = Pattern.compile(\"^[a-z]{2,8}$\");",
            "Pattern p = Pattern.compile(\"^[a-z]{2,16}$\");",
        );
        assert_eq!(fired(&node, false), None);
    }

    // ---------- broadened-cors ----------

    #[test]
    fn broadened_cors_fires_on_wildcard_crossorigin() {
        let node = modified(
            "FunctionDeclaration",
            "@CrossOrigin(origins = \"https://app.example.com\")\npublic List<Order> list() { return repo.all(); }",
            "@CrossOrigin(origins = \"*\")\npublic List<Order> list() { return repo.all(); }",
        );
        assert_eq!(fired(&node, false), Some("broadened-cors"));
    }

    #[test]
    fn broadened_cors_fires_on_add_allowed_origin_wildcard() {
        let node = modified(
            "ExpressionStatement",
            "config.addAllowedOrigin(\"https://app.example.com\");",
            "config.addAllowedOrigin(\"*\");",
        );
        assert_eq!(fired(&node, false), Some("broadened-cors"));
    }

    #[test]
    fn broadened_cors_ignores_named_origin() {
        let node = modified(
            "ExpressionStatement",
            "config.addAllowedOrigin(\"https://old.example.com\");",
            "config.addAllowedOrigin(\"https://new.example.com\");",
        );
        assert_eq!(fired(&node, false), None);
    }

    // ---------- cookie httponly / secure ----------

    #[test]
    fn cookie_httponly_removed_fires() {
        let node = modified(
            "ExpressionStatement",
            "cookie.setHttpOnly(true);\ncookie.setPath(\"/\");",
            "cookie.setPath(\"/\");",
        );
        assert_eq!(fired(&node, false), Some("cookie-httponly-removed"));
    }

    #[test]
    fn cookie_secure_removed_fires() {
        let node = modified(
            "ExpressionStatement",
            "cookie.setSecure(true);\ncookie.setPath(\"/\");",
            "cookie.setPath(\"/\");",
        );
        assert_eq!(fired(&node, false), Some("cookie-secure-removed"));
    }

    #[test]
    fn cookie_httponly_kept_does_not_flag() {
        // Reordering while keeping setHttpOnly(true) must not flag.
        let node = modified(
            "ExpressionStatement",
            "cookie.setPath(\"/\");\ncookie.setHttpOnly(true);",
            "cookie.setHttpOnly(true);\ncookie.setPath(\"/\");",
        );
        assert_eq!(fired(&node, false), None);
    }

    // ---------- samesite weakened ----------

    #[test]
    fn samesite_weakened_fires_on_strict_to_none() {
        let node = modified(
            "ExpressionStatement",
            "ResponseCookie c = ResponseCookie.from(\"sid\", v).sameSite(\"Strict\").build();",
            "ResponseCookie c = ResponseCookie.from(\"sid\", v).sameSite(\"None\").build();",
        );
        assert_eq!(fired(&node, false), Some("samesite-weakened"));
    }

    #[test]
    fn samesite_lax_kept_does_not_flag() {
        let node = modified(
            "ExpressionStatement",
            "ResponseCookie c = ResponseCookie.from(\"sid\", v).sameSite(\"Lax\").build();",
            "ResponseCookie c = ResponseCookie.from(\"sid\", v).sameSite(\"Lax\").secure(true).build();",
        );
        assert_eq!(fired(&node, false), None);
    }

    // ---------- broad idiomatic-negative sweep ----------

    #[test]
    fn idiomatic_overload_heavy_class_does_not_flag() {
        // Overloads disambiguated by signature; a benign body edit must stay quiet.
        let node = modified(
            "FunctionDeclaration",
            "public int area(int w, int h) {\n    return w * h;\n}",
            "public int area(int w, int h) {\n    return Math.multiplyExact(w, h);\n}",
        );
        assert_eq!(fired(&node, false), None);
    }

    #[test]
    fn idiomatic_try_with_resources_intro_does_not_flag() {
        // Introducing try-with-resources around new code is safe, not a removal.
        let node = modified(
            "FunctionDeclaration",
            "var data = read(path);\nreturn parse(data);",
            "try (var in = open(path)) {\n    var data = read(in);\n    return parse(data);\n}",
        );
        assert_eq!(fired(&node, false), None);
    }
}
