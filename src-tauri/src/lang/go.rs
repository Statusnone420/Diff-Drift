//! Go language pack. Fills the structural queries and high-severity marker
//! tables the security rules consult for Go, each grounded against the
//! tree-sitter-go grammar and exercised by the unit tests below.
//!
//! `error_handling_strategy` is `CoveredByGuards`: Go has no try/catch and its
//! error handling is `if err != nil { return … }`, already surfaced by the
//! guard rules — so `ErrorHandlingRemoved` does not run for Go (per the matrix,
//! Go is the one family that rule skips). `eval` (no runtime eval) and
//! env-TLS / SameSite-strong/weak pairing are likewise left empty where Go has
//! no idiomatic equivalent; an empty field keeps the matching rule silent.
use super::{ErrorHandlingStrategy, FamilyPack};

pub static PACK: FamilyPack = FamilyPack {
    // ---- structural queries ----
    // `if_statement` has fields `condition` and `consequence: (block)`.
    if_condition: Some("(if_statement condition: (_) @cond)"),
    if_consequence: Some("(if_statement consequence: (block) @cons)"),
    // Every call's callee name: a bare `f(…)` and a selector `pkg.F(…)` /
    // `recv.Method(…)`. `call_expression`'s `function` is either an
    // `identifier` or a `selector_expression` whose `field` is a
    // `field_identifier`.
    callee: Some(
        "(call_expression function: (identifier) @callee) \
         (call_expression function: (selector_expression field: (field_identifier) @callee))",
    ),
    // Go has no try/catch construct; `try_block` stays None.

    // ---- literal tables ----
    falsy_literals: &["false"],
    // Early-exit keywords that mark an `if` consequence as a guard clause. Go's
    // early exits are `return`, `panic`, `break`, `continue`.
    guard_exit_keywords: &["return", "panic", "break", "continue"],

    // ---- error-handling strategy (already set in skeleton; keep) ----
    error_handling_strategy: ErrorHandlingStrategy::CoveredByGuards,

    // ---- high-severity markers (regex sources) ----
    // ChildProcess: importing os/exec, or calling exec.Command / exec.CommandContext.
    subprocess_import: Some(r#"["']os/exec["']"#),
    subprocess_call: Some(r"\bexec\.Command(Context)?\s*\("),
    // TlsRejectFalse: tls.Config{ InsecureSkipVerify: true }.
    tls_disable: Some(r"InsecureSkipVerify\s*:\s*true"),
    // LooseRegex: regexp.Compile / regexp.MustCompile / regexp.CompilePOSIX with
    // a raw-string-or-double-quoted pattern. Group 1 = the pattern body.
    regex_compile: Some(r#"regexp\.(?:MustCompile|Compile|CompilePOSIX|MustCompilePOSIX)\s*\(\s*[`"]([^`"]*)[`"]"#),
    // BroadenedCors: a wildcard origin opened in any of the common Go stacks —
    // gin's AllowAllOrigins, gin's AllowOrigins with "*", gorilla/handlers'
    // AllowedOrigins with "*", or a raw Access-Control-Allow-Origin: * header.
    cors_permissive: Some(
        r#"(AllowAllOrigins\s*:\s*true|Allow(?:ed)?Origins\s*:\s*\[\]string\{[^}]*"\*"|AllowedOrigins\s*\(\s*\[\]string\{[^}]*"\*"|Access-Control-Allow-Origin["'\s,)]*\*)"#,
    ),
    // Cookie flags on an http.Cookie literal. Removal (matched before, not
    // after) fires Cookie*Removed; SameSite strong→weak fires SameSiteWeakened.
    cookie_httponly: Some(r"HttpOnly\s*:\s*true"),
    cookie_secure: Some(r"Secure\s*:\s*true"),
    cookie_samesite: Some(r"SameSite\s*:\s*http\.SameSite(?:Strict|Lax)Mode"),
    cookie_samesite_weak: Some(r"SameSite\s*:\s*http\.SameSiteNoneMode"),

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
            lang: Lang::Go,
        }
    }

    fn test_ctx() -> RuleCtx {
        RuleCtx {
            deps: HashSet::new(),
            is_test_file: true,
            is_build_script: false,
            lang: Lang::Go,
        }
    }

    fn node(kind: &str, before: &[&str], after: &[&str]) -> AstNode {
        AstNode {
            id: "t".into(),
            kind: kind.into(),
            name: "n".into(),
            signature: None,
            state: NodeState::Modified,
            reviewed: false,
            flag_id: None,
            before: Some(before.iter().map(|s| s.to_string()).collect()),
            after: Some(after.iter().map(|s| s.to_string()).collect()),
            children: None,
        }
    }

    fn added(kind: &str, after: &[&str]) -> AstNode {
        let mut n = node(kind, &[], after);
        n.state = NodeState::Added;
        n.before = None;
        n
    }

    fn fired(n: &AstNode, c: &RuleCtx) -> Option<&'static str> {
        registry().check(n, c).map(|(id, _)| id)
    }

    // ---------------- guard-removed ----------------

    #[test]
    fn guard_removed_fires_when_guarded_call_runs_unconditionally() {
        // A call that ran inside an `if` guard before now runs unconditionally.
        let n = node(
            "FunctionDeclaration",
            &[
                "func capture(o Order) {",
                "    if isVerified(o) {",
                "        chargeCard(o)",
                "    }",
                "}",
            ],
            &["func capture(o Order) {", "    chargeCard(o)", "}"],
        );
        assert_eq!(fired(&n, &ctx()), Some("guard-removed"));
    }

    #[test]
    fn guard_removed_negative_err_check_refactored_into_helper_keeps_a_check() {
        // Idiomatic Go: an `if err != nil { return err }` check refactored into a
        // wrapping helper (`fmt.Errorf`) where a check still exists must NOT flag.
        let n = node(
            "FunctionDeclaration",
            &[
                "func load(p string) ([]byte, error) {",
                "    b, err := read(p)",
                "    if err != nil {",
                "        return nil, err",
                "    }",
                "    return b, nil",
                "}",
            ],
            &[
                "func load(p string) ([]byte, error) {",
                "    b, err := read(p)",
                "    if err != nil {",
                "        return nil, fmt.Errorf(\"read %s: %w\", p, err)",
                "    }",
                "    return b, nil",
                "}",
            ],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    #[test]
    fn guard_removed_negative_guard_clause_hoisted() {
        // Converting a wrapping `if` into an inverted early-return guard clause is
        // the dominant guard refactor and must not read as a removed guard.
        let n = node(
            "FunctionDeclaration",
            &[
                "func capture(o Order) error {",
                "    if isVerified(o) {",
                "        chargeCard(o)",
                "    }",
                "    return nil",
                "}",
            ],
            &[
                "func capture(o Order) error {",
                "    if !isVerified(o) {",
                "        return errUnverified",
                "    }",
                "    chargeCard(o)",
                "    return nil",
                "}",
            ],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    // ---------------- removed-if-guard ----------------

    #[test]
    fn removed_if_guard_fires_on_constant_false() {
        let n = node(
            "IfStatement",
            &["if isAdmin(u) {", "    audit()", "}"],
            &["if false {", "    audit()", "}"],
        );
        assert_eq!(fired(&n, &ctx()), Some("removed-if-guard"));
    }

    #[test]
    fn removed_if_guard_negative_live_condition_tightened() {
        // A normal refactor that keeps a live condition must not flag.
        let n = node(
            "IfStatement",
            &["if ok {", "    run()", "}"],
            &["if ok && ready {", "    run()", "}"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    // ---------------- removed-sanitize ----------------

    #[test]
    fn removed_sanitize_fires_when_validation_call_dropped() {
        let n = node(
            "ExpressionStatement",
            &["save(sanitizeInput(body))"],
            &["save(body)"],
        );
        assert_eq!(fired(&n, &ctx()), Some("removed-sanitize"));
    }

    #[test]
    fn removed_sanitize_negative_call_still_present() {
        // Renaming the variable while keeping the sanitizer call must not flag.
        let n = node(
            "ExpressionStatement",
            &["save(sanitizeInput(body))"],
            &["store(sanitizeInput(payload))"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    // ---------------- verify-to-decode ----------------

    #[test]
    fn verify_to_decode_fires_on_downgrade() {
        // Idiomatic Go uses lowercase (package-private) helpers for the token
        // path. A signature `verifyToken` replaced by a non-verifying
        // `decodeToken` is the downgrade this rule watches for.
        let n = node(
            "VariableDeclaration",
            &["claims, err := verifyToken(raw)"],
            &["claims, err := decodeToken(raw)"],
        );
        assert_eq!(fired(&n, &ctx()), Some("verify-to-decode"));
    }

    #[test]
    fn verify_to_decode_negative_still_verifies() {
        let n = node(
            "VariableDeclaration",
            &["claims, err := verifyToken(raw)"],
            &["claims, err := verifyTokenWithLeeway(raw)"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    // ---------------- child-process (subprocess) ----------------

    #[test]
    fn child_process_fires_on_exec_command() {
        let n = node(
            "ExpressionStatement",
            &["out, err := run(args)"],
            &["out, err := exec.Command(\"sh\", \"-c\", args).Output()"],
        );
        assert_eq!(fired(&n, &ctx()), Some("child-process"));
    }

    #[test]
    fn child_process_fires_on_os_exec_import() {
        let n = node(
            "ImportDeclaration",
            &["import \"fmt\""],
            &["import \"os/exec\""],
        );
        assert_eq!(fired(&n, &ctx()), Some("child-process"));
    }

    #[test]
    fn child_process_negative_in_test_file() {
        // An exec.Command in a *_test.go file is build tooling, not a finding.
        let n = node(
            "ExpressionStatement",
            &["out, err := run(args)"],
            &["out, err := exec.Command(\"go\", \"build\").Output()"],
        );
        assert_eq!(fired(&n, &test_ctx()), None);
    }

    #[test]
    fn child_process_ignores_marker_in_comment_or_string() {
        // FIX 1: an `exec.Command(` named only in a comment/string is not a call.
        let comment = node(
            "ExpressionStatement",
            &["out := run(args)"],
            &["// never exec.Command(\"sh\", \"-c\", args) on user input", "out := run(args)"],
        );
        assert_eq!(fired(&comment, &ctx()), None, "marker in comment");
        let string = node(
            "VariableDeclaration",
            &["doc := \"helper\""],
            &["doc := \"wraps exec.Command(name) for tests\""],
        );
        assert_eq!(fired(&string, &ctx()), None, "marker in string");
        // The os/exec import string is a module specifier — still detected (the
        // import path is intentionally not blanked).
        let import = node(
            "ImportDeclaration",
            &["import \"fmt\""],
            &["import \"os/exec\""],
        );
        assert_eq!(fired(&import, &ctx()), Some("child-process"), "real import still fires");
    }

    // ---------------- tls-disable ----------------

    #[test]
    fn tls_disable_fires_on_insecure_skip_verify() {
        let n = node(
            "VariableDeclaration",
            &["cfg := &tls.Config{}"],
            &["cfg := &tls.Config{InsecureSkipVerify: true}"],
        );
        assert_eq!(fired(&n, &ctx()), Some("tls-reject-false"));
    }

    #[test]
    fn tls_disable_ignores_marker_in_comment_or_string() {
        // FIX 1: `InsecureSkipVerify: true` named only in a comment or an error
        // string must not fire.
        let comment = node(
            "VariableDeclaration",
            &["cfg := &tls.Config{}"],
            &["// setting InsecureSkipVerify: true would be insecure", "cfg := &tls.Config{}"],
        );
        assert_eq!(fired(&comment, &ctx()), None, "marker in comment");
        let string = node(
            "ExpressionStatement",
            &["log.Print(\"tls ok\")"],
            &["log.Fatal(\"refusing InsecureSkipVerify: true\")"],
        );
        assert_eq!(fired(&string, &ctx()), None, "marker in string");
    }

    #[test]
    fn tls_disable_negative_already_present() {
        // Differential: a marker in BOTH before and after is not newly introduced.
        let n = node(
            "VariableDeclaration",
            &["cfg := &tls.Config{InsecureSkipVerify: true}"],
            &["cfg := &tls.Config{InsecureSkipVerify: true, MinVersion: tls.VersionTLS12}"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    #[test]
    fn tls_disable_negative_in_test_file() {
        let n = node(
            "VariableDeclaration",
            &["cfg := &tls.Config{}"],
            &["cfg := &tls.Config{InsecureSkipVerify: true}"],
        );
        assert_eq!(fired(&n, &test_ctx()), None);
    }

    // ---------------- loose-regex ----------------

    #[test]
    fn loose_regex_fires_when_anchors_dropped() {
        let n = node(
            "VariableDeclaration",
            &["re := regexp.MustCompile(`^[a-z]+$`)"],
            &["re := regexp.MustCompile(`[a-z]+`)"],
        );
        assert_eq!(fired(&n, &ctx()), Some("loose-regex"));
    }

    #[test]
    fn loose_regex_fires_when_widened_to_catch_all() {
        let n = added("VariableDeclaration", &["re := regexp.MustCompile(`.*`)"]);
        assert_eq!(fired(&n, &ctx()), Some("loose-regex"));
    }

    #[test]
    fn loose_regex_negative_pattern_unchanged() {
        let n = node(
            "VariableDeclaration",
            &["re := regexp.MustCompile(`^[a-z]+$`)"],
            &["pat := regexp.MustCompile(`^[a-z]+$`)"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    // ---------------- cors-permissive ----------------

    #[test]
    fn cors_permissive_ignores_marker_in_comment() {
        // FIX 1: a permissive-origin marker named only in a comment must not
        // fire. (`AllowAllOrigins: true` is a code field, so the comment-only
        // mention is the false positive to suppress.)
        let n = node(
            "VariableDeclaration",
            &["cfg := cors.Config{AllowAllOrigins: false}"],
            &["// never set cors.Config{AllowAllOrigins: true}", "cfg := cors.Config{AllowAllOrigins: false}"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    #[test]
    fn cors_permissive_fires_on_allow_all_origins() {
        let n = node(
            "VariableDeclaration",
            &["cfg := cors.Config{AllowAllOrigins: false}"],
            &["cfg := cors.Config{AllowAllOrigins: true}"],
        );
        assert_eq!(fired(&n, &ctx()), Some("broadened-cors"));
    }

    #[test]
    fn cors_permissive_fires_on_wildcard_allowed_origins() {
        let n = node(
            "VariableDeclaration",
            &["c := handlers.AllowedOrigins([]string{\"https://app.example.com\"})"],
            &["c := handlers.AllowedOrigins([]string{\"*\"})"],
        );
        assert_eq!(fired(&n, &ctx()), Some("broadened-cors"));
    }

    #[test]
    fn cors_permissive_negative_specific_origin() {
        let n = node(
            "VariableDeclaration",
            &["cfg := cors.Config{AllowOrigins: []string{\"https://a.example.com\"}}"],
            &["cfg := cors.Config{AllowOrigins: []string{\"https://b.example.com\"}}"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    // ---------------- cookies ----------------

    #[test]
    fn cookie_httponly_removed_fires() {
        let n = node(
            "VariableDeclaration",
            &["c := http.Cookie{Name: \"sid\", HttpOnly: true}"],
            &["c := http.Cookie{Name: \"sid\"}"],
        );
        assert_eq!(fired(&n, &ctx()), Some("cookie-httponly-removed"));
    }

    #[test]
    fn cookie_secure_removed_fires() {
        let n = node(
            "VariableDeclaration",
            &["c := http.Cookie{Name: \"sid\", Secure: true}"],
            &["c := http.Cookie{Name: \"sid\"}"],
        );
        assert_eq!(fired(&n, &ctx()), Some("cookie-secure-removed"));
    }

    #[test]
    fn cookie_samesite_weakened_fires() {
        let n = node(
            "VariableDeclaration",
            &["c := http.Cookie{SameSite: http.SameSiteStrictMode}"],
            &["c := http.Cookie{SameSite: http.SameSiteNoneMode}"],
        );
        assert_eq!(fired(&n, &ctx()), Some("samesite-weakened"));
    }

    #[test]
    fn cookie_negative_flag_retained() {
        // HttpOnly present in both versions: nothing removed, no flag.
        let n = node(
            "VariableDeclaration",
            &["c := http.Cookie{Name: \"sid\", HttpOnly: true}"],
            &["c := http.Cookie{Name: \"session\", HttpOnly: true}"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    #[test]
    fn cookie_httponly_ignores_non_cookie_struct() {
        // FIX 2: an `HttpOnly: true` field removed from a non-cookie struct must
        // not read as a weakened cookie — cookie context is required.
        let n = node(
            "VariableDeclaration",
            &["cfg := SessionConfig{HttpOnly: true}"],
            &["cfg := SessionConfig{}"],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }

    #[test]
    fn cookie_samesite_ignores_marker_in_comment() {
        // FIX 1: the SameSite downgrade named only in a comment must not fire.
        let n = node(
            "VariableDeclaration",
            &["c := http.Cookie{SameSite: http.SameSiteStrictMode}"],
            &[
                "c := http.Cookie{SameSite: http.SameSiteStrictMode} // not SameSiteNoneMode",
            ],
        );
        assert_eq!(fired(&n, &ctx()), None);
    }
}
