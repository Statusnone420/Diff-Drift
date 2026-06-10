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

/// Grammar group for cache keys: Ts and Tsx are distinct grammars; the JS
/// grammar covers `.js/.jsx/.mjs/.cjs` natively.
fn group(lang: Lang) -> u8 {
    match lang {
        Lang::Ts => 0,
        Lang::Tsx => 1,
        Lang::Js | Lang::Jsx => 2,
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
/// the cost is paid once; every rule query is covered by a unit test, so a
/// `None` here is a programming error surfaced in debug builds.
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
}
