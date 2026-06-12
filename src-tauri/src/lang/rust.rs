//! Rust language pack. Fills the structural queries + marker tables the security
//! rules consult so they run against the `tree-sitter-rust` grammar instead of
//! the JS node vocabulary. Every field below is grounded in a unit test in this
//! file's `#[cfg(test)]` module (positive + idiomatic-negative).
//!
//! `error_handling_strategy` is `UnwrapTransition`: Rust has no try/catch, so
//! `ErrorHandlingRemoved` detects a `before` that handled a fallible result
//! (`?`/`match`) becoming an `after` that `.unwrap()`/`.expect(...)`s it. The
//! markers live in `unwrap_markers` / `handled_markers` below.
//!
//! ## Grammar facts (tree-sitter-rust)
//! - `if_expression` has fields `condition` (any expression) and `consequence`
//!   (a `block`). It is NOT parenthesized like Java/JS, so the condition query
//!   captures the bare expression.
//! - calls are `call_expression`; the callee (`function:`) is one of
//!   `identifier` (bare `foo()`), `field_expression`/`field_identifier`
//!   (method `x.foo()`), or `scoped_identifier`/`name:` (`Type::foo()`). All
//!   three forms are captured so member and associated calls surface their name.
//! - there is no try/catch construct, so `try_block` stays `None`.
use super::{ErrorHandlingStrategy, FamilyPack};

