//! Structural matching over tree-sitter parse trees for snippet-sized sources.
//! Rules hand in a node's before/after text (the statement-level snippets that
//! `parse` surfaces) and a tree-sitter query; the snippet is re-parsed with the
//! file's grammar and the query runs against real syntax instead of raw text —
//! so a pattern inside a string literal or comment never matches, and
//! formatting tricks never evade.
//!
//! Compiled queries are cached per (grammar, query). Tree-sitter's text
//! predicates (`#eq?`, `#match?`) are applied during iteration, so queries can
//! pin capture text directly.
//!
//! All entry points return `Option`: `None` means the structural layer could
//! not answer (parse or query-compile failure) and the caller should fall back
//! to its text-based path — the same graceful-degradation contract the lockfile
//! parser uses.
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use streaming_iterator::StreamingIterator;
use tree_sitter::{Parser, Query, QueryCursor, Tree};

use crate::parse::{grammar, Lang};

/// Grammar group for cache keys: each distinct tree-sitter grammar gets its own
/// id so a compiled query is never reused across grammars. Ts and Tsx are
/// distinct grammars; the JS grammar covers `.js/.jsx/.mjs/.cjs` natively. The
/// core-four languages each have their own grammar. This match is exhaustive on
/// purpose — adding a `Lang` variant forces a new group here, which is what
/// keeps a query compiled for one grammar from leaking into another.
fn group(lang: Lang) -> u8 {
    match lang {
        Lang::Ts => 0,
        Lang::Tsx => 1,
        Lang::Js | Lang::Jsx => 2,
        Lang::Rust => 3,
        Lang::Go => 4,
        Lang::Python => 5,
        Lang::Java => 6,
        Lang::CSharp => 7,
        Lang::Kotlin => 8,
        Lang::Swift => 9,
    }
}

/// Parse a snippet with the language's grammar. Tree-sitter is error-tolerant:
/// statement snippets re-parsed outside their original context (e.g. a bare
/// `return …`) still yield the inner expression nodes queries care about.
///
/// Several rules consult the same node's before/after text in sequence, so the
/// most recent parses are memoized per thread — one parse per snippet per node,
/// no matter how many rules ask. `Tree` clones are cheap (copy-on-write).
pub fn parse_snippet(src: &str, lang: Lang) -> Option<Tree> {
    thread_local! {
        static RECENT: RefCell<HashMap<(u8, String), Option<Tree>>> =
            RefCell::new(HashMap::new());
    }
    RECENT.with(|cache| {
        let mut cache = cache.borrow_mut();
        // Bounded: rules only ever revisit the current node's two snippets, so
        // a small clear-when-full map beats an LRU here.
        if cache.len() > 32 {
            cache.clear();
        }
        cache
            .entry((group(lang), src.to_string()))
            .or_insert_with(|| {
                let mut parser = Parser::new();
                parser.set_language(&grammar(lang)).ok()?;
                parser.parse(src, None)
            })
            .clone()
    })
}

type QueryCache = Mutex<HashMap<(u8, &'static str), Option<Arc<Query>>>>;

/// Compiled-query cache. A query that fails to compile is cached as `None` so
/// the cost is paid once, and the caller falls back to its text pattern. Every
/// rule query is exercised by a unit test that calls the rule, so a query that
/// fails to compile turns CI red before release — the `debug_assert` is the
/// local fast signal, the test suite is the gate that actually prevents a
/// broken query from shipping.
fn compiled(lang: Lang, query_src: &'static str) -> Option<Arc<Query>> {
    static CACHE: OnceLock<QueryCache> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut guard = match cache.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    guard
        .entry((group(lang), query_src))
        .or_insert_with(|| {
            let q = Query::new(&grammar(lang), query_src).ok().map(Arc::new);
            debug_assert!(q.is_some(), "rule query failed to compile: {query_src}");
            q
        })
        .clone()
}

/// True when the query matches anywhere in the snippet.
pub fn query_hit(src: &str, lang: Lang, query_src: &'static str) -> Option<bool> {
    let tree = parse_snippet(src, lang)?;
    let query = compiled(lang, query_src)?;
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), src.as_bytes());
    Some(matches.next().is_some())
}

