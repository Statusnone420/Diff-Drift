//! Per-family language pack: the data the security rules consult to run against
//! a non-JS/TS grammar without hard-coding JS node kinds or JS text regexes.
//!
//! Each `Rule` declares the families it applies to (`Rule::families`). Inside a
//! rule's `check`, the JS/TS path keeps its exact historical behavior (its own
//! consts/regexes/fallbacks); for every other family the rule consults
//! [`pack`]`(family)` and runs **structurally or not at all** — no JS text-regex
//! fallback ever fires outside JS/TS, and an empty pack field makes the rule
//! return `None` silently.
//!
//! # Design contract
//!
//! Every field is optional / default-empty, so a family file that fills in
//! nothing still produces a valid, fully-silent pack. Family agents fill the
//! fields they have grounded queries + tests for; an unfilled field is the
//! honest "this cell is cut" signal — the rule simply stays quiet.
//!
//! ## Tree-sitter query fields and capture-name convention
//!
//! The query string fields each carry a single, fixed capture name so the rule
//! layer can pull the captured text without per-family knowledge:
//!
//! | Field            | Capture        | What it captures                          |
//! |------------------|----------------|-------------------------------------------|
//! | `if_condition`   | `@cond`        | the condition expression of an `if`       |
//! | `if_consequence` | `@cons`        | the consequence/body block of an `if`     |
//! | `callee`         | `@callee`      | every call's callee name (bare + member)  |
//! | `try_block`      | `@try`         | a try/catch-equivalent construct          |
//!
//! A field set to `None` means "this family has no grounded query for that
//! concept" — the consuming rule treats `None` exactly like an empty match and
//! returns `None`. Marker tables (`Option<&[&str]>`) and regex-source fields
//! (`Option<&str>`) follow the same rule: `None` => the rule is silent.
//!
//! ## `error_handling_strategy`
//!
//! [`ErrorHandlingRemoved`](crate::rules) needs a per-family notion of "the
//! error handling that could be removed". The strategies are:
//!
//! - [`ErrorHandlingStrategy::TryBlock`] — the family has try/catch; the rule
//!   uses `try_block` to detect a wrapping construct that vanished while its
//!   calls survived. (Python, Java, C#, Kotlin, Swift.)
//! - [`ErrorHandlingStrategy::UnwrapTransition`] — the family expresses fallible
//!   results without try/catch (Rust's `?`/`match` on `Result`); the rule
//!   detects a modified node whose *before* handled the error (contained `?` or
//!   a `match`) and whose *after* replaced it with `.unwrap()`/`.expect(...)`.
//!   The mechanism lives in the rule now; the Rust agent supplies the markers
//!   (`unwrap_markers` / `handled_markers`) and tests.
//! - [`ErrorHandlingStrategy::CoveredByGuards`] — the family's error handling is
//!   already surfaced by the guard rules (Go's `if err != nil { return … }`),
//!   so `ErrorHandlingRemoved` does not run; it returns `None`.
//! - [`ErrorHandlingStrategy::None`] — the rule does not apply to this family.
//!
//! # Two short examples for family agents
//!
//! ## Example A — fill a query field, write a positive unit test
//!
//! In `lang/python.rs` (illustrative — the real Python pack lives there):
//!
//! ```ignore
//! pub(super) static PACK: FamilyPack = FamilyPack {
//!     if_condition: Some("(if_statement condition: (_) @cond)"),
//!     falsy_literals: &["False", "None", "0"],
//!     ..FamilyPack::EMPTY
//! };
//! ```
//!
//! Then, inside `lang/python.rs`'s own `#[cfg(test)]` module, drive a real
//! `AstNode` through the registry:
//!
//! ```ignore
//! #[test]
//! fn python_removed_if_guard_fires_on_constant_false() {
//!     use crate::model::{AstNode, NodeState};
//!     use crate::parse::Lang;
//!     use crate::rules::{registry, RuleCtx};
//!     use std::collections::HashSet;
//!
//!     let node = AstNode {
//!         id: "t".into(), kind: "IfStatement".into(), name: "if".into(),
//!         signature: None, state: NodeState::Modified, reviewed: false,
//!         flag_id: None,
//!         before: Some(vec!["if is_admin(user):".into(), "    audit()".into()]),
//!         after: Some(vec!["if False:".into(), "    audit()".into()]),
//!         children: None,
//!     };
//!     let ctx = RuleCtx { deps: HashSet::new(), is_test_file: false, lang: Lang::Python };
//!     let hit = registry().check(&node, &ctx);
//!     assert_eq!(hit.map(|(id, _)| id), Some("removed-if-guard"));
//! }
//! ```
//!
//! ## Example B — an idiomatic-NEGATIVE test (normal code must NOT flag)
//!
//! ```ignore
//! #[test]
//! fn python_if_expression_is_not_a_removed_guard() {
//!     use crate::model::{AstNode, NodeState};
//!     use crate::parse::Lang;
//!     use crate::rules::{registry, RuleCtx};
//!     use std::collections::HashSet;
//!
//!     // A normal refactor that keeps a live condition must not flag.
//!     let node = AstNode {
//!         id: "t".into(), kind: "IfStatement".into(), name: "if".into(),
//!         signature: None, state: NodeState::Modified, reviewed: false,
//!         flag_id: None,
//!         before: Some(vec!["if ok:".into(), "    run()".into()]),
//!         after: Some(vec!["if ok and ready:".into(), "    run()".into()]),
//!         children: None,
//!     };
//!     let ctx = RuleCtx { deps: HashSet::new(), is_test_file: false, lang: Lang::Python };
//!     assert!(registry().check(&node, &ctx).is_none());
//! }
//! ```

