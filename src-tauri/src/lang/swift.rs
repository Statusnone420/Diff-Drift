//! Swift language pack. Fills the structural queries and marker tables the
//! security rules consult so they run against Swift syntax (not JS node kinds or
//! JS text regexes). Each filled field is backed by a positive and an
//! idiomatic-negative unit test below.
//!
//! Grammar ground truth (tree-sitter-swift 0.7.3, confirmed by dumping parse
//! trees): `if_statement` carries its test under the `condition:` field and its
//! body as an unfielded `(statements)` child (the `else` block is also
//! `(statements)`, so the consequence query over-captures toward "still
//! guarded"). A bare call is `(call_expression (simple_identifier) ...)` and a
//! member call is `(call_expression (navigation_expression suffix:
//! (navigation_suffix suffix: (simple_identifier))))`. `do { … } catch { … }`
//! is `(do_statement (statements …) (catch_block …))`.
//!
//! `error_handling_strategy` is `TryBlock`: Swift has `do`/`catch`, so
//! `ErrorHandlingRemoved` uses `try_block` to detect a wrapping `do` block that
//! disappeared while its calls survived.
//!
//! Honest skips for Swift (no pack field, the rule stays silent):
//! - cookies (`cookie_*`): Apple cookie attributes live in app config / the ATS
//!   plist and `HTTPCookie` property dictionaries, not as a stable source idiom
//!   — and Swift is a client platform, so the weakened-cookie rules are gated
//!   off for Swift in `rules.rs` (`ALL_EXCEPT_SWIFT`).
//! - `eval_call`: Swift has no runtime string-eval primitive.
//! - `env_tls_disable`: no process-wide TLS-disabling env var convention.
use super::{ErrorHandlingStrategy, FamilyPack};