pub static PACK: FamilyPack = FamilyPack {
    // ---- structural queries ----
    if_condition: Some("(if_expression condition: (_) @cond)"),
    if_consequence: Some("(if_expression consequence: (block) @cons)"),
    callee: Some(concat!(
        "(call_expression function: (identifier) @callee)",
        "(call_expression function: (field_expression field: (field_identifier) @callee))",
        "(call_expression function: (scoped_identifier name: (identifier) @callee))",
    )),
    // Rust has no try/catch — error handling is the UnwrapTransition below.
    try_block: None,

    // ---- literal tables ----
    // Only `false` is a constant-falsy `if` condition in Rust (there is no
    // truthy-integer / null coercion; `0`/`None` are not booleans).
    falsy_literals: &["false"],
    // Early-exit forms that mark an `if` consequence as a guard clause. `panic!`
    // (and `unreachable!`/`todo!`) diverge, so an `if c { panic!() }` is a guard.
    guard_exit_keywords: &["return", "break", "continue", "panic!", "unreachable!", "todo!"],

    // ---- error-handling strategy (UnwrapTransition) ----
    error_handling_strategy: ErrorHandlingStrategy::UnwrapTransition,
    // An unhandled unwrap appearing in the after.
    unwrap_markers: &[".unwrap(", ".expect("],
    // Prior fallible-result handling in the before: the `?` operator or a
    // `match`/`if let` on the result. `?` is the dominant form; `match `/`if let`
    // cover the explicit ones.
    handled_markers: &["?", "match ", "if let "],

    // ---- High-severity marker regexes (regex SOURCES, compiled+cached) ----
    // ChildProcess: std::process::Command, Command::new(...), and the
    // `.spawn/.output/.status` finishers — but the bare finishers fire ONLY when
    // `Command` is in scope (in the same node), because `.output()`/`.status()`/
    // `.spawn()` are common method names on unrelated receivers. `Command::new(`
    // alone fires; a finisher fires only with a `Command` token before it; and an
    // import (`use std::process::Command`) fires via subprocess_import. So a bare
    // `reader.output()?` on an arbitrary value never trips the rule.
    subprocess_import: Some(r"std::process::Command"),
    subprocess_call: Some(r"Command::new\s*\(|Command[\s\S]*?\.(spawn|output|status)\s*\("),

    // TlsRejectFalse: reqwest/native-tls danger toggles set to true.
    tls_disable: Some(
        r"\.danger_accept_invalid_certs\s*\(\s*true\s*\)|\.danger_accept_invalid_hostnames\s*\(\s*true\s*\)",
    ),
    // No process-wide TLS-disabling env var convention in Rust.
    env_tls_disable: None,
    // No runtime eval in Rust.
    eval_call: None,

    // LooseRegex: `Regex::new("…")` — group 1 is the pattern body. Rust raw
    // strings (`r"…"`) keep the same body; the leading `r` is allowed but not
    // captured.
    regex_compile: Some(r#"Regex::new\s*\(\s*r?"([^"]*)""#),

    // BroadenedCors: tower-http / actix permissive constructors and Any origin.
    cors_permissive: Some(
        r"CorsLayer::(very_permissive|permissive)\s*\(\)|Cors::permissive\s*\(\)|\.allow_origin\s*\(\s*Any\s*\)",
    ),

    // Cookie builders (cookie crate / actix). Differential: matched in before,
    // not in after => removed/weakened.
    cookie_httponly: Some(r"\.http_only\s*\(\s*true\s*\)"),
    cookie_secure: Some(r"\.secure\s*\(\s*true\s*\)"),
    // SameSite strong (Strict/Lax) in before, weakened to None in after.
    cookie_samesite: Some(r"\.same_site\s*\(\s*SameSite::(Strict|Lax)\s*\)"),
    cookie_samesite_weak: Some(r"\.same_site\s*\(\s*SameSite::None\s*\)"),
};

#[cfg(test)]
mod tests {
    use crate::model::{AstNode, NodeState};
    use crate::parse::Lang;
    use crate::rules::{registry, RuleCtx};
    use std::collections::HashSet;

    /// Build a Modified Rust node from before/after line slices.
    fn modified(kind: &str, before: &[&str], after: &[&str]) -> AstNode {
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

    fn ctx() -> RuleCtx {
        RuleCtx {
            deps: HashSet::new(),
            is_test_file: false,
            is_build_script: false,
            lang: Lang::Rust,
        }
    }

    fn ctx_test_file() -> RuleCtx {
        RuleCtx {
            deps: HashSet::new(),
            is_test_file: true,
            is_build_script: false,
            lang: Lang::Rust,
        }
    }

    /// The fired rule id, if any.
    fn fired(node: &AstNode, ctx: &RuleCtx) -> Option<&'static str> {
        registry().check(node, ctx).map(|(id, _)| id)
    }

    // ---------------- removed-if-guard ----------------

    #[test]
    fn removed_if_guard_fires_on_constant_false() {
        let node = modified(
            "FunctionDeclaration",
            &["if is_admin(user) {", "    audit();", "}"],
            &["if false {", "    audit();", "}"],
        );
        assert_eq!(fired(&node, &ctx()), Some("removed-if-guard"));
    }

    #[test]
    fn removed_if_guard_ignores_tightened_condition() {
        // Idiomatic refactor: condition stays live (gains a clause). No flag.
        let node = modified(
            "FunctionDeclaration",
            &["if ok {", "    run();", "}"],
            &["if ok && ready {", "    run();", "}"],
        );
        assert!(fired(&node, &ctx()).is_none());
    }

    #[test]
    fn removed_if_guard_ignores_if_let() {
        // `if let Some(x) = opt` is a binding form, not a constant-falsy guard.
        let node = modified(
            "FunctionDeclaration",
            &["if let Some(u) = user {", "    audit();", "}"],
            &["if let Some(u) = current() {", "    audit();", "}"],
        );
        assert!(fired(&node, &ctx()).is_none());
    }

    // ---------------- guard-removed ----------------

    #[test]
    fn guard_removed_fires_when_call_escapes_its_guard() {
        let node = modified(
            "FunctionDeclaration",
            &["if is_verified(order) {", "    charge_card(order);", "}"],
            &["charge_card(order);"],
        );
        assert_eq!(fired(&node, &ctx()), Some("guard-removed"));
    }

    #[test]
    fn guard_removed_suppressed_by_early_return_refactor() {
        // Hoisting the wrapping `if` into an inverted early-return guard clause
        // is a refactor, not a removed guard.
        let node = modified(
            "FunctionDeclaration",
            &["if is_verified(order) {", "    charge_card(order);", "}"],
            &[
                "if !is_verified(order) {",
                "    return;",
                "}",
                "charge_card(order);",
            ],
        );
        assert!(fired(&node, &ctx()).is_none());
    }

    #[test]
    fn guard_removed_ignores_match_guard_refactor() {
        // A `match` arm guard is not an `if` guard; renaming inside it must not
        // read as a removed guard (no guarded call escapes).
        let node = modified(
            "FunctionDeclaration",
            &["match state {", "    S::Ok if ready() => go(),", "    _ => {}", "}"],
            &["match status {", "    S::Ok if ready() => go(),", "    _ => {}", "}"],
        );
        assert!(fired(&node, &ctx()).is_none());
    }

    // ---------------- removed-sanitize ----------------

    #[test]
    fn removed_sanitize_fires_when_wrapper_stripped() {
        let node = modified(
            "FunctionDeclaration",
            &["store(sanitize_html(input));"],
            &["store(input);"],
        );
        assert_eq!(fired(&node, &ctx()), Some("removed-sanitize"));
    }

    #[test]
    fn removed_sanitize_ignores_preserved_call() {
        // Sanitizer still present after a rename refactor — no removal.
        let node = modified(
            "FunctionDeclaration",
            &["store(sanitize_html(input));"],
            &["persist(sanitize_html(input));"],
        );
        assert!(fired(&node, &ctx()).is_none());
    }

    // ---------------- verify-to-decode ----------------

    #[test]
    fn verify_to_decode_fires_on_downgrade() {
        let node = modified(
            "FunctionDeclaration",
            &["let claims = verify_token(token, key)?;"],
            &["let claims = decode_token(token)?;"],
        );
        assert_eq!(fired(&node, &ctx()), Some("verify-to-decode"));
    }

    #[test]
    fn verify_to_decode_ignores_preserved_verify() {
        let node = modified(
            "FunctionDeclaration",
            &["let claims = verify_token(token, key)?;"],
            &["let claims = verify_token(token, secret)?;"],
        );
        assert!(fired(&node, &ctx()).is_none());
    }

    // ---------------- error-handling-removed (UnwrapTransition) ----------------

    #[test]
    fn error_handling_removed_fires_on_question_to_unwrap() {
        let node = modified(
            "FunctionDeclaration",
            &["let cfg = load_config(path)?;"],
            &["let cfg = load_config(path).unwrap();"],
        );
        assert_eq!(fired(&node, &ctx()), Some("removed-try-catch"));
    }

    #[test]
    fn error_handling_removed_fires_on_match_to_expect() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "let cfg = match load_config(path) {",
                "    Ok(c) => c,",
                "    Err(e) => return Err(e),",
                "};",
            ],
            &["let cfg = load_config(path).expect(\"config\");"],
        );
        assert_eq!(fired(&node, &ctx()), Some("removed-try-catch"));
    }

    #[test]
    fn error_handling_removed_ignores_unwrap_present_in_both() {
        // No transition: the unwrap existed before and after. Not a removal.
        let node = modified(
            "FunctionDeclaration",
            &["let cfg = load_config(path).unwrap();"],
            &["let cfg = load_config(other).unwrap();"],
        );
        assert!(fired(&node, &ctx()).is_none());
    }

    #[test]
    fn error_handling_removed_ignores_unwrap_in_test_file() {
        // `.unwrap()` in tests is idiomatic. A test-file node with no prior
        // handling marker must not flag. (And even a `?`->unwrap transition in a
        // genuine test fixture is expected; we assert the no-transition case here
        // since the rule itself doesn't gate on is_test_file.)
        let node = modified(
            "FunctionDeclaration",
            &["let cfg = load_config(path).unwrap();"],
            &["let cfg = load_config(other).unwrap();"],
        );
        assert!(fired(&node, &ctx_test_file()).is_none());
    }

    #[test]
    fn error_handling_removed_ignores_question_preserved() {
        // `?` survives — error handling was not removed.
        let node = modified(
            "FunctionDeclaration",
            &["let cfg = load_config(path)?;"],
            &["let cfg = load_config(other)?;"],
        );
        assert!(fired(&node, &ctx()).is_none());
    }

    // ---------------- child-process ----------------

    #[test]
    fn child_process_fires_on_command_new() {
        let node = modified(
            "FunctionDeclaration",
            &["let out = read_file(path)?;"],
            &[
                "use std::process::Command;",
                "let out = Command::new(\"sh\").arg(\"-c\").arg(cmd).output()?;",
            ],
        );
        assert_eq!(fired(&node, &ctx()), Some("child-process"));
    }

    #[test]
    fn child_process_ignores_preexisting_command() {
        // Command was already used before and after — no newly introduced
        // subprocess.
        let node = modified(
            "FunctionDeclaration",
            &["let out = Command::new(\"ls\").output()?;"],
            &["let out = Command::new(\"ls\").arg(\"-a\").output()?;"],
        );
        assert!(fired(&node, &ctx()).is_none());
    }

    #[test]
    fn child_process_suppressed_in_build_script() {
        // FIX 4: build.rs legitimately shells out to git/protoc/bindgen, so
        // child-process is suppressed there (same as a test path).
        let node = modified(
            "FunctionDeclaration",
            &["let out = read_file(path)?;"],
            &[
                "use std::process::Command;",
                "let out = Command::new(\"protoc\").arg(proto).output()?;",
            ],
        );
        let build_ctx = RuleCtx {
            deps: HashSet::new(),
            is_test_file: false,
            is_build_script: true,
            lang: Lang::Rust,
        };
        assert!(
            fired(&node, &build_ctx).is_none(),
            "build.rs subprocess is build tooling, not drift"
        );
        // Sanity: the same node DOES flag in an ordinary source file.
        assert_eq!(fired(&node, &ctx()), Some("child-process"));
    }

    #[test]
    fn child_process_ignores_bare_finisher_without_command() {
        // FIX 2: a bare `.output()?` / `.status()?` / `.spawn()?` on an arbitrary
        // receiver (here a query builder) is NOT a subprocess — the finisher
        // markers fire only with `Command` in scope.
        for after in [
            "let rows = query.bind(id).output()?;",
            "let st = pipeline.status()?;",
            "let h = worker.spawn()?;",
        ] {
            let node = modified(
                "FunctionDeclaration",
                &["let rows = run(id);"],
                &[after],
            );
            assert!(
                fired(&node, &ctx()).is_none(),
                "bare finisher without Command must not flag: {after}"
            );
        }
        // Sanity: a Command-rooted builder still flags through the finisher.
        let node = modified(
            "FunctionDeclaration",
            &["let rows = run(id);"],
            &["let out = Command::new(\"ls\").spawn()?;"],
        );
        assert_eq!(fired(&node, &ctx()), Some("child-process"));
    }

    #[test]
    fn child_process_ignores_marker_in_comment_or_string() {
        // FIX 1: a subprocess token named only in a comment or string literal
        // must not fire (often code that warns against it).
        let comment = modified(
            "FunctionDeclaration",
            &["let x = compute();"],
            &["// never reach for Command::new(\"sh\") here", "let x = compute();"],
        );
        assert!(fired(&comment, &ctx()).is_none(), "marker in comment");
        let string = modified(
            "FunctionDeclaration",
            &["let x = compute();"],
            &["let note = \"avoid Command::new(\\\"sh\\\").output()\";"],
        );
        assert!(fired(&string, &ctx()).is_none(), "marker in string");
    }

    // ---------------- tls-disable ----------------

    #[test]
    fn tls_disable_ignores_marker_in_comment_or_string() {
        // FIX 1: the TLS-disable idiom named only in a comment/string must not
        // fire — these often document the forbidden call.
        let comment = modified(
            "FunctionDeclaration",
            &["let c = Client::builder().build()?;"],
            &[
                "// do NOT call .danger_accept_invalid_certs(true) in prod",
                "let c = Client::builder().build()?;",
            ],
        );
        assert!(fired(&comment, &ctx()).is_none(), "marker in comment");
        let string = modified(
            "FunctionDeclaration",
            &["let c = Client::builder().build()?;"],
            &["let msg = \"danger_accept_invalid_certs(true) is banned\";"],
        );
        assert!(fired(&string, &ctx()).is_none(), "marker in string");
    }

    #[test]
    fn cookie_secure_ignores_non_cookie_builder() {
        // FIX 2: a `.secure(true)` removed from a TLS config (not a cookie) must
        // not read as a weakened cookie flag — the cookie rules require cookie
        // context in the node.
        let node = modified(
            "FunctionDeclaration",
            &[
                "let cfg = TlsConfig::builder()",
                "    .secure(true)",
                "    .build();",
            ],
            &["let cfg = TlsConfig::builder()", "    .build();"],
        );
        assert!(
            fired(&node, &ctx()).is_none(),
            "secure(true) on a TlsConfig is not a cookie flag"
        );
        // Sanity: the same removal on an actual Cookie builder still flags.
        let cookie = modified(
            "FunctionDeclaration",
            &[
                "let c = Cookie::build(\"sid\", v)",
                "    .secure(true)",
                "    .finish();",
            ],
            &["let c = Cookie::build(\"sid\", v)", "    .finish();"],
        );
        assert_eq!(fired(&cookie, &ctx()), Some("cookie-secure-removed"));
    }

    #[test]
    fn cors_ignores_allow_origin_any_without_cors_context() {
        // FIX 2: `.allow_origin(Any)` on a non-CORS builder (a custom
        // RouteConfig) must not read as a broadened CORS — the Rust marker
        // requires CORS context.
        let node = modified(
            "FunctionDeclaration",
            &["let r = RouteConfig::new().allow_origin(trusted);"],
            &["let r = RouteConfig::new().allow_origin(Any);"],
        );
        assert!(
            fired(&node, &ctx()).is_none(),
            "allow_origin(Any) without CORS context is not a CORS broadening"
        );
        // Sanity: the same call on a CorsLayer still flags.
        let cors = modified(
            "FunctionDeclaration",
            &["let layer = CorsLayer::new().allow_origin(trusted);"],
            &["let layer = CorsLayer::new().allow_origin(Any);"],
        );
        assert_eq!(fired(&cors, &ctx()), Some("broadened-cors"));
    }

    #[test]
    fn tls_disable_fires_on_accept_invalid_certs() {
        let node = modified(
            "FunctionDeclaration",
            &["let client = Client::builder().build()?;"],
            &[
                "let client = Client::builder()",
                "    .danger_accept_invalid_certs(true)",
                "    .build()?;",
            ],
        );
        assert_eq!(fired(&node, &ctx()), Some("tls-reject-false"));
    }

    #[test]
    fn tls_disable_ignores_unchanged_setting() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "let client = Client::builder()",
                "    .danger_accept_invalid_certs(true)",
                "    .build()?;",
            ],
            &[
                "let client = Client::builder()",
                "    .danger_accept_invalid_certs(true)",
                "    .timeout(d)",
                "    .build()?;",
            ],
        );
        assert!(fired(&node, &ctx()).is_none());
    }

    // ---------------- loose-regex ----------------

    #[test]
    fn loose_regex_fires_when_widened_to_catch_all() {
        let node = modified(
            "VariableDeclaration",
            &[r#"let email = Regex::new(r"^[^@]+@[^@]+$").unwrap();"#],
            &[r#"let email = Regex::new(r".*").unwrap();"#],
        );
        assert_eq!(fired(&node, &ctx()), Some("loose-regex"));
    }

    #[test]
    fn loose_regex_ignores_unchanged_pattern() {
        let node = modified(
            "VariableDeclaration",
            &[r#"let email = Regex::new(r"^[^@]+@[^@]+$").unwrap();"#],
            &[r#"let addr = Regex::new(r"^[^@]+@[^@]+$").unwrap();"#],
        );
        assert!(fired(&node, &ctx()).is_none());
    }

    // ---------------- cookies ----------------

    #[test]
    fn cookie_httponly_removed_fires() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "let c = Cookie::build(\"sid\", v)",
                "    .http_only(true)",
                "    .finish();",
            ],
            &["let c = Cookie::build(\"sid\", v)", "    .finish();"],
        );
        assert_eq!(fired(&node, &ctx()), Some("cookie-httponly-removed"));
    }

    #[test]
    fn cookie_secure_removed_fires() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "let c = Cookie::build(\"sid\", v)",
                "    .secure(true)",
                "    .finish();",
            ],
            &["let c = Cookie::build(\"sid\", v)", "    .finish();"],
        );
        assert_eq!(fired(&node, &ctx()), Some("cookie-secure-removed"));
    }

    #[test]
    fn samesite_weakened_fires() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "let c = Cookie::build(\"sid\", v)",
                "    .same_site(SameSite::Strict)",
                "    .finish();",
            ],
            &[
                "let c = Cookie::build(\"sid\", v)",
                "    .same_site(SameSite::None)",
                "    .finish();",
            ],
        );
        assert_eq!(fired(&node, &ctx()), Some("samesite-weakened"));
    }

    #[test]
    fn cookie_flags_ignore_preserved_flags() {
        // All flags retained through a reformat — nothing weakened.
        let node = modified(
            "FunctionDeclaration",
            &[
                "let c = Cookie::build(\"sid\", v).http_only(true).secure(true).finish();",
            ],
            &[
                "let c = Cookie::build(\"sid\", v)",
                "    .http_only(true)",
                "    .secure(true)",
                "    .finish();",
            ],
        );
        assert!(fired(&node, &ctx()).is_none());
    }

    // ---------------- broadened cors is exercised via the eval case; here a
    // negative to confirm a preserved allowlist does not flag ----------------

    #[test]
    fn broadened_cors_fires_on_permissive() {
        let node = modified(
            "FunctionDeclaration",
            &["let cors = CorsLayer::new().allow_origin(allowed);"],
            &["let cors = CorsLayer::permissive();"],
        );
        assert_eq!(fired(&node, &ctx()), Some("broadened-cors"));
    }

    #[test]
    fn broadened_cors_ignores_preserved_allowlist() {
        let node = modified(
            "FunctionDeclaration",
            &["let cors = CorsLayer::new().allow_origin(allowed);"],
            &["let cors = CorsLayer::new().allow_origin(trusted);"],
        );
        assert!(fired(&node, &ctx()).is_none());
    }

    // ---------------- full benign refactor: no rule fires ----------------

    #[test]
    fn idiomatic_refactor_raises_nothing() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "pub fn handle(req: &Request) -> Response {",
                "    let body = read_body(req)?;",
                "    if body.is_empty() {",
                "        return Response::empty();",
                "    }",
                "    process(body)",
                "}",
            ],
            &[
                "pub fn handle(request: &Request) -> Response {",
                "    let payload = read_body(request)?;",
                "    if payload.is_empty() {",
                "        return Response::empty();",
                "    }",
                "    process(payload)",
                "}",
            ],
        );
        assert!(fired(&node, &ctx()).is_none());
    }
}