use crate::parse::Family;

/// How [`ErrorHandlingRemoved`](crate::rules) reasons about a family. See the
/// module docs for the meaning of each variant.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ErrorHandlingStrategy {
    /// try/catch-style: use `try_block` to find a wrapper that disappeared.
    TryBlock,
    /// Rust-style: `?`/`match`-on-`Result` in the before became `.unwrap()`/
    /// `.expect(...)` in the after.
    UnwrapTransition,
    /// Error handling is already surfaced by the guard rules — do not run.
    CoveredByGuards,
    /// The rule does not apply to this family.
    None,
}

/// The per-family contract the security rules consult. Every field is optional
/// or default-empty; the [`FamilyPack::EMPTY`] base lets a family file fill in
/// only what it has grounded + tested.
pub struct FamilyPack {
    // ---- structural query strings (single fixed capture name each) ----
    /// `@cond` — an `if` condition expression. Used by `RemovedIfGuard`.
    pub if_condition: Option<&'static str>,
    /// `@cons` — an `if` consequence/body block. Used by `GuardRemoved`.
    pub if_consequence: Option<&'static str>,
    /// `@callee` — every call's callee name (bare and member/selector forms).
    /// Used by `GuardRemoved`, `RemovedSanitize`, `VerifyToDecode`,
    /// `ErrorHandlingRemoved`.
    pub callee: Option<&'static str>,
    /// `@try` — a try/catch-equivalent construct. Used by
    /// `ErrorHandlingRemoved` when `error_handling_strategy == TryBlock`.
    pub try_block: Option<&'static str>,

    // ---- literal tables ----
    /// Condition texts that count as constant-falsy for `RemovedIfGuard`
    /// (e.g. `["False", "None", "0"]` for Python).
    pub falsy_literals: &'static [&'static str],
    /// Early-exit keywords that mark an `if` consequence as a guard clause for
    /// `GuardRemoved`'s early-return-refactor suppression (e.g. `["return",
    /// "throw", "break", "continue"]`).
    pub guard_exit_keywords: &'static [&'static str],

    // ---- error-handling strategy ----
    /// How `ErrorHandlingRemoved` reasons about this family.
    pub error_handling_strategy: ErrorHandlingStrategy,
    /// `UnwrapTransition` only: substrings whose appearance in the *after* text
    /// marks an unhandled unwrap (e.g. `[".unwrap(", ".expect("]`).
    pub unwrap_markers: &'static [&'static str],
    /// `UnwrapTransition` only: substrings whose presence in the *before* text
    /// marks prior error handling that the unwrap replaced (e.g. `["?", "match"]`).
    pub handled_markers: &'static [&'static str],

    // ---- marker tables / regex sources for the High-severity rules ----
    // (filled by family agents; `None` => the corresponding rule is silent.)
    /// Regex source matching a subprocess-module *import* (e.g. Python
    /// `import subprocess`). Used by `ChildProcess`.
    pub subprocess_import: Option<&'static str>,
    /// Regex source matching a subprocess *call* (e.g. `subprocess.run(`,
    /// `os.system(`). Used by `ChildProcess`.
    pub subprocess_call: Option<&'static str>,
    /// Regex source matching a TLS-verification-disabling marker (e.g.
    /// `verify\s*=\s*False`, `InsecureSkipVerify:\s*true`). Used by
    /// `TlsRejectFalse`.
    pub tls_disable: Option<&'static str>,
    /// Regex source matching an env-var assignment that disables TLS (e.g.
    /// `PYTHONHTTPSVERIFY\s*=\s*['"]?0`). Used by `EnvTlsReject`.
    pub env_tls_disable: Option<&'static str>,
    /// Regex source matching a real `eval`/`exec`/`compile`-style dynamic-code
    /// call. Used by `EvalCall`.
    pub eval_call: Option<&'static str>,
    /// Regex source matching the call whose string argument is a regex pattern
    /// (e.g. `re\.compile`, `Regex::new`, `Pattern\.compile`). Used by
    /// `LooseRegex` to locate the family's regex-construction call.
    pub regex_compile: Option<&'static str>,
    /// Regex source matching a permissive-CORS marker (origin opened to `*` /
    /// any). Used by `BroadenedCors`.
    pub cors_permissive: Option<&'static str>,
    /// Regex source matching a cookie `HttpOnly`-enabled marker; the rule fires
    /// when this matched the before and not the after. Used by
    /// `CookieHttpOnlyRemoved`.
    pub cookie_httponly: Option<&'static str>,
    /// Regex source matching a cookie `Secure`-enabled marker. Used by
    /// `CookieSecureRemoved`.
    pub cookie_secure: Option<&'static str>,
    /// Regex source matching a cookie `SameSite`-strong marker. Used by
    /// `SameSiteWeakened` (paired with `cookie_samesite_weak`).
    pub cookie_samesite: Option<&'static str>,
    /// Regex source matching a cookie `SameSite=None`/weak marker. Used by
    /// `SameSiteWeakened` (paired with `cookie_samesite`).
    pub cookie_samesite_weak: Option<&'static str>,
}