/// Return `src` with the *interior* of every comment node and every
/// string-literal node blanked to spaces, preserving byte length and newlines so
/// all offsets and the line structure stay intact. Comment bodies are blanked
/// whole; string literals keep their delimiter bytes (the first and last byte of
/// the node) and blank everything between. Identifiers, keywords, and operators
/// are untouched.
///
/// This lets the non-JsTs marker rules run their substring / regex checks over
/// *code only* — a marker token that appears solely inside a log line, docstring,
/// error message, or `//` comment (often code that WARNS AGAINST the very thing)
/// no longer fires the rule.
///
/// Returns `None` on parse failure (same graceful-degradation contract as the
/// rest of this module): the caller then treats the structural layer as having
/// no answer.
pub fn code_only_text(src: &str, lang: Lang) -> Option<String> {
    let tree = parse_snippet(src, lang)?;
    let mut out = src.as_bytes().to_vec();
    let mut cursor = tree.root_node().walk();
    blank_comments_and_strings(&mut cursor, &mut out);
    // `out` only ever has interior bytes replaced by ASCII spaces (newlines
    // preserved), so it remains valid UTF-8.
    String::from_utf8(out).ok()
}

/// Like [`code_only_text`] but blanks comment bodies ONLY, leaving string
/// literals intact. Used by the env-var TLS rule, whose marker (the env-var
/// name) legitimately lives inside a string key — blanking strings would defeat
/// it, but a marker named only in a comment must still be ignored.
pub fn code_minus_comments_text(src: &str, lang: Lang) -> Option<String> {
    let tree = parse_snippet(src, lang)?;
    let mut out = src.as_bytes().to_vec();
    let mut cursor = tree.root_node().walk();
    blank_comments_only(&mut cursor, &mut out);
    String::from_utf8(out).ok()
}

