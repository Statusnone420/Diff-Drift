//! C# language pack. Fills the structural queries and marker tables the security
//! rules consult for the C# family, with positive + idiomatic-negative unit
//! tests in this file's `#[cfg(test)]` module. An unfilled field keeps the
//! corresponding rule silent for C#.
//!
//! `error_handling_strategy` is `TryBlock`: C# has `try`/`catch`, so
//! `ErrorHandlingRemoved` uses `try_block` to detect a wrapping `try` that
//! disappeared while its calls survived.
//!
//! Grammar: tree-sitter-c-sharp 0.23.5. Grounded node kinds:
//! `if_statement` (fields `condition: expression`, `consequence: statement`),
//! `invocation_expression` (`function:` is `identifier` or
//! `member_access_expression`), `member_access_expression` (`name:` is
//! `identifier`/`generic_name`), `try_statement` (`body: block`).
use super::{ErrorHandlingStrategy, FamilyPack};

pub static PACK: FamilyPack = FamilyPack {
    // ---- structural queries ----
    // `if (cond) …` — condition is a bare `expression` (no parenthesized wrapper
    // node in the C# grammar; the parens are anonymous tokens), so a wildcard
    // capture yields the condition text trimmed of the surrounding `()`.
    if_condition: Some("(if_statement condition: (_) @cond)"),
    if_consequence: Some("(if_statement consequence: (_) @cons)"),
    // Every call's callee name: bare `Foo(…)` and member `obj.Foo(…)` /
    // `Type.Foo(…)` both capture `Foo`.
    callee: Some(
        "(invocation_expression function: (identifier) @callee) \
         (invocation_expression function: (member_access_expression name: (identifier) @callee))",
    ),
    try_block: Some("(try_statement) @try"),

    // ---- literal tables ----
    falsy_literals: &["false"],
    guard_exit_keywords: &["return", "throw", "break", "continue"],

    // ---- error-handling strategy ----
    error_handling_strategy: ErrorHandlingStrategy::TryBlock,

    // ---- high-severity marker / regex sources ----
    // Subprocess: `Process.Start(…)` or `new ProcessStartInfo(…)`.
    subprocess_call: Some(r"\b(Process\s*\.\s*Start|new\s+ProcessStartInfo)\b"),
    // Roslyn scripting: `CSharpScript.EvaluateAsync(…)` / `.RunAsync(…)`.
    eval_call: Some(r"\bCSharpScript\s*\.\s*(?:EvaluateAsync|RunAsync|Create|Run)\b"),
    // TLS verification turned off. Three concrete bypass idioms only — the
    // assignment alone is NOT enough, because a custom callback can be SECURE
    // (cert pinning that returns `false` on mismatch, or `=> SslPolicyErrors.None
    // == errors`). We fire when:
    //  - the `DangerousAcceptAnyServerCertificateValidator` escape hatch appears
    //    (it IS the accept-anything value, however it is assigned), OR
    //  - a server-cert callback is assigned a lambda/delegate that
    //    unconditionally returns `true` (`=> true`, `=> { … return true; }`,
    //    `delegate { … return true; }`).
    // A pinning callback (`=> errors == SslPolicyErrors.None`, `return false` on
    // mismatch) matches none of these, so it stays silent.
    tls_disable: Some(
        r#"(?:DangerousAcceptAnyServerCertificateValidator|(?:ServerCertificateCustomValidationCallback|ServerCertificateValidationCallback)\s*\+?=\s*(?:\([^)]*\)\s*=>\s*true\b|\([^)]*\)\s*=>\s*\{[^}]*return\s+true\b|delegate\s*(?:\([^)]*\))?\s*\{[^}]*return\s+true\b))"#,
    ),
    // Regex construction whose string argument is the pattern body (group 1):
    // `new Regex("pat")` (pattern is first arg) and `Regex.IsMatch(input,
    // "pat")` / `Regex.Match(input, "pat")` (pattern is second arg). The `@`
    // verbatim prefix is consumed but not captured.
    regex_compile: Some(
        r#"(?:new\s+Regex\s*\(\s*|Regex\s*\.\s*(?:IsMatch|Match)\s*\([^,]*,\s*)@?"([^"]*)""#,
    ),
    // Permissive CORS: `AllowAnyOrigin()` or `WithOrigins("*")`.
    cors_permissive: Some(r#"(AllowAnyOrigin\s*\(|WithOrigins\s*\([^)]*"\*")"#),
    // Cookie HttpOnly enabled: `HttpOnly = true`. Fires when matched before, not
    // after (the rule compares).
    cookie_httponly: Some(r"HttpOnly\s*=\s*true"),
    // Cookie Secure enabled: `Secure = true` or a hardened `SecurePolicy`.
    cookie_secure: Some(r"(Secure\s*=\s*true|CookieSecurePolicy\s*\.\s*Always)"),
    // SameSite strong (Strict/Lax) vs weak (None) — both required for the rule.
    cookie_samesite: Some(r"SameSite\s*=\s*SameSiteMode\s*\.\s*(Strict|Lax)"),
    cookie_samesite_weak: Some(r"SameSite\s*=\s*SameSiteMode\s*\.\s*None"),

    ..FamilyPack::EMPTY
};

#[cfg(test)]
mod tests {
    use crate::model::{AstNode, NodeState};
    use crate::parse::Lang;
    use crate::rules::{registry, RuleCtx};
    use std::collections::HashSet;

    /// Build a Modified node from before/after source line vectors.
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
            is_build_script: false,
            lang: Lang::CSharp,
        }
    }

    fn hit(node: &AstNode, is_test_file: bool) -> Option<&'static str> {
        registry().check(node, &ctx(is_test_file)).map(|(id, _)| id)
    }

    // ---------------------------------------------------------------
    // removed-if-guard
    // ---------------------------------------------------------------
    #[test]
    fn removed_if_guard_fires_on_constant_false() {
        let node = modified(
            "IfStatement",
            &["if (IsAdmin(user)) {", "    Audit();", "}"],
            &["if (false) {", "    Audit();", "}"],
        );
        assert_eq!(hit(&node, false), Some("removed-if-guard"));
    }

    #[test]
    fn removed_if_guard_negative_live_condition_tightened() {
        // Idiomatic refactor: a live condition gains a clause — must NOT flag.
        let node = modified(
            "IfStatement",
            &["if (ok) {", "    Run();", "}"],
            &["if (ok && ready) {", "    Run();", "}"],
        );
        assert_eq!(hit(&node, false), None);
    }

    #[test]
    fn removed_if_guard_negative_false_in_string() {
        // The word `false` inside a string literal is not a constant-falsy guard.
        let node = modified(
            "IfStatement",
            &["if (Check(user)) {", "    Log(\"ok\");", "}"],
            &["if (Check(user)) {", "    Log(\"if (false)\");", "}"],
        );
        assert_eq!(hit(&node, false), None);
    }

    // ---------------------------------------------------------------
    // guard-removed
    // ---------------------------------------------------------------
    #[test]
    fn guard_removed_fires_when_guarded_call_escapes() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "public void Pay(Order o) {",
                "    if (IsVerified(o)) {",
                "        ChargeCard(o);",
                "    }",
                "}",
            ],
            &["public void Pay(Order o) {", "    ChargeCard(o);", "}"],
        );
        assert_eq!(hit(&node, false), Some("guard-removed"));
    }

    #[test]
    fn guard_removed_negative_early_return_refactor() {
        // Hoisting a wrapping `if` into an inverted guard clause keeps the
        // protection — must NOT flag.
        let node = modified(
            "FunctionDeclaration",
            &[
                "public void Pay(Order o) {",
                "    if (IsVerified(o)) {",
                "        ChargeCard(o);",
                "    }",
                "}",
            ],
            &[
                "public void Pay(Order o) {",
                "    if (!IsVerified(o)) {",
                "        return;",
                "    }",
                "    ChargeCard(o);",
                "}",
            ],
        );
        assert_eq!(hit(&node, false), None);
    }

    // ---------------------------------------------------------------
    // removed-sanitize
    // ---------------------------------------------------------------
    #[test]
    fn removed_sanitize_fires_when_validate_call_dropped() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "public void Save(string input) {",
                "    var clean = sanitizer.escape(input);",
                "    Store(clean);",
                "}",
            ],
            &["public void Save(string input) {", "    Store(input);", "}"],
        );
        assert_eq!(hit(&node, false), Some("removed-sanitize"));
    }

    #[test]
    fn removed_sanitize_negative_rename_keeps_sanitizer() {
        // The sanitizer survives under a renamed local — still called, no flag.
        let node = modified(
            "FunctionDeclaration",
            &[
                "public void Save(string input) {",
                "    var clean = sanitizer.escape(input);",
                "    Store(clean);",
                "}",
            ],
            &[
                "public void Save(string input) {",
                "    var safe = sanitizer.escape(input);",
                "    Store(safe);",
                "}",
            ],
        );
        assert_eq!(hit(&node, false), None);
    }

    // ---------------------------------------------------------------
    // verify-to-decode
    // ---------------------------------------------------------------
    // The verify-to-decode and removed-sanitize rules key off lowercase callee
    // prefixes (`verify*`, `decode*`, `sanitize*`, `escape*`, `validate*`). C#
    // library entry points often follow that lowercase shape via fluent
    // builders / camelCase locals (e.g. `verify(token)`, `validate(input)`); a
    // member call resolves to its last segment, so `jwt.verify(…)` captures
    // `verify`. These tests use those forms.
    #[test]
    fn verify_to_decode_fires_on_downgrade() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "public object Read(string token) {",
                "    return jwt.verify(token, key);",
                "}",
            ],
            &[
                "public object Read(string token) {",
                "    return jwt.decode(token);",
                "}",
            ],
        );
        assert_eq!(hit(&node, false), Some("verify-to-decode"));
    }

    #[test]
    fn verify_to_decode_negative_verify_survives() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "public object Read(string token) {",
                "    return jwt.verify(token, key);",
                "}",
            ],
            &[
                "public object Read(string token) {",
                "    var v = jwt.verify(token, key);",
                "    return jwt.decode(v);",
                "}",
            ],
        );
        assert_eq!(hit(&node, false), None);
    }

    // ---------------------------------------------------------------
    // error-handling-removed (TryBlock)
    // ---------------------------------------------------------------
    #[test]
    fn error_handling_removed_fires_when_try_vanishes() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "public void Load() {",
                "    try {",
                "        Connect();",
                "    } catch (Exception e) {",
                "        Log(e);",
                "    }",
                "}",
            ],
            &["public void Load() {", "    Connect();", "}"],
        );
        assert_eq!(hit(&node, false), Some("removed-try-catch"));
    }

    #[test]
    fn error_handling_removed_negative_try_kept() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "public void Load() {",
                "    try {",
                "        Connect();",
                "    } catch (Exception e) {",
                "        Log(e);",
                "    }",
                "}",
            ],
            &[
                "public void Load() {",
                "    try {",
                "        Connect();",
                "        Warm();",
                "    } catch (Exception e) {",
                "        Log(e);",
                "    }",
                "}",
            ],
        );
        assert_eq!(hit(&node, false), None);
    }

    // ---------------------------------------------------------------
    // eval-call (Roslyn scripting)
    // ---------------------------------------------------------------
    #[test]
    fn eval_call_fires_on_csharpscript() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "public async Task Run(string code) {",
                "    await Task.CompletedTask;",
                "}",
            ],
            &[
                "public async Task Run(string code) {",
                "    await CSharpScript.EvaluateAsync(code);",
                "}",
            ],
        );
        assert_eq!(hit(&node, false), Some("eval-call"));
    }

    #[test]
    fn eval_call_negative_already_present() {
        // Differential: CSharpScript already ran before the change, so editing
        // around it is not a newly-introduced eval — must NOT flag.
        let node = modified(
            "FunctionDeclaration",
            &[
                "public async Task Run(string code) {",
                "    await CSharpScript.EvaluateAsync(code);",
                "}",
            ],
            &[
                "public async Task Run(string code) {",
                "    Log(code);",
                "    await CSharpScript.EvaluateAsync(code);",
                "}",
            ],
        );
        assert_eq!(hit(&node, false), None);
    }

    #[test]
    fn eval_call_negative_benign_async_refactor() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "public async Task Run() {",
                "    await DoWork();",
                "}",
            ],
            &[
                "public async Task Run() {",
                "    await DoWorkAsync();",
                "}",
            ],
        );
        assert_eq!(hit(&node, false), None);
    }

    // ---------------------------------------------------------------
    // child-process
    // ---------------------------------------------------------------
    #[test]
    fn child_process_fires_on_process_start() {
        let node = modified(
            "FunctionDeclaration",
            &["public void Open(string url) {", "    Log(url);", "}"],
            &[
                "public void Open(string url) {",
                "    Process.Start(\"explorer\", url);",
                "}",
            ],
        );
        assert_eq!(hit(&node, false), Some("child-process"));
    }

    #[test]
    fn child_process_suppressed_in_test_file() {
        let node = modified(
            "FunctionDeclaration",
            &["public void Open(string url) {", "    Log(url);", "}"],
            &[
                "public void Open(string url) {",
                "    Process.Start(\"explorer\", url);",
                "}",
            ],
        );
        assert_eq!(hit(&node, true), None);
    }

    // ---------------------------------------------------------------
    // tls-disable
    // ---------------------------------------------------------------
    #[test]
    fn tls_disable_fires_on_dangerous_validator() {
        let node = modified(
            "VariableDeclaration",
            &[
                "var handler = new HttpClientHandler {",
                "    CheckCertificateRevocationList = true,",
                "};",
            ],
            &[
                "var handler = new HttpClientHandler {",
                "    ServerCertificateCustomValidationCallback =",
                "        HttpClientHandler.DangerousAcceptAnyServerCertificateValidator,",
                "};",
            ],
        );
        assert_eq!(hit(&node, false), Some("tls-reject-false"));
    }

    #[test]
    fn tls_disable_negative_validation_kept() {
        let node = modified(
            "VariableDeclaration",
            &[
                "var handler = new HttpClientHandler {",
                "    CheckCertificateRevocationList = true,",
                "};",
            ],
            &[
                "var handler = new HttpClientHandler {",
                "    CheckCertificateRevocationList = true,",
                "    MaxConnectionsPerServer = 10,",
                "};",
            ],
        );
        assert_eq!(hit(&node, false), None);
    }

    #[test]
    fn tls_disable_fires_on_always_true_callback() {
        // FIX 2: assigning a callback that UNCONDITIONALLY returns true is a
        // bypass and must fire.
        let node = modified(
            "VariableDeclaration",
            &["var handler = new HttpClientHandler { };"],
            &[
                "var handler = new HttpClientHandler {",
                "    ServerCertificateCustomValidationCallback = (m, c, ch, e) => true,",
                "};",
            ],
        );
        assert_eq!(hit(&node, false), Some("tls-reject-false"));
    }

    #[test]
    fn tls_disable_ignores_secure_callback_returning_none_check() {
        // FIX 2: a callback that returns `SslPolicyErrors.None == errors` is the
        // SECURE default check — assigning it must NOT fire. (Previously any
        // assignment to the callback flagged.)
        let node = modified(
            "VariableDeclaration",
            &["var handler = new HttpClientHandler { };"],
            &[
                "var handler = new HttpClientHandler {",
                "    ServerCertificateCustomValidationCallback = (m, c, ch, e) => e == SslPolicyErrors.None,",
                "};",
            ],
        );
        assert_eq!(hit(&node, false), None);
    }

    #[test]
    fn tls_disable_ignores_cert_pinning_callback() {
        // FIX 2: a pinning callback that returns `false` on mismatch is secure —
        // assigning it must NOT fire.
        let node = modified(
            "FunctionDeclaration",
            &["public void Configure(HttpClientHandler h) { }"],
            &[
                "public void Configure(HttpClientHandler h) {",
                "    h.ServerCertificateCustomValidationCallback = (m, cert, ch, e) => {",
                "        if (cert.Thumbprint != Pinned) { return false; }",
                "        return e == SslPolicyErrors.None;",
                "    };",
                "}",
            ],
        );
        assert_eq!(hit(&node, false), None);
    }

    #[test]
    fn tls_disable_ignores_marker_in_comment_or_string() {
        // FIX 1: the dangerous validator named only in a comment/string must not
        // fire.
        let comment = modified(
            "VariableDeclaration",
            &["var handler = new HttpClientHandler { };"],
            &[
                "// do not assign DangerousAcceptAnyServerCertificateValidator",
                "var handler = new HttpClientHandler { };",
            ],
        );
        assert_eq!(hit(&comment, false), None, "marker in comment");
        let string = modified(
            "VariableDeclaration",
            &["var msg = \"ok\";"],
            &["var msg = \"never use DangerousAcceptAnyServerCertificateValidator\";"],
        );
        assert_eq!(hit(&string, false), None, "marker in string");
    }

    #[test]
    fn eval_call_suppressed_in_test_file() {
        // FIX 5: a Roslyn-scripting integration test of CSharpScript in a
        // *Test.cs file is scaffolding, not production drift.
        let node = modified(
            "FunctionDeclaration",
            &[
                "public async Task Run(string code) {",
                "    await Task.CompletedTask;",
                "}",
            ],
            &[
                "public async Task Run(string code) {",
                "    await CSharpScript.EvaluateAsync(code);",
                "}",
            ],
        );
        assert_eq!(hit(&node, true), None, "eval suppressed in test file");
        // Sanity: fires in a non-test file.
        assert_eq!(hit(&node, false), Some("eval-call"));
    }

    #[test]
    fn child_process_ignores_marker_in_comment_or_string() {
        // FIX 1: `Process.Start` named only in a comment/string is not a call.
        let comment = modified(
            "FunctionDeclaration",
            &["public void Open(string url) { Log(url); }"],
            &[
                "public void Open(string url) {",
                "    // never Process.Start(\"explorer\", url) with user input",
                "    Log(url);",
                "}",
            ],
        );
        assert_eq!(hit(&comment, false), None, "marker in comment");
        let string = modified(
            "FunctionDeclaration",
            &["public void Open(string url) { Log(url); }"],
            &[
                "public void Open(string url) {",
                "    Log(\"calls Process.Start internally\");",
                "}",
            ],
        );
        assert_eq!(hit(&string, false), None, "marker in string");
    }

    // ---------------------------------------------------------------
    // loose-regex
    // ---------------------------------------------------------------
    #[test]
    fn loose_regex_fires_on_widened_pattern() {
        let node = modified(
            "VariableDeclaration",
            &["var emailRe = new Regex(\"^[^@]+@[^@]+$\");"],
            &["var emailRe = new Regex(\".*\");"],
        );
        assert_eq!(hit(&node, false), Some("loose-regex"));
    }

    #[test]
    fn loose_regex_negative_pattern_unchanged() {
        let node = modified(
            "VariableDeclaration",
            &["var emailRe = new Regex(\"^[^@]+@[^@]+$\");"],
            &["var addrRe = new Regex(\"^[^@]+@[^@]+$\");"],
        );
        assert_eq!(hit(&node, false), None);
    }

    // ---------------------------------------------------------------
    // cors-permissive
    // ---------------------------------------------------------------
    #[test]
    fn cors_permissive_fires_on_allow_any_origin() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "public void Configure(CorsPolicyBuilder b) {",
                "    b.WithOrigins(\"https://admin.example.com\");",
                "}",
            ],
            &[
                "public void Configure(CorsPolicyBuilder b) {",
                "    b.AllowAnyOrigin();",
                "}",
            ],
        );
        assert_eq!(hit(&node, false), Some("broadened-cors"));
    }

    #[test]
    fn cors_permissive_negative_allowlist_kept() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "public void Configure(CorsPolicyBuilder b) {",
                "    b.WithOrigins(\"https://admin.example.com\");",
                "}",
            ],
            &[
                "public void Configure(CorsPolicyBuilder b) {",
                "    b.WithOrigins(\"https://admin.example.com\", \"https://app.example.com\");",
                "}",
            ],
        );
        assert_eq!(hit(&node, false), None);
    }

    // ---------------------------------------------------------------
    // cookie httponly / secure / samesite
    // ---------------------------------------------------------------
    #[test]
    fn cookie_httponly_removed_fires() {
        let node = modified(
            "VariableDeclaration",
            &[
                "var options = new CookieOptions {",
                "    HttpOnly = true,",
                "};",
            ],
            &["var options = new CookieOptions {", "};"],
        );
        assert_eq!(hit(&node, false), Some("cookie-httponly-removed"));
    }

    #[test]
    fn cookie_secure_removed_fires() {
        let node = modified(
            "VariableDeclaration",
            &[
                "var options = new CookieOptions {",
                "    Secure = true,",
                "};",
            ],
            &["var options = new CookieOptions {", "};"],
        );
        assert_eq!(hit(&node, false), Some("cookie-secure-removed"));
    }

    #[test]
    fn cookie_samesite_weakened_fires() {
        let node = modified(
            "VariableDeclaration",
            &[
                "var options = new CookieOptions {",
                "    SameSite = SameSiteMode.Strict,",
                "};",
            ],
            &[
                "var options = new CookieOptions {",
                "    SameSite = SameSiteMode.None,",
                "};",
            ],
        );
        assert_eq!(hit(&node, false), Some("samesite-weakened"));
    }

    #[test]
    fn cookie_negative_flags_kept() {
        // Reformatting that keeps HttpOnly/Secure must not flag.
        let node = modified(
            "VariableDeclaration",
            &[
                "var options = new CookieOptions {",
                "    HttpOnly = true,",
                "    Secure = true,",
                "};",
            ],
            &[
                "var options = new CookieOptions { HttpOnly = true, Secure = true };",
            ],
        );
        assert_eq!(hit(&node, false), None);
    }

    // ---------------------------------------------------------------
    // C#-shape idiomatic negatives (mapper quirks)
    // ---------------------------------------------------------------
    #[test]
    fn expression_bodied_member_refactor_does_not_flag() {
        // `=> expr` bodies are normal; renaming + arrowizing keeps the call.
        let node = modified(
            "FunctionDeclaration",
            &[
                "public int Total(Cart c) {",
                "    return Sum(c.Items);",
                "}",
            ],
            &["public int Total(Cart c) => Sum(c.Items);"],
        );
        assert_eq!(hit(&node, false), None);
    }

    #[test]
    fn benign_call_reorder_does_not_flag() {
        let node = modified(
            "FunctionDeclaration",
            &[
                "public void Run() {",
                "    Init();",
                "    Process();",
                "}",
            ],
            &[
                "public void Run() {",
                "    Process();",
                "    Init();",
                "}",
            ],
        );
        assert_eq!(hit(&node, false), None);
    }

    #[test]
    fn added_node_with_plain_call_does_not_flag() {
        let node = added(
            "FunctionDeclaration",
            &["public void Ping() {", "    Send();", "}"],
        );
        assert_eq!(hit(&node, false), None);
    }
}