impl FamilyPack {
    /// A fully-empty pack: every query `None`, every table empty, error handling
    /// not applicable. Family files spread this with `..FamilyPack::EMPTY` and
    /// override only the fields they fill.
    pub const EMPTY: FamilyPack = FamilyPack {
        if_condition: None,
        if_consequence: None,
        callee: None,
        try_block: None,
        falsy_literals: &[],
        guard_exit_keywords: &[],
        error_handling_strategy: ErrorHandlingStrategy::None,
        unwrap_markers: &[],
        handled_markers: &[],
        subprocess_import: None,
        subprocess_call: None,
        tls_disable: None,
        env_tls_disable: None,
        eval_call: None,
        regex_compile: None,
        cors_permissive: None,
        cookie_httponly: None,
        cookie_secure: None,
        cookie_samesite: None,
        cookie_samesite_weak: None,
    };
}

pub mod csharp;
pub mod go;
pub mod java;
pub mod kotlin;
pub mod python;
pub mod rust;
pub mod swift;

/// The static pack for a family. `JsTs` returns the empty pack: the JS/TS rule
/// paths never consult `pack` — they keep their own consts/regexes — so a JsTs
/// caller reaching here would (correctly) see no markers. Every other family
/// dispatches to its own module's static.
pub fn pack(f: Family) -> &'static FamilyPack {
    match f {
        Family::JsTs => &JS_TS_EMPTY,
        Family::Rust => &rust::PACK,
        Family::Go => &go::PACK,
        Family::Python => &python::PACK,
        Family::Java => &java::PACK,
        Family::CSharp => &csharp::PACK,
        Family::Kotlin => &kotlin::PACK,
        Family::Swift => &swift::PACK,
    }
}

/// JS/TS never reads its pack (its rules carry their own logic). A real empty
/// pack is returned for completeness so `pack(Family::JsTs)` is total.
static JS_TS_EMPTY: FamilyPack = FamilyPack::EMPTY;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pack_dispatch_returns_the_right_static_per_family() {
        // Each family resolves to its own module's static — pointer identity
        // proves the dispatch isn't accidentally returning one shared pack.
        assert!(std::ptr::eq(pack(Family::Rust), &rust::PACK));
        assert!(std::ptr::eq(pack(Family::Go), &go::PACK));
        assert!(std::ptr::eq(pack(Family::Python), &python::PACK));
        assert!(std::ptr::eq(pack(Family::Java), &java::PACK));
        assert!(std::ptr::eq(pack(Family::CSharp), &csharp::PACK));
        assert!(std::ptr::eq(pack(Family::Kotlin), &kotlin::PACK));
        assert!(std::ptr::eq(pack(Family::Swift), &swift::PACK));
        assert!(std::ptr::eq(pack(Family::JsTs), &JS_TS_EMPTY));
    }

    #[test]
    fn empty_pack_is_silent_everywhere() {
        // The EMPTY base has no queries and no markers, so a rule consulting it
        // has nothing to match. Family skeletons start from this — until a field
        // is filled, the corresponding rule stays quiet.
        let p = &FamilyPack::EMPTY;
        assert!(p.if_condition.is_none());
        assert!(p.if_consequence.is_none());
        assert!(p.callee.is_none());
        assert!(p.try_block.is_none());
        assert!(p.falsy_literals.is_empty());
        assert!(p.guard_exit_keywords.is_empty());
        assert_eq!(p.error_handling_strategy, ErrorHandlingStrategy::None);
        assert!(p.subprocess_import.is_none());
        assert!(p.eval_call.is_none());
        assert!(p.cors_permissive.is_none());
    }
}
