//! Kotlin language pack. Fills the structural queries + marker tables the
//! security rules consult so they run against Kotlin source without hard-coding
//! JS node kinds or JS text regexes. Every field left at the `EMPTY` default
//! keeps the corresponding rule silent for Kotlin.
//!
//! `error_handling_strategy` is `TryBlock`: Kotlin has `try`/`catch` (as a
//! `try_expression`), so `ErrorHandlingRemoved` uses `try_block` to detect a
//! wrapping `try` that disappeared while its calls survived.
//!
//! ## Grammar notes (tree-sitter-kotlin-ng 1.1.0)
//!
//! The "ng" grammar gives `call_expression` no fields: a bare call `foo(x)` is
//! `(call_expression (identifier) value_arguments)` and a member call
//! `a.b.exec(x)` is `(call_expression (navigation_expression … (identifier))
//! value_arguments)`, where the navigation_expression's *last* `identifier`
//! child is the selector. The callee query anchors the bare form to the first
//! child (`.`) and pulls the selector from the immediate navigation_expression,
//! so `a.b.exec(x)` captures `exec` (not `a`/`b`).
//!
//! `if_expression` exposes a `condition` field but no consequence field: the
//! then-branch is an unfielded `block` child *only when it is a braced body*.
//! An if-*expression* (`val x = if (c) a() else b()`) has bare-expression
//! branches and therefore NO `block` child, so `if_consequence` captures
//! nothing for it — the guard rules stay quiet on that idiom by construction.
//! A braced `if (c) { x } else { y }` captures BOTH blocks; over-capturing the
//! else block only adds to the "still-guarded" side, the suppression direction.
//! `when` expressions are neither `if_expression` nor a `block` consequence, so
//! when-refactors never read as a removed guard.
use super::{ErrorHandlingStrategy, FamilyPack};