pub static PACK: FamilyPack = FamilyPack {
    // --- structural queries ---
    // `if cond { … }` — the condition expression. `condition` is a `multiple`
    // field in the grammar, but for an ordinary single-test `if` it captures the
    // one condition node (e.g. `false`, `isValid(x)`).
    if_condition: Some("(if_statement condition: (_) @cond)"),
    // The body of an `if`. Swift surfaces it as an unfielded `(statements)`
    // child; the `else` arm is also `(statements)`, so this can capture both —
    // which over-counts guarded calls (suppression direction), never under.
    if_consequence: Some("(if_statement (statements) @cons)"),
    // Every call's callee: bare `foo(x)` is the call_expression's leading
    // `simple_identifier`; member `obj.foo(x)` puts the name in the trailing
    // `navigation_suffix`. Nested calls (`save(sanitize(x))`) each match, so a
    // stripped wrapper is detected.
    callee: Some(
        "(call_expression (simple_identifier) @callee) \
         (call_expression (navigation_expression suffix: (navigation_suffix suffix: (simple_identifier) @callee)))",
    ),
    // `do { … } catch { … }` — Swift's try/catch equivalent.
    try_block: Some("(do_statement) @try"),

    // --- literal tables ---
    // Swift's only constant-falsy `if` condition idiom.
    falsy_literals: &["false"],
    // Early-exit keywords marking an `if` consequence as a guard clause.
    guard_exit_keywords: &["return", "throw", "break", "continue"],

    // --- error handling ---
    // TryBlock strategy: Swift has `do`/`catch`; it does not use the
    // UnwrapTransition markers (those stay empty via the EMPTY spread).
    error_handling_strategy: ErrorHandlingStrategy::TryBlock,

    // --- high-severity marker regexes ---
    // Subprocess: Foundation `Process()` / the older `NSTask()`, plus the
    // `launchPath`/`executableURL` + `/bin/sh` shell-out idiom. Any newly
    // appearing marker fires. (No import-shaped signal — subprocess_import stays
    // None via the spread.)
    subprocess_call: Some(r#"(\bProcess\s*\(\s*\)|\bNSTask\s*\(|\.launchPath\s*=|\.executableURL\s*=|/bin/(?:ba)?sh)"#),
    // TLS: the three mainstream ways Swift code turns off certificate
    // validation. URLSession's challenge handler answering `.useCredential`
    // with a server-trust credential (accept-anything), and Alamofire's
    // `DisabledTrustEvaluator` / `disableEvaluation` / `allowInvalidCertificates`.
    tls_disable: Some(
        r#"(URLCredential\s*\(\s*trust\s*:|DisabledTrustEvaluator|disableEvaluation|allowInvalidCertificates\s*:\s*true)"#,
    ),
    // Regex construction whose pattern is a string argument: `Regex("…")`,
    // `try Regex("…")`, and `NSRegularExpression(pattern: "…")`. The optional
    // `pattern:` label lets one capture group (group 1 = the pattern body) cover
    // both call shapes; the loose-regex rule feeds group 1 through the shared
    // weakening comparison.
    regex_compile: Some(
        r#"(?:NSRegularExpression|Regex)\s*\(\s*(?:pattern\s*:\s*)?"([^"]*)""#,
    ),
    // CORS: Vapor opens cross-origin access with `allowedOrigin: .all` (or the
    // wildcard string form).
    cors_permissive: Some(r#"allowedOrigin\s*:\s*(?:\.all\b|\.custom\s*\(\s*"\*"|"\*")"#),

    // Everything left at its EMPTY default — the honest Swift skips documented in
    // the module header: env_tls_disable, eval_call, and all cookie_* fields
    // (Swift is gated out of the cookie rules), plus the UnwrapTransition markers.
    ..FamilyPack::EMPTY
};

#[cfg(test)]
mod tests {
    use crate::model::{AstNode, NodeState};
    use crate::parse::Lang;
    use crate::rules::{registry, RuleCtx};
    use std::collections::HashSet;

    fn ctx() -> RuleCtx {
        RuleCtx {
            deps: HashSet::new(),
            is_test_file: false,
            is_build_script: false,
            lang: Lang::Swift,
        }
    }

    fn test_ctx() -> RuleCtx {
        RuleCtx {
            deps: HashSet::new(),
            is_test_file: true,
            is_build_script: false,
            lang: Lang::Swift,
        }
    }

    fn lines(s: &str) -> Option<Vec<String>> {
        Some(s.lines().map(|l| l.to_string()).collect())
    }

    fn node(kind: &str, state: NodeState, before: &str, after: &str) -> AstNode {
        AstNode {
            id: "t".into(),
            kind: kind.into(),
            name: "n".into(),
            signature: None,
            state,
            reviewed: false,
            flag_id: None,
            before: lines(before),
            after: lines(after),
            children: None,
        }
    }

    fn fired(node: &AstNode, ctx: &RuleCtx) -> Option<&'static str> {
        registry().check(node, ctx).map(|(id, _)| id)
    }

    // ---------------- removed-if-guard ----------------

    #[test]
    fn removed_if_guard_fires_on_constant_false() {
        let n = node(
            "IfStatement",
            NodeState::Modified,
            "if isAdmin(user) {\n    audit()\n}",
            "if false {\n    audit()\n}",
        );
        assert_eq!(fired(&n, &ctx()), Some("removed-if-guard"));
    }

    #[test]
    fn removed_if_guard_ignores_live_condition_tightening() {
        // A normal refactor that keeps (and tightens) a live condition: no flag.
        let n = node(
            "IfStatement",
            NodeState::Modified,
            "if ok {\n    run()\n}",
            "if ok && ready {\n    run()\n}",
        );
        assert!(fired(&n, &ctx()).is_none());
    }

    #[test]
    fn removed_if_guard_ignores_false_inside_string() {
        // `false` only as substring of a string literal must not flag.
        let n = node(
            "IfStatement",
            NodeState::Modified,
            "if enabled {\n    log(\"on\")\n}",
            "if enabled {\n    log(\"set to false\")\n}",
        );
        assert!(fired(&n, &ctx()).is_none());
    }

    // ---------------- guard-removed ----------------

    #[test]
    fn guard_removed_fires_when_guarded_call_escapes() {
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            "if isVerified(order) {\n    chargeCard(order)\n}",
            "chargeCard(order)",
        );
        assert_eq!(fired(&n, &ctx()), Some("guard-removed"));
    }

    #[test]
    fn guard_removed_suppressed_when_converted_to_guard_let_early_return() {
        // guard-let refactored to an if-let early-return: protection still
        // exists (an early-exit guard clause was introduced) — no flag.
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            "if let user = current {\n    greet(user)\n}",
            "if current == nil {\n    return\n}\ngreet(current!)",
        );
        assert!(fired(&n, &ctx()).is_none());
    }

    #[test]
    fn guard_removed_not_fired_when_call_still_guarded() {
        // Reordered but still inside the same `if` — no escape.
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            "if isVerified(order) {\n    chargeCard(order)\n}",
            "if isVerified(order) {\n    log(order)\n    chargeCard(order)\n}",
        );
        assert!(fired(&n, &ctx()).is_none());
    }

    // ---------------- removed-sanitize ----------------

    #[test]
    fn removed_sanitize_fires_when_wrapper_stripped() {
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            "store(sanitizeInput(raw))",
            "store(raw)",
        );
        assert_eq!(fired(&n, &ctx()), Some("removed-sanitize"));
    }

    #[test]
    fn removed_sanitize_not_fired_when_sanitizer_retained() {
        // Sanitizer kept (just renamed variable) — no flag.
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            "store(sanitizeInput(raw))",
            "let clean = sanitizeInput(raw)\nstore(clean)",
        );
        assert!(fired(&n, &ctx()).is_none());
    }

    #[test]
    fn removed_sanitize_member_form_detected() {
        // `validator.validateEmail(x)` removed via member-call callee capture.
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            "send(validator.validateEmail(addr))",
            "send(addr)",
        );
        assert_eq!(fired(&n, &ctx()), Some("removed-sanitize"));
    }

    #[test]
    fn removed_sanitize_ignores_validator_definition_churn() {
        // FIX 3 (Alamofire cluster): the matched node is the DEFINITION of a
        // method literally named `validate` whose signature gains `@Sendable`.
        // Its body's internal `validate(...)` self-call is restructured away, but
        // this is the validator's own definition churning — NOT a caller dropping
        // sanitization. Must stay silent. (Without the guard the lost internal
        // `validate(...)` callee reads as a removed sanitizer and flags.)
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            "func validate(_ rule: Rule) -> Self {\n    return validate(rule, options: defaults)\n}",
            "@Sendable func validate(_ rule: Rule) -> Self {\n    return Self(rule)\n}",
        );
        // `node.name` carries the declared method name; mirror what the parser
        // surfaces for a Swift function declaration.
        let mut n = n;
        n.name = "validate".into();
        assert!(
            fired(&n, &ctx()).is_none(),
            "a validator definition churning must not read as removed sanitization"
        );
    }

    // ---------------- verify-to-decode ----------------

    #[test]
    fn verify_to_decode_fires_on_member_calls() {
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            "let claims = jwt.verify(token)",
            "let claims = jwt.decode(token)",
        );
        assert_eq!(fired(&n, &ctx()), Some("verify-to-decode"));
    }

    #[test]
    fn verify_to_decode_not_fired_when_verify_retained() {
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            "let claims = jwt.verify(token)",
            "let claims = jwt.verify(token, options)",
        );
        assert!(fired(&n, &ctx()).is_none());
    }

    // ---------------- error-handling-removed (do/catch) ----------------

    #[test]
    fn error_handling_removed_fires_when_do_catch_dropped() {
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            "do {\n    try persist(record)\n} catch {\n    log(error)\n}",
            "let saved = try? persist(record)",
        );
        assert_eq!(fired(&n, &ctx()), Some("removed-try-catch"));
    }

    #[test]
    fn error_handling_removed_suppressed_when_moved_into_helper() {
        // do/catch moved into a called helper; the wrapped call is now behind a
        // different name — the original call no longer appears unwrapped here,
        // so no flag.
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            "do {\n    try persist(record)\n} catch {\n    log(error)\n}",
            "persistSafely(record)",
        );
        assert!(fired(&n, &ctx()).is_none());
    }

    #[test]
    fn error_handling_removed_not_fired_when_do_catch_retained() {
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            "do {\n    try persist(record)\n} catch {\n    log(error)\n}",
            "do {\n    try persist(record)\n    try flush()\n} catch {\n    report(error)\n}",
        );
        assert!(fired(&n, &ctx()).is_none());
    }

    // ---------------- subprocess ----------------

    #[test]
    fn subprocess_fires_on_process_launch() {
        let n = node(
            "FunctionDeclaration",
            NodeState::Added,
            "",
            "let task = Process()\ntask.launchPath = \"/bin/sh\"\ntask.arguments = [\"-c\", cmd]\ntask.launch()",
        );
        assert_eq!(fired(&n, &ctx()), Some("child-process"));
    }

    #[test]
    fn subprocess_suppressed_in_test_file() {
        let n = node(
            "FunctionDeclaration",
            NodeState::Added,
            "",
            "let task = Process()\ntask.launchPath = \"/bin/sh\"",
        );
        assert!(fired(&n, &test_ctx()).is_none());
    }

    #[test]
    fn subprocess_not_fired_on_unrelated_process_word() {
        // `processOrder(` is not the `Process()` constructor — must not flag.
        let n = node(
            "FunctionDeclaration",
            NodeState::Added,
            "",
            "processOrder(order)\nlet status = inProcess",
        );
        assert!(fired(&n, &ctx()).is_none());
    }

    #[test]
    fn subprocess_ignores_marker_in_comment_or_string() {
        // FIX 1: the `Process()` shell-out idiom named only in a comment/string
        // must not fire.
        let comment = node(
            "FunctionDeclaration",
            NodeState::Added,
            "",
            "// never launch Process() with launchPath = \"/bin/sh\" on user input\nhandle(cmd)",
        );
        assert!(fired(&comment, &ctx()).is_none(), "marker in comment");
        let string = node(
            "FunctionDeclaration",
            NodeState::Added,
            "",
            "log(\"helper wraps Process() and /bin/sh\")",
        );
        assert!(fired(&string, &ctx()).is_none(), "marker in string");
    }

    // ---------------- tls-disable ----------------

    #[test]
    fn tls_disable_fires_on_server_trust_credential() {
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            "completionHandler(.performDefaultHandling, nil)",
            "completionHandler(.useCredential, URLCredential(trust: challenge.protectionSpace.serverTrust!))",
        );
        assert_eq!(fired(&n, &ctx()), Some("tls-reject-false"));
    }

    #[test]
    fn tls_disable_fires_on_alamofire_disabled_evaluator() {
        let n = node(
            "VariableDeclaration",
            NodeState::Modified,
            "let evaluators = [\"api.example.com\": PinnedCertificatesTrustEvaluator()]",
            "let evaluators = [\"api.example.com\": DisabledTrustEvaluator()]",
        );
        assert_eq!(fired(&n, &ctx()), Some("tls-reject-false"));
    }

    #[test]
    fn tls_disable_not_fired_on_default_secure_handling() {
        // Idiomatic URLSession default handling — no accept-anything credential.
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            "completionHandler(.performDefaultHandling, nil)",
            "completionHandler(.performDefaultHandling, nil)\nlog(challenge)",
        );
        assert!(fired(&n, &ctx()).is_none());
    }

    #[test]
    fn tls_disable_ignores_marker_in_comment_or_string() {
        // FIX 1: `DisabledTrustEvaluator` named only in a comment/string must not
        // fire — Alamofire docs/log lines reference it without using it.
        let comment = node(
            "VariableDeclaration",
            NodeState::Modified,
            "let evaluators = [\"api.example.com\": PinnedCertificatesTrustEvaluator()]",
            "// do NOT swap in DisabledTrustEvaluator() here\nlet evaluators = [\"api.example.com\": PinnedCertificatesTrustEvaluator()]",
        );
        assert!(fired(&comment, &ctx()).is_none(), "marker in comment");
        let string = node(
            "FunctionDeclaration",
            NodeState::Modified,
            "configure()",
            "fatalError(\"DisabledTrustEvaluator is banned in release builds\")",
        );
        assert!(fired(&string, &ctx()).is_none(), "marker in string");
    }

    #[test]
    fn tls_disable_not_fired_when_present_in_both() {
        // Differential: already-disabled in before stays unflagged (no new
        // weakening).
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            "completionHandler(.useCredential, URLCredential(trust: t))",
            "completionHandler(.useCredential, URLCredential(trust: t))\nlog(host)",
        );
        assert!(fired(&n, &ctx()).is_none());
    }

    // ---------------- loose-regex ----------------

    #[test]
    fn loose_regex_fires_when_anchors_dropped() {
        let n = node(
            "VariableDeclaration",
            NodeState::Modified,
            "let emailRegex = NSRegularExpression(pattern: \"^[a-z]+@[a-z]+$\")",
            "let emailRegex = NSRegularExpression(pattern: \"[a-z]+@[a-z]+\")",
        );
        assert_eq!(fired(&n, &ctx()), Some("loose-regex"));
    }

    #[test]
    fn loose_regex_fires_on_catch_all_regex_type() {
        let n = node(
            "VariableDeclaration",
            NodeState::Modified,
            "let token = try Regex(\"^[A-Z0-9]{8}$\")",
            "let token = try Regex(\".*\")",
        );
        assert_eq!(fired(&n, &ctx()), Some("loose-regex"));
    }

    #[test]
    fn loose_regex_not_fired_when_pattern_unchanged() {
        let n = node(
            "VariableDeclaration",
            NodeState::Modified,
            "let r = NSRegularExpression(pattern: \"^[a-z]+$\")",
            "let r = NSRegularExpression(pattern: \"^[a-z]+$\")\nlet opts = []",
        );
        assert!(fired(&n, &ctx()).is_none());
    }

    // ---------------- cors-permissive ----------------

    #[test]
    fn cors_permissive_fires_on_vapor_allow_all() {
        let n = node(
            "VariableDeclaration",
            NodeState::Modified,
            "let cors = CORSMiddleware.Configuration(allowedOrigin: .custom(\"https://app.example.com\"), allowedMethods: [.GET])",
            "let cors = CORSMiddleware.Configuration(allowedOrigin: .all, allowedMethods: [.GET])",
        );
        assert_eq!(fired(&n, &ctx()), Some("broadened-cors"));
    }

    #[test]
    fn cors_permissive_not_fired_on_specific_origin() {
        let n = node(
            "VariableDeclaration",
            NodeState::Modified,
            "let cors = CORSMiddleware.Configuration(allowedOrigin: .custom(\"https://app.example.com\"))",
            "let cors = CORSMiddleware.Configuration(allowedOrigin: .custom(\"https://www.example.com\"))",
        );
        assert!(fired(&n, &ctx()).is_none());
    }

    #[test]
    fn cors_permissive_ignores_marker_in_comment() {
        // FIX 1: `allowedOrigin: .all` named only in a comment must not fire.
        let n = node(
            "VariableDeclaration",
            NodeState::Modified,
            "let cors = CORSMiddleware.Configuration(allowedOrigin: .custom(\"https://app.example.com\"))",
            "// never set allowedOrigin: .all in production\nlet cors = CORSMiddleware.Configuration(allowedOrigin: .custom(\"https://app.example.com\"))",
        );
        assert!(fired(&n, &ctx()).is_none());
    }

    // ---------------- idiomatic-negative: nothing flags on a benign refactor ----------------

    #[test]
    fn benign_refactor_is_quiet() {
        // Rename + reformat + keep the guard: no flag of any kind.
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            "if isVerified(o) {\n    charge(o)\n}",
            "if isVerified(order) {\n    charge(order)\n}",
        );
        assert!(fired(&n, &ctx()).is_none());
    }

    #[test]
    fn tls_disable_suppressed_in_test_file() {
        // The TLS rule honors is_test_file (a FooTests.swift fixture that accepts
        // any server trust is test-only, not production drift).
        let n = node(
            "FunctionDeclaration",
            NodeState::Modified,
            "completionHandler(.performDefaultHandling, nil)",
            "completionHandler(.useCredential, URLCredential(trust: t))",
        );
        assert!(fired(&n, &test_ctx()).is_none());
    }
}