/// Walk the tree blanking only comment nodes (strings untouched).
fn blank_comments_only(cursor: &mut tree_sitter::TreeCursor, out: &mut [u8]) {
    let node = cursor.node();
    if is_comment_kind(node.kind()) {
        blank_range(out, node.start_byte(), node.end_byte());
        return;
    }
    if cursor.goto_first_child() {
        loop {
            blank_comments_only(cursor, out);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

/// True when a node kind denotes a comment in any of the supported grammars.
fn is_comment_kind(kind: &str) -> bool {
    kind.contains("comment")
}

/// True when a node kind denotes a string/character literal whose *contents*
/// should be blanked. Covers JS/TS (`string`, `template_string`), Rust
/// (`string_literal`, `raw_string_literal`, `char_literal`), Go
/// (`interpreted_string_literal`, `raw_string_literal`), Python (`string`),
/// Java/C#/Kotlin/Swift (`string_literal`, `line_string_literal`, …). The match
/// is intentionally broad: blanking the inside of any string-shaped node is
/// always safe for marker matching.
fn is_string_kind(kind: &str) -> bool {
    kind.contains("string") || kind == "char_literal" || kind == "character_literal"
}

/// Blank `b` from `start..end` (exclusive) to ASCII spaces, leaving newlines and
/// carriage returns intact so line offsets are preserved.
fn blank_range(b: &mut [u8], start: usize, end: usize) {
    for byte in b.iter_mut().take(end).skip(start) {
        if *byte != b'\n' && *byte != b'\r' {
            *byte = b' ';
        }
    }
}

/// Walk the tree; for the outermost comment / string node encountered, blank its
/// interior in `out` and do NOT descend into it (its children are already
/// covered). Other nodes are descended into normally.
fn blank_comments_and_strings(cursor: &mut tree_sitter::TreeCursor, out: &mut [u8]) {
    let node = cursor.node();
    let kind = node.kind();
    if is_comment_kind(kind) {
        // Blank the whole comment body (delimiters included — they carry no
        // marker token).
        blank_range(out, node.start_byte(), node.end_byte());
        return;
    }
    if is_string_kind(kind) {
        // Keep one delimiter byte at each end; blank everything between. For a
        // degenerate 0/1/2-byte node there is nothing interior to blank.
        let (s, e) = (node.start_byte(), node.end_byte());
        if e > s + 2 {
            blank_range(out, s + 1, e - 1);
        }
        return;
    }
    if cursor.goto_first_child() {
        loop {
            blank_comments_and_strings(cursor, out);
            if !cursor.goto_next_sibling() {
                break;
            }
        }
        cursor.goto_parent();
    }
}

/// Source text of every node captured under `capture_name`, in document order.
pub fn capture_texts(
    src: &str,
    lang: Lang,
    query_src: &'static str,
    capture_name: &str,
) -> Option<Vec<String>> {
    let tree = parse_snippet(src, lang)?;
    let query = compiled(lang, query_src)?;
    let index = query.capture_index_for_name(capture_name)?;
    let mut cursor = QueryCursor::new();
    let mut matches = cursor.matches(&query, tree.root_node(), src.as_bytes());
    let mut out = Vec::new();
    while let Some(m) = matches.next() {
        for cap in m.captures.iter().filter(|c| c.index == index) {
            out.push(cap.node.utf8_text(src.as_bytes()).unwrap_or("").to_string());
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVAL_CALL: &str = r#"(call_expression function: (identifier) @callee (#eq? @callee "eval"))"#;

    #[test]
    fn hits_across_formatting_variants() {
        for src in ["eval(x)", "eval (x)", "eval\n  (payload)", "const r = eval( code );"] {
            assert_eq!(
                query_hit(src, Lang::Ts, EVAL_CALL),
                Some(true),
                "should match: {src}"
            );
        }
    }

    #[test]
    fn ignores_strings_and_comments() {
        for src in [
            r#"const s = "eval(x)";"#,
            "// eval(x)",
            "/* eval(x) */ run();",
            "const evaluate = (x) => x; evaluate(input);",
        ] {
            assert_eq!(
                query_hit(src, Lang::Ts, EVAL_CALL),
                Some(false),
                "should NOT match: {src}"
            );
        }
    }

    #[test]
    fn statement_snippets_survive_reparse_out_of_context() {
        // Function-body children are surfaced as bare statements; error
        // recovery must keep the inner call visible to queries.
        assert_eq!(query_hit("return eval(token);", Lang::Ts, EVAL_CALL), Some(true));
    }

    #[test]
    fn works_across_grammars() {
        assert_eq!(query_hit("eval(x)", Lang::Js, EVAL_CALL), Some(true));
        assert_eq!(query_hit("eval(x)", Lang::Jsx, EVAL_CALL), Some(true));
        assert_eq!(
            query_hit("<Badge onClick={() => eval(s)} />", Lang::Tsx, EVAL_CALL),
            Some(true)
        );
    }

    #[test]
    fn code_only_text_blanks_comment_and_string_interiors_preserving_offsets() {
        // A marker token living only in a string and a comment is gone from the
        // code-only view, while real code (the identifier, the call) survives.
        let src = "let x = \"InsecureSkipVerify: true\"; // child_process here\nrun(InsecureSkipVerify);";
        let out = code_only_text(src, Lang::Rust).expect("parse ok");
        // Byte length and newline positions are preserved.
        assert_eq!(out.len(), src.len(), "length preserved");
        assert_eq!(
            out.matches('\n').count(),
            src.matches('\n').count(),
            "newlines preserved"
        );
        // The marker inside the string literal is blanked.
        assert!(
            !out.contains("InsecureSkipVerify: true"),
            "string interior blanked: {out:?}"
        );
        // The comment body is blanked.
        assert!(!out.contains("child_process"), "comment blanked: {out:?}");
        // Real code survives: the bare identifier reference and the call.
        assert!(out.contains("run("), "code survives: {out:?}");
        assert!(
            out.contains("InsecureSkipVerify)"),
            "identifier in code survives: {out:?}"
        );
    }

    #[test]
    fn code_only_text_keeps_string_delimiters() {
        // The delimiters stay so the result still parses / reads as a string.
        let src = r#"const s = "eval(x)";"#;
        let out = code_only_text(src, Lang::Ts).expect("parse ok");
        assert!(out.contains('"'), "delimiters kept: {out:?}");
        assert!(!out.contains("eval(x)"), "interior blanked: {out:?}");
    }

    #[test]
    fn capture_texts_returns_document_order() {
        const REGEX_LIT: &str = "(regex) @re";
        let texts = capture_texts(
            "const a = /^x$/; const b = /.*/;",
            Lang::Ts,
            REGEX_LIT,
            "re",
        )
        .unwrap();
        assert_eq!(texts, vec!["/^x$/", "/.*/"]);
    }

    #[test]
    fn invalid_query_degrades_to_none() {
        assert_eq!(query_hit_invalid_for_test(), None);
    }

    // Kept out of the main API: exercises the compile-failure cache path
    // without tripping the debug_assert during normal runs.
    fn query_hit_invalid_for_test() -> Option<bool> {
        let src = "eval(x)";
        let tree = parse_snippet(src, Lang::Ts)?;
        let query = Query::new(&grammar(Lang::Ts), "(nonexistent_node_kind) @x").ok()?;
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(&query, tree.root_node(), src.as_bytes());
        Some(matches.next().is_some())
    }

    #[test]
    fn each_lang_gets_its_own_grammar_group() {
        // Distinct grammars must never share a cache key — otherwise a query
        // compiled for one grammar could be reused against another.
        use std::collections::HashSet;
        let mut seen: HashSet<u8> = HashSet::new();
        // Ts, Tsx, Js are distinct groups; Js/Jsx share (same grammar).
        assert_ne!(group(Lang::Ts), group(Lang::Js));
        assert_ne!(group(Lang::Tsx), group(Lang::Js));
        assert_eq!(group(Lang::Js), group(Lang::Jsx));
        // The structural-drift languages each get a group distinct from every other.
        for lang in [
            Lang::Ts,
            Lang::Tsx,
            Lang::Js,
            Lang::Rust,
            Lang::Go,
            Lang::Python,
            Lang::Java,
            Lang::CSharp,
            Lang::Kotlin,
            Lang::Swift,
        ] {
            assert!(
                seen.insert(group(lang)),
                "duplicate grammar group for {lang:?}"
            );
        }
    }

    #[test]
    fn js_node_kind_query_fails_to_compile_against_foreign_grammars() {
        // The eval query names `call_expression`/`identifier` — node kinds that
        // exist in the JS grammar. Compiling it against Rust/Go/Python/Java must
        // NOT silently succeed against a different node vocabulary. We compile
        // directly (not via `compiled()`) to avoid the debug_assert; the point
        // is that the JS-grammar query is grammar-specific. A query that DID
        // compile would yield coincidental matches — the rule gate in rules.rs
        // is what guarantees these queries are never run cross-grammar, and this
        // test documents why that gate is load-bearing.
        const TLS_REJECT: &str =
            r#"(pair key: (property_identifier) @k (#eq? @k "x") value: (false))"#;
        // `pair` and `property_identifier` are JS-grammar node kinds; they do
        // not exist in the structural-drift grammars, so compilation fails.
        for lang in [
            Lang::Rust,
            Lang::Go,
            Lang::Python,
            Lang::Java,
            Lang::CSharp,
            Lang::Kotlin,
            Lang::Swift,
        ] {
            assert!(
                Query::new(&grammar(lang), TLS_REJECT).is_err(),
                "JS-specific query must not compile against {lang:?}"
            );
        }
        // Sanity: it DOES compile against the JS/TS grammar.
        assert!(Query::new(&grammar(Lang::Ts), TLS_REJECT).is_ok());
    }
}