pub static PACK: FamilyPack = FamilyPack {
    // ---- structural queries ----
    if_condition: Some("(if_expression condition: (_) @cond)"),
    if_consequence: Some("(if_expression (block) @cons)"),
    callee: Some(concat!(
        "(call_expression . (identifier) @callee)",
        "(call_expression (navigation_expression (_) (identifier) @callee))"
    )),
    try_block: Some("(try_expression) @try"),

    // ---- literal tables ----
    falsy_literals: &["false"],
    guard_exit_keywords: &["return", "throw", "break", "continue"],

    // ---- error handling ----
    error_handling_strategy: ErrorHandlingStrategy::TryBlock,

    // ---- high-severity markers (regex sources) ----
    // Dynamic code execution: the JVM scripting bridge. `getEngineByName(` is
    // the call that hands a script engine the string it later `eval`s. Matching
    // the call (not a bare `import javax.script.ScriptEngineManager`) keeps the
    // import line from double-firing.
    eval_call: Some(r"\bgetEngineByName\s*\("),
    // Subprocess: `ProcessBuilder(...)` or `Runtime.getRuntime().exec(...)`.
    subprocess_import: Some(r"import\s+java\.lang\.ProcessBuilder|import\s+java\.lang\.Runtime"),
    subprocess_call: Some(r"\bProcessBuilder\s*\(|Runtime\.getRuntime\(\)\.exec\s*\(|\.getRuntime\(\)\.exec\s*\("),
    // TLS off: an all-trusting X509TrustManager, or a hostname verifier that
    // always returns true. Both are the canonical "ignore the cert" idioms.
    tls_disable: Some(
        r"object\s*:\s*X509TrustManager|HostnameVerifier\s*\{[^}]*->\s*true|hostnameVerifier\s*=?\s*\{[^}]*->\s*true",
    ),
    // Regex construction: `Regex("…")` and `Pattern.compile("…")` carry the
    // pattern body in group 1. (`"…".toRegex()` puts the literal before the
    // call; the differential path needs the body in group 1, which the receiver
    // form can't supply, so it is intentionally not matched.)
    regex_compile: Some(r#"(?:\bRegex|Pattern\.compile)\s*\(\s*"([^"]*)""#),
    // CORS opened to any origin: Spring `@CrossOrigin(origins = "*")` /
    // `allowedOrigins("*")`, and ktor `anyHost()`.
    cors_permissive: Some(
        r#"origins\s*=\s*\[?\s*"\*"|allowedOrigins\s*\(\s*"\*"|\banyHost\s*\(\s*\)"#,
    ),
    // Cookie flags. Java servlet `cookie.setHttpOnly(true)` / `setSecure(true)`
    // and ktor cookie config `httpOnly = true` / `secure = true`. The rule
    // fires when the flag matched the before and not the after (a removal).
    cookie_httponly: Some(r"setHttpOnly\s*\(\s*true\s*\)|httpOnly\s*=\s*true"),
    cookie_secure: Some(r"setSecure\s*\(\s*true\s*\)|\bsecure\s*=\s*true"),
    // SameSite: a strong value before, a weak (`None`) value after.
    cookie_samesite: Some(r#"[Ss]ameSite\s*[=(]\s*"?(Strict|Lax|strict|lax)"#),
    cookie_samesite_weak: Some(r#"[Ss]ameSite\s*[=(]\s*"?(None|none)"#),

    ..FamilyPack::EMPTY
};

#[cfg(test)]
mod tests {
    use crate::model::{AstNode, NodeState};
    use crate::parse::Lang;
    use crate::rules::{registry, RuleCtx};
    use std::collections::HashSet;

    /// Build a Modified node from before/after line slices.
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

    fn added(kind: &str, after: &[&str]) -> AstNode {
        AstNode {
            id: "t".into(),
            kind: kind.into(),
            name: "n".into(),
            signature: None,
            state: NodeState::Added,
            reviewed: false,
            flag_id: None,
            before: None,
            after: Some(after.iter().map(|s| s.to_string()).collect()),
            children: None,
        }
    }

    fn ctx(is_test_file: bool) -> RuleCtx {
        RuleCtx {
            deps: HashSet::new(),
            is_test_file,
            lang: Lang::Kotlin,
        }
    }

    fn check_id(node: &AstNode, is_test_file: bool) -> Option<&'static str> {
        registry().check(node, &ctx(is_test_file)).map(|(id, _)| id)
    }

    // ---------------- removed-if-guard ----------------

    #[test]
    fn removed_if_guard_fires_on_constant_false() {
        let node = modified(
            "FunctionDeclaration",
            &["fun f() {", "    if (isAdmin(user)) {", "        audit()", "    }", "}"],
            &["fun f() {", "    if (false) {", "        audit()", "    }", "}"],
        );
        assert_eq!(check_id(&node, false), Some("removed-if-guard"));
    }

    #[test]
    fn removed_if_guard_quiet_on_live_condition() {
        let node = modified(
            "FunctionDeclaration",
            &["fun f() {", "    if (ok) {", "        run()", "    }", "}"],
            &["fun f() {", "    if (ok && ready) {", "        run()", "    }", "}"],
        );
        assert_eq!(check_id(&node, false), None);
    }

    #[test]
    fn removed_if_guard_quiet_on_if_expression() {
        // if-as-expression has no `block` consequence; the condition `c` is not
        // a falsy literal, so nothing flags.
        let node = modified(
            "VariableDeclaration",
            &["val x = if (c) a() else b()"],
            &["val x = if (c) a() else fallback()"],
        );
        assert_eq!(check_id(&node, false), None);
    }

    // ---------------- guard-removed ----------------

    #[test]
    fn guard_removed_fires_when_call_escapes_guard() {
        let node = modified(
            "FunctionDeclaration",
            &["fun pay(o: Order) {", "    if (isVerified(o)) {", "        chargeCard(o)", "    }", "}"],
            &["fun pay(o: Order) {", "    chargeCard(o)", "}"],
        );
        assert_eq!(check_id(&node, false), Some("guard-removed"));
    }

    #[test]
    fn guard_removed_quiet_on_elvis_refactor() {
        // Replacing an if-null-check with an elvis `?:` is an idiomatic refactor:
        // the protected call is still gated, just expressed differently. The
        // before's only guarded call still appears guarded — and the after has no
        // if-block at all — so the rule must stay quiet.
        let node = modified(
            "FunctionDeclaration",
            &[
                "fun load(id: String): User {",
                "    if (cache[id] != null) {",
                "        return cache[id]",
                "    }",
                "    return fetch(id)",
                "}",
            ],
            &[
                "fun load(id: String): User {",
                "    return cache[id] ?: fetch(id)",
                "}",
            ],
        );
        assert_eq!(check_id(&node, false), None);
    }

    #[test]
    fn guard_removed_quiet_on_when_refactor() {
        // An if-as-expression selecting a value, refactored to a `when`. The
        // branches were bare expressions (no `if` *block* consequence), so the
        // before has no guarded call; converting it to `when` flags nothing.
        // (A single side-effect call lifted out of an `if` *block* into a `when`
        // branch is a genuine guard relocation and is treated the same way the
        // JS path treats `if`→`switch` — that is not what a value-`when` is.)
        let node = modified(
            "FunctionDeclaration",
            &[
                "fun label(r: Req): String {",
                "    return if (r.kind == 1) classify(r) else fallback(r)",
                "}",
            ],
            &[
                "fun label(r: Req): String {",
                "    return when (r.kind) {",
                "        1 -> classify(r)",
                "        else -> fallback(r)",
                "    }",
                "}",
            ],
        );
        assert_eq!(check_id(&node, false), None);
    }

    #[test]
    fn guard_removed_quiet_on_early_return_refactor() {
        // Hoisting a wrapping if into an inverted early-return guard clause keeps
        // the protection — must not read as removed.
        let node = modified(
            "FunctionDeclaration",
            &[
                "fun pay(o: Order) {",
                "    if (isVerified(o)) {",
                "        chargeCard(o)",
                "    }",
                "}",
            ],
            &[
                "fun pay(o: Order) {",
                "    if (!isVerified(o)) {",
                "        return",
                "    }",
                "    chargeCard(o)",
                "}",
            ],
        );
        assert_eq!(check_id(&node, false), None);
    }

    // ---------------- removed-sanitize ----------------

    #[test]
    fn removed_sanitize_fires_when_call_dropped() {
        let node = modified(
            "FunctionDeclaration",
            &["fun save(input: String) {", "    store(sanitizeHtml(input))", "}"],
            &["fun save(input: String) {", "    store(input)", "}"],
        );
        assert_eq!(check_id(&node, false), Some("removed-sanitize"));
    }

    #[test]
    fn removed_sanitize_fires_on_member_sanitizer() {
        let node = modified(
            "FunctionDeclaration",
            &["fun save(input: String) {", "    store(validator.validate(input))", "}"],
            &["fun save(input: String) {", "    store(input)", "}"],
        );
        assert_eq!(check_id(&node, false), Some("removed-sanitize"));
    }

    #[test]
    fn removed_sanitize_quiet_when_sanitizer_kept() {
        let node = modified(
            "FunctionDeclaration",
            &["fun save(input: String) {", "    store(sanitize(input))", "}"],
            &["fun save(input: String) {", "    persist(sanitize(input))", "}"],
        );
        assert_eq!(check_id(&node, false), None);
    }

    // ---------------- verify-to-decode ----------------

    #[test]
    fn verify_to_decode_fires() {
        let node = modified(
            "FunctionDeclaration",
            &["fun auth(token: String): Claims {", "    return jwt.verify(token)", "}"],
            &["fun auth(token: String): Claims {", "    return jwt.decode(token)", "}"],
        );
        assert_eq!(check_id(&node, false), Some("verify-to-decode"));
    }

    #[test]
    fn verify_to_decode_quiet_when_verify_kept() {
        let node = modified(
            "FunctionDeclaration",
            &["fun auth(token: String): Claims {", "    return jwt.verify(token)", "}"],
            &["fun auth(token: String): Claims {", "    return jwt.verify(token, key)", "}"],
        );
        assert_eq!(check_id(&node, false), None);
    }

    // ---------------- error-handling-removed (try) ----------------

    #[test]
    fn error_handling_removed_fires_when_try_dropped() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "fun sync() {",
                "    try {",
                "        pushChanges()",
                "    } catch (e: Exception) {",
                "        log(e)",
                "    }",
                "}",
            ],
            &["fun sync() {", "    pushChanges()", "}"],
        );
        assert_eq!(check_id(&node, false), Some("removed-try-catch"));
    }

    #[test]
    fn error_handling_removed_quiet_when_try_kept() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "fun sync() {",
                "    try {",
                "        pushChanges()",
                "    } catch (e: Exception) {",
                "        log(e)",
                "    }",
                "}",
            ],
            &[
                "fun sync() {",
                "    try {",
                "        pushChanges()",
                "    } catch (e: IOException) {",
                "        retry()",
                "    }",
                "}",
            ],
        );
        assert_eq!(check_id(&node, false), None);
    }

    // ---------------- eval-call ----------------

    #[test]
    fn eval_call_fires_on_script_engine() {
        let node = modified(
            "FunctionDeclaration",
            &["fun run(code: String) {", "    println(code)", "}"],
            &[
                "fun run(code: String) {",
                "    val engine = ScriptEngineManager().getEngineByName(\"kotlin\")",
                "    engine.eval(code)",
                "}",
            ],
        );
        assert_eq!(check_id(&node, false), Some("eval-call"));
    }

    #[test]
    fn eval_call_quiet_on_unrelated_eval_word() {
        // A method literally named `evaluate` on a domain object is not the
        // scripting bridge; no ScriptEngineManager/getEngineByName marker.
        let node = modified(
            "FunctionDeclaration",
            &["fun run(rule: Rule) {", "    rule.check()", "}"],
            &["fun run(rule: Rule) {", "    rule.evaluate(ctx)", "}"],
        );
        assert_eq!(check_id(&node, false), None);
    }

    // ---------------- subprocess (child-process) ----------------

    #[test]
    fn subprocess_fires_on_process_builder() {
        let node = added(
            "FunctionDeclaration",
            &[
                "fun run(cmd: String) {",
                "    ProcessBuilder(\"sh\", \"-c\", cmd).start()",
                "}",
            ],
        );
        assert_eq!(check_id(&node, false), Some("child-process"));
    }

    #[test]
    fn subprocess_fires_on_runtime_exec() {
        let node = added(
            "FunctionDeclaration",
            &["fun run(cmd: String) {", "    Runtime.getRuntime().exec(cmd)", "}"],
        );
        assert_eq!(check_id(&node, false), Some("child-process"));
    }

    #[test]
    fn subprocess_quiet_in_test_file() {
        let node = added(
            "FunctionDeclaration",
            &["fun run(cmd: String) {", "    ProcessBuilder(cmd).start()", "}"],
        );
        assert_eq!(check_id(&node, true), None);
    }

    #[test]
    fn subprocess_quiet_on_unrelated_builder() {
        let node = added(
            "FunctionDeclaration",
            &["fun build(): Request {", "    return RequestBuilder(url).build()", "}"],
        );
        assert_eq!(check_id(&node, false), None);
    }

    // ---------------- tls-disable ----------------

    #[test]
    fn tls_disable_fires_on_trust_all_manager() {
        let node = added(
            "VariableDeclaration",
            &[
                "val trustAll = object : X509TrustManager {",
                "    override fun checkServerTrusted(c: Array<X509Certificate>, t: String) {}",
                "    override fun getAcceptedIssuers() = arrayOf<X509Certificate>()",
                "}",
            ],
        );
        assert_eq!(check_id(&node, false), Some("tls-reject-false"));
    }

    #[test]
    fn tls_disable_fires_on_permissive_hostname_verifier() {
        let node = added(
            "VariableDeclaration",
            &["val verifier = HostnameVerifier { _, _ -> true }"],
        );
        assert_eq!(check_id(&node, false), Some("tls-reject-false"));
    }

    #[test]
    fn tls_disable_quiet_when_unchanged() {
        // Differential: the trust-all manager exists in BOTH before and after, so
        // nothing was newly disabled — must not flag.
        let node = modified(
            "VariableDeclaration",
            &["val tm = object : X509TrustManager { }", "val a = 1"],
            &["val tm = object : X509TrustManager { }", "val a = 2"],
        );
        assert_eq!(check_id(&node, false), None);
    }

    // ---------------- loose-regex ----------------

    #[test]
    fn loose_regex_fires_on_widening_to_catch_all() {
        let node = modified(
            "VariableDeclaration",
            &["val emailRe = Regex(\"^[^@]+@[^@]+$\")"],
            &["val emailRe = Regex(\".*\")"],
        );
        assert_eq!(check_id(&node, false), Some("loose-regex"));
    }

    #[test]
    fn loose_regex_quiet_on_equivalent_pattern() {
        let node = modified(
            "VariableDeclaration",
            &["val zipRe = Regex(\"^[0-9]{5}$\")"],
            &["val zipRe = Regex(\"^[0-9]{5}$\")", "val other = 1"],
        );
        assert_eq!(check_id(&node, false), None);
    }

    // ---------------- cors-permissive ----------------

    #[test]
    fn cors_permissive_fires_on_any_host() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "fun Application.configure() {",
                "    install(CORS) {",
                "        allowHost(\"app.example.com\")",
                "    }",
                "}",
            ],
            &[
                "fun Application.configure() {",
                "    install(CORS) {",
                "        anyHost()",
                "    }",
                "}",
            ],
        );
        assert_eq!(check_id(&node, false), Some("broadened-cors"));
    }

    #[test]
    fn cors_permissive_fires_on_wildcard_origin() {
        let node = modified(
            "ClassDeclaration",
            &["@CrossOrigin(origins = [\"https://app.example.com\"])", "class Api"],
            &["@CrossOrigin(origins = [\"*\"])", "class Api"],
        );
        assert_eq!(check_id(&node, false), Some("broadened-cors"));
    }

    #[test]
    fn cors_permissive_quiet_when_unchanged() {
        let node = modified(
            "FunctionDeclaration",
            &["fun configure() {", "    anyHost()", "}"],
            &["fun configure() {", "    anyHost()", "    log()", "}"],
        );
        assert_eq!(check_id(&node, false), None);
    }

    // ---------------- cookie flags ----------------

    #[test]
    fn cookie_httponly_removed_fires() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "fun build(): Cookie {",
                "    val c = Cookie(\"sid\", token)",
                "    c.setHttpOnly(true)",
                "    return c",
                "}",
            ],
            &[
                "fun build(): Cookie {",
                "    val c = Cookie(\"sid\", token)",
                "    return c",
                "}",
            ],
        );
        assert_eq!(check_id(&node, false), Some("cookie-httponly-removed"));
    }

    #[test]
    fn cookie_secure_removed_fires() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "fun build(): Cookie {",
                "    val c = Cookie(\"sid\", token)",
                "    c.setSecure(true)",
                "    return c",
                "}",
            ],
            &[
                "fun build(): Cookie {",
                "    val c = Cookie(\"sid\", token)",
                "    return c",
                "}",
            ],
        );
        assert_eq!(check_id(&node, false), Some("cookie-secure-removed"));
    }

    #[test]
    fn cookie_samesite_weakened_fires() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "fun build(): Cookie {",
                "    cookie.sameSite = \"Strict\"",
                "    return cookie",
                "}",
            ],
            &[
                "fun build(): Cookie {",
                "    cookie.sameSite = \"None\"",
                "    return cookie",
                "}",
            ],
        );
        assert_eq!(check_id(&node, false), Some("samesite-weakened"));
    }

    #[test]
    fn cookie_flags_quiet_when_preserved() {
        // The flag is set in both versions (just reformatted) — no removal.
        let node = modified(
            "FunctionDeclaration",
            &["fun build() {", "    c.setHttpOnly(true)", "    c.setSecure(true)", "}"],
            &[
                "fun build() {",
                "    c.setHttpOnly(true)",
                "    c.setSecure(true)",
                "    log()",
                "}",
            ],
        );
        assert_eq!(check_id(&node, false), None);
    }

    // ---------------- broad idiomatic-negative sweep ----------------

    #[test]
    fn idiomatic_kotlin_does_not_flag() {
        // A grab-bag of normal Kotlin that must produce no finding.
        let samples: &[(&str, &[&str], &[&str])] = &[
            // when-expression as a value
            (
                "VariableDeclaration",
                &["val label = if (n > 0) \"pos\" else \"neg\""],
                &["val label = when { n > 0 -> \"pos\"; else -> \"neg\" }"],
            ),
            // null-safe call chain
            (
                "FunctionDeclaration",
                &["fun name(u: User?): String {", "    return u.name", "}"],
                &["fun name(u: User?): String {", "    return u?.name ?: \"anon\"", "}"],
            ),
            // adding a normal guarded call (no escape)
            (
                "FunctionDeclaration",
                &["fun pay(o: Order) {", "    if (ok(o)) {", "        charge(o)", "    }", "}"],
                &[
                    "fun pay(o: Order) {",
                    "    if (ok(o)) {",
                    "        charge(o)",
                    "        notify(o)",
                    "    }",
                    "}",
                ],
            ),
        ];
        for (kind, before, after) in samples {
            let node = modified(kind, before, after);
            assert_eq!(
                check_id(&node, false),
                None,
                "idiomatic Kotlin flagged: {after:?}"
            );
        }
    }
}
